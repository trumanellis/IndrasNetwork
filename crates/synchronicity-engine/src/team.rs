//! Device-local team binding types.
//!
//! Synced team types ([`LogicalAgentId`], [`Team`]) live in
//! `indras_sync_engine::team` and are embedded in the vault document so they
//! replicate across devices. This module holds the **device-local** pieces:
//!
//! - [`FolderBinding`] — one logical agent mapped to an on-disk folder.
//! - [`TeamBindingRegistry`] — the full set of bindings this device hosts.
//! - [`DeviceTeamMembership`] — computed view of which team roster members
//!   this device hosts, used to decide whether to join the team realm.
//!
//! Persistence (load/save of the registry) lives in a later subtask; this
//! module defines only the in-memory types.

use indras_network::IndrasNetwork;
use indras_storage::BlobStore;
use indras_sync_engine::realm_team::RealmTeam;
use indras_sync_engine::realm_vault::RealmVault;
use indras_sync_engine::team::{LogicalAgentId, Team};
use indras_sync_engine::workspace::{FolderLock, LocalWorkspaceIndex, WorkspaceWatcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::vault_manager::VaultManager;

/// Subfolder-name prefix used to auto-detect agent worktrees inside a
/// syncengine-managed vault. Any directory entry whose name starts with
/// this prefix is bound as a logical agent, using the folder name as
/// the agent id.
const AGENT_FOLDER_PREFIX: &str = "agent";

/// A device-local binding of a logical agent to a filesystem folder.
///
/// The folder is what the AI agent edits; the syncengine mirrors edits from
/// the folder into the team realm's braid DAG on the agent's behalf.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderBinding {
    /// Which logical agent owns edits from this folder.
    pub agent: LogicalAgentId,
    /// Absolute path to the folder on this device.
    pub folder: PathBuf,
}

impl FolderBinding {
    /// Build a new binding from a logical agent and an absolute folder path.
    pub fn new(agent: LogicalAgentId, folder: PathBuf) -> Self {
        Self { agent, folder }
    }
}

/// Device-local map from logical agent id to bound folder path.
///
/// Persisted as JSON at `{data_dir}/team_bindings.json`. Load/save logic
/// lives in subtask 0.7; this type just models the in-memory shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamBindingRegistry {
    /// All agent → folder bindings this device hosts, flattened across teams.
    /// An agent id is unique within this registry — a device can only host
    /// a given logical agent in one folder at a time.
    pub bindings: HashMap<LogicalAgentId, PathBuf>,
}

impl TeamBindingRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace the binding for an agent.
    pub fn bind(&mut self, agent: LogicalAgentId, folder: PathBuf) {
        self.bindings.insert(agent, folder);
    }

    /// Remove the binding for an agent, if any. Returns the previous path.
    pub fn unbind(&mut self, agent: &LogicalAgentId) -> Option<PathBuf> {
        self.bindings.remove(agent)
    }

    /// Look up the folder path for a given agent.
    pub fn folder_for(&self, agent: &LogicalAgentId) -> Option<&PathBuf> {
        self.bindings.get(agent)
    }

    /// Compute the subset of a team's roster this device hosts.
    pub fn membership_for(&self, team: &Team) -> DeviceTeamMembership {
        let hosted = team
            .roster
            .iter()
            .filter_map(|agent| {
                self.bindings
                    .get(agent)
                    .map(|path| (agent.clone(), path.clone()))
            })
            .collect();
        DeviceTeamMembership { hosted }
    }

    /// Discover bindings by scanning every vault managed by
    /// `vault_manager` for subdirectories whose name begins with
    /// [`AGENT_FOLDER_PREFIX`] (`agent`). Each such subdirectory is
    /// bound as a logical agent whose id is the folder name.
    ///
    /// Convention over configuration: a folder named `agent1` inside a
    /// managed vault becomes the binding for logical agent `agent1`.
    /// No JSON file, no env var, no UI. Drop a new `agent*` folder into
    /// a vault and it's picked up on the next startup scan.
    pub async fn discover_from(vault_manager: &VaultManager) -> Self {
        let mut bindings: HashMap<LogicalAgentId, PathBuf> = HashMap::new();
        for realm in vault_manager.realms().await {
            let rid = *realm.id().as_bytes();
            let Some(vault_path) = vault_manager.vault_path(&rid) else {
                continue;
            };
            let mut entries = match tokio::fs::read_dir(&vault_path).await {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!(
                        path = %vault_path.display(),
                        error = %e,
                        "discover_from: read_dir failed"
                    );
                    continue;
                }
            };
            loop {
                let entry = match entries.next_entry().await {
                    Ok(Some(entry)) => entry,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::debug!(error = %e, "discover_from: next_entry failed");
                        break;
                    }
                };
                let Ok(ft) = entry.file_type().await else { continue };
                if !ft.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with(AGENT_FOLDER_PREFIX) {
                    continue;
                }
                bindings.insert(LogicalAgentId::new(name), entry.path());
            }
        }
        Self { bindings }
    }
}

