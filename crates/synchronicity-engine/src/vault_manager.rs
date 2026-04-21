//! Per-realm vault sync manager.
//!
//! Owns a `Vault` instance per shared realm, wiring up `VaultWatcher`,
//! `SyncToDisk`, and `RelayBlobSync` so files automatically propagate
//! between realm members.
//!
//! # Vault directory layout
//!
//! All vaults live as siblings under `{data_dir}/vaults/`, named after
//! the peer (for DMs) or the realm (for groups/worlds). The home
//! vault is named after the user's own display name. This lets a user
//! open `{data_dir}/vaults/` as a single Obsidian workspace root and
//! see every vault as a named subfolder.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use indras_network::{IndrasNetwork, Realm};
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::vault::vault_file::VaultFile;
use indras_sync_engine::vault::Vault;
use tokio::sync::RwLock;
use tracing::info;

/// Step in the full sync process, reported to the UI via progress channel.
#[derive(Debug, Clone)]
pub enum SyncStep {
    /// Checking for local changes.
    Checking,
    /// Committing dirty files to the DAG.
    Committing { dirty_count: usize },
    /// Commit landed.
    Committed { change_id: String },
    /// Checking for peer changes.
    Pulling,
    /// Peer forks detected.
    PeerForks { count: usize },
    /// Merged a trusted peer's fork.
    Merged { peer: String, change_id: String },
    /// Sync complete.
    Done { summary: String },
    /// Nothing to sync.
    NothingToSync,
    /// Sync failed.
    Failed(String),
}

/// Summary of a full sync operation.
#[derive(Debug, Clone, Default)]
pub struct SyncSummary {
    /// Short hex of the committed changeset (if any).
    pub committed: Option<String>,
    /// Number of files in the commit.
    pub files_committed: usize,
    /// Number of peer forks detected.
    pub peer_forks: usize,
    /// Number of trusted peer forks auto-merged.
    pub merges: usize,
}

/// Manages per-realm vault sync instances.
///
/// Each shared realm (DM, Group, World) gets its own on-disk vault
/// directory with a `Vault` that handles bidirectional file sync via
/// the CRDT pipeline.
///
/// All vaults share a single content-addressed blob store so that
/// identical files synced to multiple realms are stored only once on
/// disk.
pub struct VaultManager {
    /// Active vaults keyed by realm ID bytes.
    vaults: RwLock<HashMap<[u8; 32], Vault>>,
    /// Recorded vault directory per realm so `vault_path()` returns
    /// the same name-based path that `ensure_vault` chose. DashMap
    /// (not tokio RwLock) so UI render/click handlers can resolve
    /// paths synchronously.
    paths: DashMap<[u8; 32], PathBuf>,
    /// Reverse index: which realm owns a given sanitized vault name.
    /// Used for collision resolution.
    name_to_realm: RwLock<HashMap<String, [u8; 32]>>,
    /// Base data directory (vaults live under `{data_dir}/vaults/`).
    data_dir: PathBuf,
    /// Shared blob store across all vaults on this device.
    blob_store: Arc<BlobStore>,
}

impl VaultManager {
    /// Create a new vault manager with a shared blob store.
    ///
    /// The blob store lives at `{data_dir}/shared-blobs/` and is
    /// passed to every vault so identical content is stored once.
    pub async fn new(data_dir: PathBuf) -> Result<Self, String> {
        let blob_dir = data_dir.join("shared-blobs");
        let blob_config = BlobStoreConfig {
            base_dir: blob_dir,
            ..Default::default()
        };
        let blob_store = Arc::new(
            BlobStore::new(blob_config)
                .await
                .map_err(|e| format!("shared blob store: {e}"))?,
        );
        info!(path = %data_dir.display(), "VaultManager started with shared blob store");
        Ok(Self {
            vaults: RwLock::new(HashMap::new()),
            paths: DashMap::new(),
            name_to_realm: RwLock::new(HashMap::new()),
            data_dir,
            blob_store,
        })
    }

