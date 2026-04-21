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
use indras_sync_engine::braid::changeset::Evidence;
use indras_sync_engine::braid::ChangeId;
use indras_sync_engine::team::{LogicalAgentId, Team};
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::{FolderLock, LocalWorkspaceIndex, WorkspaceWatcher};
use indras_sync_engine::SymlinkIndex;
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

impl WorkspaceHandle {
    /// Snapshot the agent's current on-disk state and land it into
    /// `vault`'s inner (local-only) braid as an agent-authored changeset.
    ///
    /// This is the primary bridge from the device-local working tree to
    /// the hierarchical braid: the agent's edits live in a
    /// [`LocalWorkspaceIndex`], and this call lifts the full snapshot
    /// into a [`SymlinkIndex`] and records it on the inner DAG via
    /// [`Vault::agent_land`]. No CRDT sync, no peer visibility — the
    /// user still has to merge the agent's HEAD and then
    /// [`Vault::promote`](indras_sync_engine::vault::Vault::promote) to
    /// broadcast.
    pub async fn land_to_inner_braid(
        &self,
        vault: &Vault,
        intent: String,
        evidence: Evidence,
    ) -> ChangeId {
        vault
            .land_agent_snapshot(&self.agent, &self.index, intent, evidence)
            .await
    }
}

/// Runtime-binding error surface for [`runtime_bind`].
///
/// Distinguishes recoverable UI-surfaceable causes from generic io noise
/// so the Agent Roster can render a targeted message (e.g. "folder
/// already held by another syncengine") and a one-click retry.
#[derive(Debug)]
pub enum BindError {
    /// Another process (or another binding) already owns the folder's
    /// advisory lock. Typically means a stale syncengine is still
    /// mirroring it, or the folder was bound earlier in this run.
    LockHeld,
    /// The target folder does not exist on disk. `runtime_bind` does not
    /// create the folder itself — the caller is responsible for
    /// materializing it before binding (so first-run creation and
    /// rehydration remain separate concerns).
    FolderMissing,
    /// [`WorkspaceWatcher::start`] returned an error (e.g. notify backend
    /// failed to subscribe). The string is the underlying error's
    /// `Display`.
    WatcherFailed(String),
    /// The path is unbindable for a reason other than lock contention —
    /// e.g. it points at a file, a symlink loop, or is outside the
    /// allowed vault root.
    PathCollision,
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::LockHeld => write!(f, "folder lock is held by another process"),
            BindError::FolderMissing => write!(f, "folder does not exist"),
            BindError::WatcherFailed(e) => write!(f, "workspace watcher failed to start: {e}"),
            BindError::PathCollision => write!(f, "path is not a bindable directory"),
        }
    }
}

impl std::error::Error for BindError {}

/// Acquire the folder lock, populate an initial index, and start a
/// [`WorkspaceWatcher`] for a single agent binding at runtime.
///
/// This mirrors the per-binding logic inside
/// [`spawn_workspace_watchers`] but (a) returns the handle to the caller
/// instead of pushing into a batch, and (b) surfaces failures via
/// [`BindError`] instead of logging-and-skipping, so interactive
/// creation/retry in the Agent Roster can show an actionable error.
///
/// The caller is responsible for ensuring `folder` exists on disk before
/// calling this function; a missing folder yields
/// [`BindError::FolderMissing`].
pub async fn runtime_bind(
    agent: LogicalAgentId,
    folder: PathBuf,
    blob_store: Arc<BlobStore>,
) -> Result<WorkspaceHandle, BindError> {
    // Existence + type check up front so we can return a specific error
    // instead of letting FolderLock::acquire auto-create the directory.
    match tokio::fs::metadata(&folder).await {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(BindError::PathCollision),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(BindError::FolderMissing);
        }
        Err(_) => return Err(BindError::PathCollision),
    }

    let lock = match FolderLock::acquire(&folder) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            return Err(BindError::LockHeld);
        }
        Err(_) => return Err(BindError::PathCollision),
    };

    let index = Arc::new(LocalWorkspaceIndex::new(
        folder.clone(),
        Arc::clone(&blob_store),
    ));
    if let Err(e) = index.initial_scan().await {
        tracing::warn!(
            folder = %folder.display(),
            error = %e,
            "runtime_bind: initial scan failed; proceeding with empty index"
        );
    }

    match WorkspaceWatcher::start(Arc::clone(&index)) {
        Ok(watcher) => Ok(WorkspaceHandle {
            agent,
            lock,
            watcher,
            index,
        }),
        Err(e) => {
            drop(lock);
            Err(BindError::WatcherFailed(e.to_string()))
        }
    }
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