/// The subset of a team's roster actually hosted on this device, with folders.
///
/// Derived from [`Team`] + [`TeamBindingRegistry`]. Used to decide whether
/// this device should join the team realm (non-empty `hosted` ⇒ join).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceTeamMembership {
    /// Logical agents this device hosts, with their bound folders.
    pub hosted: HashMap<LogicalAgentId, PathBuf>,
}

impl DeviceTeamMembership {
    /// Whether the device hosts at least one agent for the team.
    pub fn is_participating(&self) -> bool {
        !self.hosted.is_empty()
    }
}

/// Live working-tree plumbing for one bound agent folder.
///
/// Bundles the things that must stay alive together: the OS-level
/// single-writer lock on the folder, the background fs-watcher, and the
/// index it populates. Drop the handle to stop watching and release the
/// lock.
pub struct WorkspaceHandle {
    /// Logical agent this folder is bound to.
    pub agent: LogicalAgentId,
    /// Held to prevent a second syncengine from mirroring the same folder.
    pub lock: FolderLock,
    /// Background watcher keeping `index` in sync with disk.
    pub watcher: WorkspaceWatcher,
    /// The living working-tree index for this agent.
    pub index: Arc<LocalWorkspaceIndex>,
}

/// For each binding in `registry`, acquire a folder lock, populate an
/// initial index of the folder's current content, and start an
/// [`WorkspaceWatcher`]. Returns one [`WorkspaceHandle`] per successfully
/// bound folder. Failures (lock held by another process, inaccessible
/// folder) are logged and skipped — one bad binding must not block
/// healthy ones.
pub async fn spawn_workspace_watchers(
    registry: &TeamBindingRegistry,
    blob_store: Arc<BlobStore>,
) -> Vec<WorkspaceHandle> {
    let mut handles = Vec::new();
    for (agent, folder) in &registry.bindings {
        let lock = match FolderLock::acquire(folder) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(
                    folder = %folder.display(),
                    error = %e,
                    "skipping binding: folder lock unavailable"
                );
                continue;
            }
        };
        let index = Arc::new(LocalWorkspaceIndex::new(
            folder.clone(),
            Arc::clone(&blob_store),
        ));
        if let Err(e) = index.initial_scan().await {
            tracing::warn!(
                folder = %folder.display(),
                error = %e,
                "initial scan failed; proceeding with empty index"
            );
        }
        match WorkspaceWatcher::start(Arc::clone(&index)) {
            Ok(watcher) => {
                handles.push(WorkspaceHandle {
                    agent: agent.clone(),
                    lock,
                    watcher,
                    index,
                });
            }
            Err(e) => {
                tracing::warn!(
                    folder = %folder.display(),
                    error = %e,
                    "watcher start failed; dropping lock"
                );
                drop(lock);
            }
        }
    }
    handles
}

/// Persisted HEAD state on disk — a plain JSON file at
/// `{vault_path}/.braid-head.json`. Survives restarts reliably without
/// depending on the Document event-replay pipeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedHead {
    change_id: indras_sync_engine::braid::ChangeId,
    manifest: indras_sync_engine::braid::PatchManifest,
}

/// Read the persisted HEAD for a vault. Returns `None` if the file
/// doesn't exist or can't be parsed.
pub fn load_persisted_head(
    vault_path: &std::path::Path,
) -> Option<indras_sync_engine::braid::ChangeId> {
    let path = vault_path.join(".braid-head.json");
    let data = std::fs::read_to_string(&path).ok()?;
    let head: PersistedHead = serde_json::from_str(&data).ok()?;
    Some(head.change_id)
}