    /// Ensure vault sync is running for a realm.
    ///
    /// Idempotent — returns immediately if the vault already exists.
    /// Creates the vault directory, attaches the sync pipeline, and
    /// runs an initial scan of any pre-existing files.
    ///
    /// `peer_name` is used to name the on-disk directory (sanitized;
    /// falls back to a short hex of the realm id if `None` or empty).
    /// Collisions with a different realm append a short-hex suffix.
    pub async fn ensure_vault(
        &self,
        network: &IndrasNetwork,
        realm: &Realm,
        peer_name: Option<&str>,
    ) -> Result<(), String> {
        let rid = *realm.id().as_bytes();

        // Fast path: already tracked
        if self.vaults.read().await.contains_key(&rid) {
            return Ok(());
        }

        // Slow path: create vault (double-check under write lock)
        let mut vaults = self.vaults.write().await;
        if vaults.contains_key(&rid) {
            return Ok(());
        }

        let final_name = self.resolve_vault_name(&rid, peer_name).await;
        let vault_path = self.data_dir.join("vaults").join(&final_name);

        let vault = Vault::attach(
                network,
                realm.clone(),
                vault_path.clone(),
                Arc::clone(&self.blob_store),
            )
            .await
            .map_err(|e| format!("vault attach: {e}"))?;

        let count = vault
            .initial_scan()
            .await
            .map_err(|e| format!("initial scan: {e}"))?;

        info!(realm_name = %final_name, files = count, "Vault sync started");
        vaults.insert(rid, vault);
        self.paths.insert(rid, vault_path);
        self.name_to_realm.write().await.insert(final_name, rid);
        Ok(())
    }

    /// Predict the on-disk path of the user's private (home) vault.
    ///
    /// The home vault lives at `{data_dir}/vaults/<sanitize(self_name)>/`
    /// alongside peer DM vaults, so Obsidian can open the parent
    /// `vaults/` folder as one workspace. Returns the chosen path.
    /// Does not itself register or attach a vault — callers should call
    /// [`ensure_vault`](Self::ensure_vault) with the home realm to
    /// actually wire up sync. Kept as a pure path helper so UI code can
    /// reason about the directory before the realm is ready.
    pub async fn start_private_vault(&self, self_name: &str) -> PathBuf {
        let sanitized = sanitize(self_name).unwrap_or_else(|| "home".to_string());
        self.data_dir.join("vaults").join(sanitized)
    }

    /// List active (non-deleted) files for a realm.
    ///
    /// Returns an empty vec if the vault hasn't been initialized yet.
    pub async fn list_files(&self, realm_id: &[u8; 32]) -> Vec<VaultFile> {
        let vaults = self.vaults.read().await;
        match vaults.get(realm_id) {
            Some(vault) => vault.list_files().await,
            None => Vec::new(),
        }
    }

    /// Get the on-disk vault directory for a realm.
    ///
    /// Returns `None` if the vault hasn't been initialized yet. Synchronous so
    /// UI render/click handlers can resolve paths without awaiting.
    pub fn vault_path(&self, realm_id: &[u8; 32]) -> Option<PathBuf> {
        self.paths.get(realm_id).map(|e| e.value().clone())
    }

    /// Snapshot of every realm this manager currently owns a vault for.
    ///
    /// Used to iterate vaults at startup for cross-cutting work like
    /// materializing team realms. Clones the `Realm` handles; the
    /// underlying realm state is still shared with `IndrasNetwork`.
    pub async fn realms(&self) -> Vec<Realm> {
        self.vaults
            .read()
            .await
            .values()
            .map(|v| v.realm().clone())
            .collect()
    }

    /// Shared content-addressed blob store used by every vault on this
    /// device. Also the store into which agent-folder working-tree
    /// content is written by [`LocalWorkspaceIndex`].
    pub fn blob_store(&self) -> Arc<BlobStore> {
        Arc::clone(&self.blob_store)
    }