/// After a successful commit, publish the new HEAD to the braid DAG's
/// per-peer head tracking and materialize files to the vault root.
///
/// Two steps:
/// 1. Update `peer_heads` in the `BraidDag` with this peer's new HEAD
///    (CRDT-synced via the DAG document, visible to all vault members).
/// 2. Write each file in the manifest from the blob store to the vault
///    root on disk; the vault's `VaultWatcher` picks up the writes and
///    syncs the `VaultFileDocument`.
pub async fn publish_and_materialize_head(
    vault_manager: &VaultManager,
    vault_realm: &indras_network::Realm,
    change_id: indras_sync_engine::braid::ChangeId,
    manifest: &indras_sync_engine::braid::PatchManifest,
    user_id: indras_sync_engine::UserId,
) {
    use indras_sync_engine::braid::RealmBraid;

    // 1. Publish HEAD to the braid DAG's peer_heads.
    match vault_realm.braid_dag().await {
        Ok(dag) => {
            let manifest_clone: SymlinkIndex = manifest.clone().into();
            if let Err(e) = dag
                .update(|d| {
                    d.update_peer_head(user_id, change_id, manifest_clone.into());
                })
                .await
            {
                tracing::warn!(error = %e, "failed to publish HEAD to braid DAG");
            }
        }
        Err(e) => tracing::warn!(error = %e, "braid_dag unavailable for HEAD publish"),
    }

    // 2. Materialize files to the vault root on disk.
    let rid = *vault_realm.id().as_bytes();
    let Some(vault_path) = vault_manager.vault_path(&rid) else {
        tracing::warn!("vault path not found; skipping materialization");
        return;
    };
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

/// No-op: the braid DAG now lives on the vault realm directly.
///
/// Previously, this function materialized a separate team realm for each
/// vault. With DAG universalization, all vault members automatically
/// participate in the DAG — no separate realm is needed. This function
/// is retained for call-site compatibility and will be removed in a
/// future cleanup.
pub async fn ensure_team_realms_for_hosted_vaults(
    _network: &IndrasNetwork,
    _vault_manager: &VaultManager,
    _registry: &TeamBindingRegistry,
) {
    // DAG lives on vault realm; no team realm materialization needed.
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

    async fn test_blob_store(tmp: &tempfile::TempDir) -> Arc<BlobStore> {
        let cfg = indras_storage::BlobStoreConfig {
            base_dir: tmp.path().join("blobs"),
            ..Default::default()
        };
        Arc::new(BlobStore::new(cfg).await.expect("blob store init"))
    }

    #[tokio::test]
    async fn runtime_bind_reports_folder_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let blob_store = test_blob_store(&tmp).await;
        match runtime_bind(agent("agent-ghost"), missing, blob_store).await {
            Err(BindError::FolderMissing) => {}
            Err(e) => panic!("expected FolderMissing, got {e:?}"),
            Ok(_) => panic!("expected FolderMissing, got Ok(handle)"),
        }
    }

    #[tokio::test]
    async fn runtime_bind_succeeds_on_fresh_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("agent-fresh");
        std::fs::create_dir_all(&folder).unwrap();
        let blob_store = test_blob_store(&tmp).await;
        let handle = match runtime_bind(agent("agent-fresh"), folder.clone(), blob_store).await {
            Ok(h) => h,
            Err(e) => panic!("bind should succeed on a fresh empty folder, got {e:?}"),
        };
        assert_eq!(handle.agent.as_str(), "agent-fresh");
        drop(handle);
    }

    #[tokio::test]
    async fn runtime_bind_reports_lock_held() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("agent-contested");
        std::fs::create_dir_all(&folder).unwrap();
        let blob_store = test_blob_store(&tmp).await;
        let first = match runtime_bind(
            agent("agent-contested"),
            folder.clone(),
            Arc::clone(&blob_store),
        )
        .await
        {
            Ok(h) => h,
            Err(e) => panic!("first bind should succeed, got {e:?}"),
        };
        match runtime_bind(agent("agent-contested"), folder.clone(), blob_store).await {
            Err(BindError::LockHeld) => {}
            Err(e) => panic!("expected LockHeld, got {e:?}"),
            Ok(_) => panic!("second bind should fail while first holds lock"),
        }
        drop(first);
    }

    #[tokio::test]
    async fn runtime_bind_rejects_non_directory_path() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("not-a-dir.txt");
        std::fs::write(&file_path, b"hello").unwrap();
        let blob_store = test_blob_store(&tmp).await;
        match runtime_bind(agent("agent-collision"), file_path, blob_store).await {
            Err(BindError::PathCollision) => {}
            Err(e) => panic!("expected PathCollision, got {e:?}"),
            Ok(_) => panic!("expected PathCollision for a file path, got Ok(handle)"),
        }
    }
}