/// After a successful commit, persist HEAD to disk and materialize
/// the committed files to the vault root so they appear in the vault
/// column + sync to other devices via the existing CRDT pipeline.
///
/// Two steps:
/// 1. Write HEAD (ChangeId + PatchManifest) to a plain JSON file at
///    `{vault_path}/.braid-head.json` — survives restarts.
/// 2. Write each file in the manifest from the blob store to the vault
///    root on disk; the vault's `VaultWatcher` picks up the writes and
///    syncs the `VaultFileDocument`.
pub async fn publish_and_materialize_head(
    vault_manager: &VaultManager,
    vault_realm: &indras_network::Realm,
    change_id: indras_sync_engine::braid::ChangeId,
    manifest: &indras_sync_engine::braid::PatchManifest,
) {
    let rid = *vault_realm.id().as_bytes();
    let Some(vault_path) = vault_manager.vault_path(&rid) else {
        tracing::warn!("vault path not found; skipping HEAD publish");
        return;
    };

    // 1. Persist HEAD to a plain JSON file.
    let head = PersistedHead {
        change_id,
        manifest: manifest.clone(),
    };
    let head_path = vault_path.join(".braid-head.json");
    match serde_json::to_string_pretty(&head) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&head_path, json).await {
                tracing::warn!(error = %e, "failed to write .braid-head.json");
            }
        }
        Err(e) => tracing::warn!(error = %e, "failed to serialize HEAD"),
    }

    // 2. Materialize files to the vault root on disk.
    let blob = vault_manager.blob_store();
    for file in &manifest.files {
        let content_ref = indras_storage::ContentRef::new(file.hash, file.size);
        match blob.load(&content_ref).await {
            Ok(bytes) => {
                let target = vault_path.join(&file.path);
                if let Some(parent) = target.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                if let Err(e) = tokio::fs::write(&target, &bytes).await {
                    tracing::warn!(path = %file.path, error = %e, "materialize to vault failed");
                }
            }
            Err(e) => {
                tracing::warn!(path = %file.path, error = %e, "blob load for materialize failed");
            }
        }
    }
}

/// Materialize the team realm for every currently-tracked synced vault on
/// this device that has a declared team this device participates in.
///
/// Iterates the vault manager's known realms, reads each vault's
/// [`Team`] from its vault-index document, and — if this device hosts
/// at least one of the team's roster members — calls
/// [`RealmTeam::ensure_team_realm`] to join (or create) the team realm.
/// Vaults without a team, or where this device hosts no agent, are
/// skipped silently. Errors are logged and do not abort the loop; a
/// single bad vault must not prevent the others from materializing.
pub async fn ensure_team_realms_for_hosted_vaults(
    network: &IndrasNetwork,
    vault_manager: &VaultManager,
    registry: &TeamBindingRegistry,
) {
    for realm in vault_manager.realms().await {
        let idx = match realm.vault_index().await {
            Ok(idx) => idx,
            Err(e) => {
                tracing::debug!(error = %e, "vault_index fetch failed while ensuring team realms");
                continue;
            }
        };
        let team = idx.read().await.team.clone();
        if !registry.membership_for(&team).is_participating() {
            continue;
        }
        if let Err(e) = realm.ensure_team_realm(network, "team-realm").await {
            tracing::warn!(error = %e, "ensure_team_realm failed for a hosted vault");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str) -> LogicalAgentId {
        LogicalAgentId::new(name)
    }

    #[test]
    fn registry_bind_and_lookup() {
        let mut reg = TeamBindingRegistry::new();
        reg.bind(agent("a"), PathBuf::from("/tmp/a"));
        assert_eq!(reg.folder_for(&agent("a")), Some(&PathBuf::from("/tmp/a")));
        assert_eq!(reg.folder_for(&agent("b")), None);
    }

    #[test]
    fn membership_intersects_roster_with_bindings() {
        let mut reg = TeamBindingRegistry::new();
        reg.bind(agent("a"), PathBuf::from("/tmp/a"));
        reg.bind(agent("unrelated"), PathBuf::from("/tmp/other"));

        let team = Team {
            roster: vec![agent("a"), agent("b")],
            ..Default::default()
        };
        let membership = reg.membership_for(&team);
        assert_eq!(membership.hosted.len(), 1);
        assert!(membership.hosted.contains_key(&agent("a")));
        assert!(!membership.hosted.contains_key(&agent("b")));
        assert!(!membership.hosted.contains_key(&agent("unrelated")));
        assert!(membership.is_participating());
    }

    #[test]
    fn empty_membership_not_participating() {
        let reg = TeamBindingRegistry::new();
        let team = Team::empty();
        assert!(!reg.membership_for(&team).is_participating());
    }

}