    /// Sync a vault's dirty files to the braid DAG.
    ///
    /// Calls `Vault::sync()` on the vault for the given realm, creating
    /// a changeset with `Evidence::Human`. Returns the new `ChangeId`
    /// on success, or an error if the vault isn't found or has nothing
    /// to sync.
    pub async fn sync_vault(
        &self,
        realm_id: &[u8; 32],
        intent: String,
        message: Option<String>,
    ) -> Result<indras_sync_engine::braid::ChangeId, String> {
        let mut vaults = self.vaults.write().await;
        let vault = vaults
            .get_mut(realm_id)
            .ok_or_else(|| "vault not found for this realm".to_string())?;
        vault
            .sync(intent, message)
            .await
            .map_err(|e| format!("{e}"))
    }

    /// Get the user ID for the first vault (all vaults share the same user).
    pub async fn user_id(&self) -> Option<indras_sync_engine::vault::vault_file::UserId> {
        let vaults = self.vaults.read().await;
        vaults.values().next().map(|v| v.user_id())
    }

    /// Collect diverged agents across every attached vault's inner braid.
    ///
    /// For each vault, calls
    /// [`AgentBraid::agent_forks`](indras_sync_engine::braid::AgentBraid::agent_forks)
    /// with the supplied `roster`, then counts the number of changesets on
    /// each agent's branch that aren't reachable from the user's inner HEAD
    /// — the "N changes ahead" figure rendered on the Agent Lane strip.
    ///
    /// Pure read path: does not mutate any vault state.
    pub async fn collect_agent_forks(
        &self,
        roster: &[indras_sync_engine::team::LogicalAgentId],
    ) -> Vec<crate::state::AgentForkView> {
        if roster.is_empty() {
            return Vec::new();
        }
        let vaults = self.vaults.read().await;
        let mut out: Vec<crate::state::AgentForkView> = Vec::new();
        for (realm_id, vault) in vaults.iter() {
            let inner = vault.inner_braid().read().await;
            let dag = inner.dag();
            let user_ancestors = inner
                .user_head()
                .map(|ps| {
                    let mut a = dag.ancestors(&ps.head);
                    a.insert(ps.head);
                    a
                })
                .unwrap_or_default();

            for (agent, ps) in inner.agent_forks(roster) {
                // Count commits reachable from the agent HEAD (inclusive)
                // that are NOT reachable from the user HEAD.
                let mut reachable = dag.ancestors(&ps.head);
                reachable.insert(ps.head);
                let change_count = reachable.difference(&user_ancestors).count();

                let agent_id = inner.agent_user_id(&agent);
                let color_class = crate::state::member_class_for(&agent_id);
                let color_hex = crate::state::member_hex_for(&agent_id);
                let head_short_hex: String = ps
                    .head
                    .as_bytes()
                    .iter()
                    .take(4)
                    .map(|b| format!("{b:02x}"))
                    .collect();

                out.push(crate::state::AgentForkView {
                    name: agent.as_str().to_string(),
                    realm_id: *realm_id,
                    change_count,
                    head_short_hex,
                    color_class,
                    color_hex,
                    // Runtime status is populated by the polling loop in
                    // home_vault.rs after it drains IPC hook events. Default
                    // to Idle here so the struct is always valid.
                    runtime_status: crate::state::AgentRuntimeStatus::default(),
                });
            }
        }
        out
    }

    /// Land an agent's working-tree snapshot into the first vault's inner
    /// braid. The single-vault assumption mirrors the rest of this
    /// manager's Phase-1 surface; multi-vault routing is Phase-N.
    ///
    /// Returns the new inner-braid [`ChangeId`] or an error string if no
    /// vault is registered. The caller owns the `Arc<LocalWorkspaceIndex>`
    /// already (from the `WorkspaceHandle`), so this method borrows it
    /// rather than snapshotting ownership.
    pub async fn land_agent_snapshot_on_first(
        &self,
        agent: &indras_sync_engine::team::LogicalAgentId,
        index: &Arc<indras_sync_engine::workspace::LocalWorkspaceIndex>,
        intent: String,
        evidence: indras_sync_engine::braid::changeset::Evidence,
    ) -> Result<indras_sync_engine::braid::ChangeId, String> {
        let vaults = self.vaults.read().await;
        let vault = vaults
            .values()
            .next()
            .ok_or_else(|| "no vault on this device".to_string())?;
        Ok(vault
            .land_agent_snapshot(agent, index.as_ref(), intent, evidence)
            .await)
    }

    /// Run the full braid sync pipeline on the first registered vault.
    ///
    /// Merges every agent in `roster` whose inner HEAD diverges, then
    /// promotes the user's inner HEAD (if it differs from the outer
    /// HEAD), auto-merges trusted peers, and materializes the resulting
    /// outer HEAD to disk. Returns the per-step summary.
    ///
    /// Single-vault assumption matches the rest of the manager's
    /// Phase-1 surface.
    pub async fn sync_all_on_first(
        &self,
        intent: String,
        roster: &[indras_sync_engine::team::LogicalAgentId],
    ) -> Result<indras_sync_engine::vault::SyncAllReport, String> {
        let vaults = self.vaults.read().await;
        let vault = vaults
            .values()
            .next()
            .ok_or_else(|| "no vault on this device".to_string())?;
        vault
            .sync_all(intent, roster)
            .await
            .map_err(|e| format!("{e}"))
    }

    /// Whether a vault's *inner* braid (local-only agent DAG) contains
    /// the given changeset id. Returns `false` if no vault is registered
    /// for the realm. Used by Phase-3 view-model helpers to detect
    /// un-promoted commits, and by integration tests verifying the
    /// inner-braid routing.
    pub async fn inner_braid_contains(
        &self,
        realm_id: &[u8; 32],
        id: &indras_sync_engine::braid::ChangeId,
    ) -> bool {
        let vaults = self.vaults.read().await;
        let Some(vault) = vaults.get(realm_id) else {
            return false;
        };
        vault.inner_braid().read().await.dag().contains(id)
    }

    /// Whether a vault's *outer* (shared, signed) DAG contains the given
    /// changeset id. Counterpart to [`Self::inner_braid_contains`] —
    /// together they let callers tell where a commit currently lives in
    /// the hierarchical braid.
    pub async fn outer_dag_contains(
        &self,
        realm_id: &[u8; 32],
        id: &indras_sync_engine::braid::ChangeId,
    ) -> bool {
        let vaults = self.vaults.read().await;
        let Some(vault) = vaults.get(realm_id) else {
            return false;
        };
        vault.dag().read().await.contains(id)
    }

    /// Load a [`crate::state::BraidView`] snapshot for a realm's braid.
    ///
    /// Reads the vault's `BraidDag` CRDT and translates it into the
    /// plain view model the drawer renders from. Returns `None` if the
    /// realm has no attached vault on this device yet.
    pub async fn load_braid_view(
        &self,
        realm_id: &[u8; 32],
        peers: &[crate::state::PeerDisplayInfo],
        self_display_name: &str,
    ) -> Option<crate::state::BraidView> {
        let vaults = self.vaults.read().await;
        let vault = vaults.get(realm_id)?;
        let dag_guard = vault.dag().read().await;
        let self_user_id = vault.user_id();
        Some(crate::braid_bridge::build_braid_view(
            *realm_id,
            &dag_guard,
            peers,
            self_user_id,
            self_display_name,
        ))
    }

    /// Full sync: commit local changes, check for peer forks, auto-merge
    /// trusted peers. Reports each step via `progress` channel.
    pub async fn full_sync(
        &self,
        realm_id: &[u8; 32],
        intent: String,
        progress: tokio::sync::mpsc::Sender<SyncStep>,
    ) -> Result<SyncSummary, String> {
        let mut summary = SyncSummary::default();

        // Step 1: Check dirty count.
        let _ = progress.send(SyncStep::Checking).await;
        let dirty_count = {
            let vaults = self.vaults.read().await;
            let vault = vaults
                .get(realm_id)
                .ok_or_else(|| "vault not found".to_string())?;
            vault.dirty_count()
        };

        // Step 2: Commit if there are dirty files.
        if dirty_count > 0 {
            let _ = progress.send(SyncStep::Committing { dirty_count }).await;
            let mut vaults = self.vaults.write().await;
            let vault = vaults.get_mut(realm_id).ok_or("vault not found")?;
            match vault.sync(intent, None).await {
                Ok(id) => {
                    let short: String = id
                        .as_bytes()
                        .iter()
                        .take(4)
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    summary.committed = Some(short.clone());
                    summary.files_committed = dirty_count;
                    let _ = progress.send(SyncStep::Committed { change_id: short }).await;
                }
                Err(e) => {
                    let msg = format!("{e}");
                    let _ = progress.send(SyncStep::Failed(msg.clone())).await;
                    return Err(msg);
                }
            }
        } else {
            let _ = progress.send(SyncStep::NothingToSync).await;
        }

        // Step 3: Check for peer forks.
        let _ = progress.send(SyncStep::Pulling).await;
        let fork_count = {
            let vaults = self.vaults.read().await;
            let vault = vaults.get(realm_id).ok_or("vault not found")?;
            vault.pending_forks().await.len()
        };
        summary.peer_forks = fork_count;
        if fork_count > 0 {
            let _ = progress.send(SyncStep::PeerForks { count: fork_count }).await;
        }

        // Step 4: Auto-merge trusted peer forks.
        if fork_count > 0 {
            let merged = {
                let vaults = self.vaults.read().await;
                let vault = vaults.get(realm_id).ok_or("vault not found")?;
                vault.auto_merge_trusted().await
            };
            for (peer_id, change_id) in &merged {
                let peer_short: String = peer_id[..4].iter().map(|b| format!("{b:02x}")).collect();
                let change_short: String = change_id
                    .as_bytes()
                    .iter()
                    .take(4)
                    .map(|b| format!("{b:02x}"))
                    .collect();
                let _ = progress
                    .send(SyncStep::Merged {
                        peer: peer_short,
                        change_id: change_short,
                    })
                    .await;
            }
            summary.merges = merged.len();
        }

        // Step 5: Done.
        let done_msg = if summary.files_committed > 0 || summary.merges > 0 {
            let mut parts = Vec::new();
            if summary.files_committed > 0 {
                parts.push(format!("{} files committed", summary.files_committed));
            }
            if summary.merges > 0 {
                parts.push(format!("{} merged", summary.merges));
            }
            if summary.peer_forks > summary.merges {
                parts.push(format!(
                    "{} untrusted forks",
                    summary.peer_forks - summary.merges
                ));
            }
            parts.join(", ")
        } else {
            "up to date".to_string()
        };
        let _ = progress.send(SyncStep::Done { summary: done_msg }).await;

        Ok(summary)
    }

    /// Resolve the final sanitized vault directory name for a realm,
    /// handling sanitization, empty fallback, and collision suffixing.
    async fn resolve_vault_name(
        &self,
        rid: &[u8; 32],
        peer_name: Option<&str>,
    ) -> String {
        let base = peer_name
            .and_then(sanitize)
            .unwrap_or_else(|| short_hex(rid));

        let n2r = self.name_to_realm.read().await;
        match n2r.get(&base) {
            None => base,
            Some(existing) if existing == rid => base,
            Some(_) => format!("{}.{}", base, short_hex(rid)),
        }
    }
}

/// Keep only `[A-Za-z0-9_-]` characters; return `None` if empty.
fn sanitize(name: &str) -> Option<String> {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if s.is_empty() { None } else { Some(s) }
}

/// Six-char lowercase hex prefix of the first 3 bytes of `rid`.
fn short_hex(rid: &[u8; 32]) -> String {
    rid.iter().take(3).map(|b| format!("{b:02x}")).collect()
}
