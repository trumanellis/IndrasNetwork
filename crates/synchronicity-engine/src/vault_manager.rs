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
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use dashmap::DashMap;
use indras_network::{IndrasNetwork, Realm};
use indras_storage::{BlobStore, BlobStoreConfig, ContentRef};
use indras_sync_engine::braid::changeset::PatchManifest;
use indras_sync_engine::project;
use indras_sync_engine::project::{ProjectEntry, ProjectRegistry};
use indras_sync_engine::vault::vault_file::VaultFile;
use indras_sync_engine::vault::Vault;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Lightweight description of a Project realm attached to a parent realm.
///
/// Returned by [`VaultManager::create_project`] so callers can stash the id,
/// display the name, and re-open the Project later by passing the head back
/// into [`VaultManager::open_project`]. `manifest_head` points into the
/// shared [`BlobStore`] at the currently-materialized [`PatchManifest`].
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    /// Realm id allocated for this project (own, independent realm).
    pub id: [u8; 32],
    /// Parent realm that hosts this project's folder.
    pub parent: [u8; 32],
    /// Human-readable project name.
    pub name: String,
    /// Content-addressed reference to the project's current manifest blob.
    pub manifest_head: ContentRef,
}

/// Default blob-GC interval — 15 minutes.
///
/// Long enough that a vault with busy writers isn't churning GC calls,
/// short enough that abandoned blobs clear inside a normal session.
pub const DEFAULT_GC_INTERVAL: Duration = Duration::from_secs(15 * 60);

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
    /// Optional background blob-GC task. Populated by
    /// [`Self::start_gc_loop`]; aborted by [`Self::stop_gc_loop`] or
    /// when the manager drops.
    gc_task: StdMutex<Option<JoinHandle<()>>>,
    /// Current manifest head per Project realm. Updated on
    /// [`Self::create_project`] and future project-snapshot passes.
    /// DashMap so UI callers can look up heads without awaiting.
    project_heads: DashMap<[u8; 32], ContentRef>,
    /// Parent realm → list of Project realm ids it contains.
    /// Written by [`Self::create_project`], read by
    /// [`Self::projects_of`].
    projects_by_parent: DashMap<[u8; 32], Vec<[u8; 32]>>,
    /// Display name per Project realm id. Populated by
    /// [`Self::create_project`], read by [`Self::project_name`]. Separate from
    /// [`Self::projects_by_parent`] so the UI can render the name synchronously
    /// without fetching the manifest.
    project_names: DashMap<[u8; 32], String>,
    /// Handle to the running `IndrasNetwork`, set lazily after construction via
    /// [`Self::set_network`] (or implicitly the first time [`Self::ensure_vault`]
    /// sees one). Needed to open the per-realm
    /// [`Document<ProjectRegistry>`](indras_sync_engine::project::ProjectRegistry)
    /// that is the source of truth for project metadata. `None` during unit
    /// tests that pre-register paths without standing up a network; the
    /// registry writes are then skipped and the DashMaps act as local-only
    /// caches.
    network: RwLock<Option<Arc<IndrasNetwork>>>,
    /// Realms whose `_projects` document has an active subscription listener
    /// running. Used to keep
    /// [`Self::subscribe_to_registry`] idempotent.
    subscribed_registries: DashMap<[u8; 32], ()>,
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
            gc_task: StdMutex::new(None),
            project_heads: DashMap::new(),
            projects_by_parent: DashMap::new(),
            project_names: DashMap::new(),
            network: RwLock::new(None),
            subscribed_registries: DashMap::new(),
        })
    }

    /// Cache a handle to the running [`IndrasNetwork`] so project-registry
    /// operations can open the per-realm `Document<ProjectRegistry>`.
    ///
    /// Idempotent — subsequent calls replace the stored handle. Called once
    /// from the boot flow (after `IndrasNetwork::new` succeeds); unit tests
    /// that pre-register paths without a network simply skip this call and
    /// the registry-write paths fall back to updating the in-memory caches
    /// only.
    pub async fn set_network(&self, network: Arc<IndrasNetwork>) {
        *self.network.write().await = Some(network);
    }

    /// Resolve the private-vault sentinel `[0u8; 32]` to the actual home realm
    /// id (or pass through any other realm id unchanged).
    ///
    /// Project registry writes need the real home realm id because that's
    /// where the `Document<ProjectRegistry>` is hosted — the `[0u8; 32]`
    /// sentinel is a UI convenience, not a network-addressable realm.
    async fn resolve_registry_realm(&self, parent_realm_id: &[u8; 32]) -> Option<[u8; 32]> {
        if *parent_realm_id != [0u8; 32] {
            return Some(*parent_realm_id);
        }
        let net_guard = self.network.read().await;
        let net = net_guard.as_ref()?;
        match net.home_realm().await {
            Ok(home) => Some(*home.id().as_bytes()),
            Err(e) => {
                warn!(error = %e, "resolve_registry_realm: home_realm() failed");
                None
            }
        }
    }

    /// Open the `Document<ProjectRegistry>` for `realm_id`, returning `None`
    /// if no network is attached or the realm isn't loaded.
    async fn open_registry_doc(
        &self,
        realm_id: &[u8; 32],
    ) -> Option<indras_network::Document<ProjectRegistry>> {
        let net_guard = self.network.read().await;
        let net = net_guard.as_ref()?;
        let rid = indras_network::RealmId::from(*realm_id);
        let realm = net.get_realm_by_id(&rid)?;
        match realm.document::<ProjectRegistry>("_projects").await {
            Ok(doc) => Some(doc),
            Err(e) => {
                warn!(
                    realm = %short_hex(realm_id),
                    error = %e,
                    "open_registry_doc: failed to open _projects document"
                );
                None
            }
        }
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
        // Drop the vaults write guard before opening the registry doc —
        // `subscribe_to_registry` may re-enter the vault manager via the
        // network layer, and holding `vaults` locked would deadlock.
        drop(vaults);
        // Best-effort: open the `_projects` document for this realm and
        // spawn the cache-refresh listener. Silent no-op if the network
        // handle isn't attached yet — later callers (e.g. the private
        // column polling loop) will hit this path again once set_network
        // has run.
        self.subscribe_to_registry(&rid).await;
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
        let path = self.data_dir.join("vaults").join(sanitized);
        self.paths.insert([0u8; 32], path.clone());
        path
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

    /// Get the on-disk folder that backs a Project realm.
    ///
    /// Returns `<parent_vault_root>/projects/<hex(project_id)>/`, or `None` if
    /// the parent vault hasn't been initialized yet. Does not itself ensure
    /// the folder exists on disk — use [`Self::create_project`] for that.
    pub fn project_path(
        &self,
        parent_realm_id: &[u8; 32],
        project_id: &[u8; 32],
    ) -> Option<PathBuf> {
        let parent = self.vault_path(parent_realm_id)?;
        let hex = hex_bytes(project_id);
        Some(parent.join("projects").join(hex))
    }

    /// Allocate a fresh Project realm under `parent_realm_id`.
    ///
    /// Steps:
    /// 1. Derive a new [`RealmId`](indras_network::RealmId) via
    ///    `artifact_interface_id(&generate_tree_id())` — same path that
    ///    normal shared realms use.
    /// 2. Create `<parent_vault_root>/projects/<hex(project_id)>/` on disk.
    /// 3. Persist an empty [`PatchManifest`] to the blob store and record
    ///    its [`ContentRef`] as this project's manifest head.
    /// 4. Link the new project under its parent so
    ///    [`Self::projects_of`] surfaces it.
    ///
    /// Errors as a string if the parent vault isn't registered or any blob /
    /// filesystem I/O fails.
    pub async fn create_project(
        &self,
        parent_realm_id: &[u8; 32],
        name: &str,
    ) -> Result<ProjectInfo, String> {
        let parent_root = self
            .vault_path(parent_realm_id)
            .ok_or_else(|| "parent vault not initialized".to_string())?;

        let artifact_id = indras_network::generate_tree_id();
        let interface_id = indras_network::artifact_interface_id(&artifact_id);
        let project_id: [u8; 32] = *interface_id.as_bytes();

        let project_dir = parent_root.join("projects").join(hex_bytes(&project_id));
        tokio::fs::create_dir_all(&project_dir)
            .await
            .map_err(|e| format!("create project dir: {e}"))?;

        let empty = PatchManifest::default();
        let bytes = serde_json::to_vec(&empty)
            .map_err(|e| format!("encode empty manifest: {e}"))?;
        let manifest_head = self
            .blob_store
            .store(&bytes)
            .await
            .map_err(|e| format!("store manifest blob: {e}"))?;

        // Populate in-memory caches so the UI sees the project immediately —
        // the subscription listener will re-populate from the document if we
        // get evicted, but this path is synchronous w.r.t. the UI.
        self.project_heads.insert(project_id, manifest_head);
        self.projects_by_parent
            .entry(*parent_realm_id)
            .or_default()
            .push(project_id);
        self.project_names.insert(project_id, name.to_string());

        // Write the new project into the per-realm `Document<ProjectRegistry>`
        // if a network handle is attached. The `[0u8; 32]` private sentinel
        // is resolved to the actual home realm id so multi-device sync
        // across the user's own devices works the same way as cross-peer
        // sync in shared realms.
        let creator: [u8; 32] = {
            let net_guard = self.network.read().await;
            net_guard.as_ref().map(|n| n.id()).unwrap_or([0u8; 32])
        };
        let created_at = chrono::Utc::now().timestamp_millis();
        let entry = ProjectEntry {
            id: project_id,
            name: name.to_string(),
            manifest_head,
            creator,
            created_at,
        };
        if let Some(registry_realm) = self.resolve_registry_realm(parent_realm_id).await
            && let Some(doc) = self.open_registry_doc(&registry_realm).await
        {
            let entry_for_doc = entry.clone();
            if let Err(e) = doc
                .update(move |reg| {
                    reg.insert(entry_for_doc);
                })
                .await
            {
                warn!(
                    parent = %short_hex(parent_realm_id),
                    project = %short_hex(&project_id),
                    error = %e,
                    "create_project: failed to write ProjectRegistry document"
                );
            }
        }

        info!(
            parent = ?parent_realm_id,
            project = ?project_id,
            path = %project_dir.display(),
            "Project created"
        );
        Ok(ProjectInfo {
            id: project_id,
            parent: *parent_realm_id,
            name: name.to_string(),
            manifest_head,
        })
    }

    /// Materialize a Project's current manifest into its on-disk folder.
    ///
    /// Loads the manifest blob pointed at by the recorded head and calls
    /// [`project::materialize_to`] to write every file under the project's
    /// folder. Errors if the project has no recorded head on this device or
    /// any blob fetch / filesystem write fails.
    pub async fn open_project(
        &self,
        parent_realm_id: &[u8; 32],
        project_id: &[u8; 32],
    ) -> Result<(), String> {
        let project_dir = self
            .project_path(parent_realm_id, project_id)
            .ok_or_else(|| "parent vault not initialized".to_string())?;
        tokio::fs::create_dir_all(&project_dir)
            .await
            .map_err(|e| format!("ensure project dir: {e}"))?;

        let head = self
            .project_heads
            .get(project_id)
            .map(|e| *e.value())
            .ok_or_else(|| "no manifest head for project".to_string())?;

        let bytes = self
            .blob_store
            .load(&head)
            .await
            .map_err(|e| format!("load manifest blob: {e}"))?;
        let manifest: PatchManifest = serde_json::from_slice(&bytes)
            .map_err(|e| format!("decode manifest: {e}"))?;

        project::materialize_to(&manifest, &project_dir, &self.blob_store)
            .await
            .map_err(|e| format!("materialize project: {e}"))?;
        Ok(())
    }

    /// List the Project realm ids currently registered under
    /// `parent_realm_id`. Returns an empty vec if none.
    pub fn projects_of(&self, parent_realm_id: &[u8; 32]) -> Vec<[u8; 32]> {
        self.projects_by_parent
            .get(parent_realm_id)
            .map(|e| e.value().clone())
            .unwrap_or_default()
    }

    /// Display name recorded for a Project realm id, if any. Populated by
    /// [`Self::create_project`]. Synchronous so UI render can resolve the
    /// label without awaiting.
    pub fn project_name(&self, project_id: &[u8; 32]) -> Option<String> {
        self.project_names.get(project_id).map(|e| e.value().clone())
    }

    /// Return the id of the realm's default Project, creating a `main`
    /// Project if none exist yet.
    ///
    /// Acts as a frictionless shim for Phase 3: callers that need a Project
    /// id (e.g. agent creation) but don't yet have a UI to pick one can
    /// resolve to the first project under the realm, or auto-materialize
    /// `main` the first time anyone asks. Idempotent for subsequent calls.
    ///
    /// Race-safe enough for single-process use — the DashMap guard on
    /// `projects_by_parent` plus the second read after
    /// [`Self::create_project`] mean two concurrent callers can at worst
    /// create two projects, both of which remain valid; the first one wins
    /// as the "default" on subsequent lookups.
    pub async fn default_project(
        &self,
        parent_realm_id: &[u8; 32],
    ) -> Result<[u8; 32], String> {
        let pid = match self.projects_of(parent_realm_id).into_iter().next() {
            Some(existing) => existing,
            None => self.create_project(parent_realm_id, "Home").await?.id,
        };
        // Idempotent file migration: move any loose top-level files in the
        // parent vault root into the default project folder, but *only* when
        // that folder is still empty. Once any file (promoted or user-added)
        // lives in the default project, subsequent calls are no-ops. Skips
        // directories (including `projects/`) and dotfiles.
        if let (Some(parent_root), Some(project_dir)) = (
            self.vault_path(parent_realm_id),
            self.project_path(parent_realm_id, &pid),
        ) {
            if let Err(e) = promote_loose_files_if_empty(&parent_root, &project_dir).await {
                warn!(error = %e, "promote loose files into default project");
            }
        }
        Ok(pid)
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

    /// Spawn a periodic background task that calls
    /// [`Vault::gc_blobs`] on every attached vault every `interval`.
    ///
    /// The task owns a cloned `Arc<Self>`, so as long as any caller
    /// holds an `Arc<VaultManager>` the loop stays alive. Any prior
    /// task is aborted before the new one takes over, so calling this
    /// more than once is safe (useful when the manager moves between
    /// setup phases). The first gc pass fires *after* one full
    /// `interval` tick — startup doesn't race vault attach.
    pub fn start_gc_loop(self: &Arc<Self>, interval: Duration) {
        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            // Skip the immediate first tick tokio::time::interval fires.
            tick.tick().await;
            loop {
                tick.tick().await;
                this.run_gc_once().await;
            }
        });
        let mut slot = self
            .gc_task
            .lock()
            .expect("VaultManager gc_task mutex poisoned");
        if let Some(prev) = slot.replace(handle) {
            prev.abort();
        }
    }

    /// Abort the background GC task started by
    /// [`Self::start_gc_loop`]. No-op if no task is running.
    pub fn stop_gc_loop(&self) {
        let handle = self
            .gc_task
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(h) = handle {
            h.abort();
        }
    }

    /// Iterate every attached vault and run one blob-GC pass. Exposed
    /// at crate visibility so unit tests can exercise the GC work
    /// synchronously without spinning up the interval task.
    pub(crate) async fn run_gc_once(&self) {
        let realm_ids: Vec<[u8; 32]> = {
            let vaults = self.vaults.read().await;
            vaults.keys().copied().collect()
        };
        for rid in realm_ids {
            let vaults = self.vaults.read().await;
            let Some(vault) = vaults.get(&rid) else {
                continue;
            };
            match vault.gc_blobs().await {
                Ok(result) => tracing::debug!(
                    realm = ?rid,
                    deleted = result.deleted_count,
                    retained = result.retained_count,
                    bytes_freed = result.bytes_freed,
                    "periodic blob GC pass"
                ),
                Err(e) => tracing::warn!(
                    realm = ?rid,
                    error = %e,
                    "periodic blob GC pass failed"
                ),
            }
        }
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

    /// Land an agent's working-tree snapshot into a vault's inner braid.
    ///
    /// `realm_id` selects the target vault:
    /// - `Some(rid)` — route to the vault keyed by `rid`. Returns an error if
    ///   no vault is registered for that realm.
    /// - `None` — fall back to `vaults.values().next()`, preserving the
    ///   legacy single-vault behavior from Phase 1. This fallback will be
    ///   removed once Phase 3 threads Project IDs through every caller.
    ///
    /// Returns the new inner-braid [`ChangeId`] or an error string if no
    /// matching vault is registered. The caller owns the
    /// `Arc<LocalWorkspaceIndex>` already (from the `WorkspaceHandle`), so
    /// this method borrows it rather than snapshotting ownership.
    pub async fn land_agent_snapshot(
        &self,
        realm_id: Option<&[u8; 32]>,
        agent: &indras_sync_engine::team::LogicalAgentId,
        index: &Arc<indras_sync_engine::workspace::LocalWorkspaceIndex>,
        intent: String,
        evidence: indras_sync_engine::braid::changeset::Evidence,
    ) -> Result<indras_sync_engine::braid::ChangeId, String> {
        let vaults = self.vaults.read().await;
        let vault = match realm_id {
            Some(rid) => vaults
                .get(rid)
                .ok_or_else(|| format!("no vault for realm {}", hex::encode(rid)))?,
            None => vaults
                .values()
                .next()
                .ok_or_else(|| "no vault on this device".to_string())?,
        };
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

    /// Snapshot every registered Project folder into the blob store and update
    /// its manifest head in-place.
    ///
    /// For each Project whose on-disk folder exists, calls
    /// [`indras_sync_engine::project::snapshot_dir`] to build a fresh
    /// [`PatchManifest`], serialises it with `serde_json`, stores the bytes in
    /// the shared [`BlobStore`], and compares the resulting [`ContentRef`]
    /// against the current `project_heads` entry. Projects whose tree is
    /// byte-identical to the current manifest (detected by hash/size equality
    /// on the [`ContentRef`]) are skipped — no new blob is stored for them.
    ///
    /// Returns the list of Project IDs whose manifest head changed. An empty
    /// `Vec` means everything was already up-to-date. Projects that have no
    /// on-disk folder are silently skipped (not yet materialized on this
    /// device).
    pub async fn snapshot_all_projects(&self) -> Result<Vec<[u8; 32]>, String> {
        let mut changed: Vec<[u8; 32]> = Vec::new();

        // Collect (parent, project_id) pairs under a short lock so we
        // don't hold the DashMap ref across the async I/O below.
        let pairs: Vec<([u8; 32], [u8; 32])> = self
            .projects_by_parent
            .iter()
            .flat_map(|entry| {
                let parent = *entry.key();
                entry.value().iter().map(move |pid| (parent, *pid)).collect::<Vec<_>>()
            })
            .collect();

        for (parent, pid) in pairs {
            let project_dir = match self.project_path(&parent, &pid) {
                Some(p) => p,
                None => continue, // parent vault not registered on this device
            };

            // Skip projects not yet materialized on disk.
            if !project_dir.exists() {
                continue;
            }

            let manifest = indras_sync_engine::project::snapshot_dir(&project_dir, &self.blob_store)
                .await
                .map_err(|e| format!("snapshot project {}: {e}", hex_bytes(&pid)))?;

            let bytes = serde_json::to_vec(&manifest)
                .map_err(|e| format!("encode manifest for {}: {e}", hex_bytes(&pid)))?;
            let new_head = self
                .blob_store
                .store(&bytes)
                .await
                .map_err(|e| format!("store manifest blob for {}: {e}", hex_bytes(&pid)))?;

            let old_head = self.project_heads.get(&pid).map(|e| *e.value());
            if old_head == Some(new_head) {
                continue; // byte-identical — nothing changed
            }

            self.project_heads.insert(pid, new_head);
            changed.push(pid);

            // Propagate the new head through the per-realm registry document
            // so peers / other devices materialize the same content.
            if let Some(registry_realm) = self.resolve_registry_realm(&parent).await
                && let Some(doc) = self.open_registry_doc(&registry_realm).await
                && let Err(e) = doc
                    .update(move |reg| {
                        reg.set_head(&pid, new_head);
                    })
                    .await
            {
                warn!(
                    parent = %short_hex(&parent),
                    project = %short_hex(&pid),
                    error = %e,
                    "snapshot_all_projects: failed to update registry head"
                );
            }

            info!(
                project = %short_hex(&pid),
                head = %new_head.short_hash(),
                "project manifest updated"
            );
        }

        if !changed.is_empty() {
            info!(
                count = changed.len(),
                ids = %changed.iter().map(short_hex).collect::<Vec<_>>().join(", "),
                "snapshot_all_projects: updated"
            );
        }

        Ok(changed)
    }

    /// Drain a realm's `Document<ProjectRegistry>` into the in-memory caches,
    /// then spawn a background listener that refreshes the caches whenever
    /// the document changes (locally or via P2P sync).
    ///
    /// Idempotent — subsequent calls for the same realm are no-ops. Safe to
    /// call multiple times as realms come up: each `ensure_vault` caller can
    /// wire its registry without coordination.
    ///
    /// The `parent_realm_id` may be the `[0u8; 32]` private-vault sentinel;
    /// it is resolved to the actual home realm id internally. If the network
    /// isn't attached yet (e.g. early in boot), the call silently returns
    /// and should be retried once `set_network` has been invoked.
    pub async fn subscribe_to_registry(&self, parent_realm_id: &[u8; 32]) {
        // Resolve sentinel → real realm id for document access, but keep
        // the caller's id for the DashMap key so the UI's lookup key matches.
        let ui_parent = *parent_realm_id;
        let Some(registry_realm) = self.resolve_registry_realm(parent_realm_id).await else {
            return;
        };

        // Idempotency guard — keyed on the UI-facing parent id (so the
        // private sentinel and the home realm id both stay idempotent
        // independently of each other, avoiding races between them).
        if self
            .subscribed_registries
            .insert(ui_parent, ())
            .is_some()
        {
            return;
        }

        let Some(doc) = self.open_registry_doc(&registry_realm).await else {
            // Couldn't open yet — drop the idempotency marker so a later
            // retry can succeed once the realm is fully attached.
            self.subscribed_registries.remove(&ui_parent);
            return;
        };

        // Drain whatever's already in the document into the caches.
        {
            let state = doc.read().await;
            self.apply_registry_to_caches(&ui_parent, &state);
        }

        // Spawn a listener that refreshes caches on every change event.
        let rx = doc.subscribe();
        let project_heads = self.project_heads.clone();
        let projects_by_parent = self.projects_by_parent.clone();
        let project_names = self.project_names.clone();
        tokio::spawn(async move {
            let mut rx = rx;
            loop {
                match rx.recv().await {
                    Ok(change) => {
                        apply_registry_change(
                            &ui_parent,
                            &change.new_state,
                            &project_heads,
                            &projects_by_parent,
                            &project_names,
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Lag — keep going; next event re-syncs the caches.
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Snapshot the registry state into the three cache DashMaps. Helper for
    /// both the drain step in [`Self::subscribe_to_registry`] and the
    /// listener loop it spawns (via [`apply_registry_change`] for the
    /// latter, which takes the DashMaps by ref to keep the spawned future
    /// `'static`).
    fn apply_registry_to_caches(
        &self,
        ui_parent: &[u8; 32],
        registry: &ProjectRegistry,
    ) {
        apply_registry_change(
            ui_parent,
            registry,
            &self.project_heads,
            &self.projects_by_parent,
            &self.project_names,
        );
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

impl Drop for VaultManager {
    fn drop(&mut self) {
        // Abort the background GC task so it doesn't outlive the
        // manager that owns the vaults it iterates.
        if let Ok(mut slot) = self.gc_task.lock() {
            if let Some(handle) = slot.take() {
                handle.abort();
            }
        }
    }
}

/// Merge every entry in `registry` into the three project DashMaps, keyed by
/// the UI-facing parent id (which may be the `[0u8; 32]` private sentinel).
///
/// Never removes existing cache rows — the source of truth is the document,
/// and entries only grow (project deletion is not yet modelled). Appending
/// to `projects_by_parent` is deduped so repeated listener callbacks don't
/// bloat the list.
fn apply_registry_change(
    ui_parent: &[u8; 32],
    registry: &ProjectRegistry,
    project_heads: &DashMap<[u8; 32], ContentRef>,
    projects_by_parent: &DashMap<[u8; 32], Vec<[u8; 32]>>,
    project_names: &DashMap<[u8; 32], String>,
) {
    let mut list = projects_by_parent.entry(*ui_parent).or_default();
    for (pid, entry) in &registry.projects {
        project_heads.insert(*pid, entry.manifest_head);
        project_names.insert(*pid, entry.name.clone());
        if !list.contains(pid) {
            list.push(*pid);
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

/// Full 64-char lowercase hex of a 32-byte id. Used for project folder names
/// so distinct projects cannot collide on the 6-char `short_hex` prefix.
fn hex_bytes(id: &[u8; 32]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

/// Move every loose top-level regular file in `parent_root` into `project_dir`
/// — but only if `project_dir` is currently empty of non-dotfile content.
///
/// This gate makes the migration idempotent across boots: once any file lives
/// in the default project (whether promoted here or added by the user), the
/// function returns without touching anything. Skips directories (including
/// `projects/` itself) and dotfiles (`.obsidian`, `.DS_Store`, …) at the
/// parent root so editor state and nested sub-vaults are preserved.
async fn promote_loose_files_if_empty(
    parent_root: &std::path::Path,
    project_dir: &std::path::Path,
) -> std::io::Result<()> {
    // Bail if the project folder doesn't exist or already has any non-dotfile
    // content — including directories, which signals prior user activity.
    if !project_dir.exists() {
        return Ok(());
    }
    let mut project_entries = tokio::fs::read_dir(project_dir).await?;
    while let Some(entry) = project_entries.next_entry().await? {
        let name = entry.file_name();
        if let Some(n) = name.to_str() {
            if !n.starts_with('.') {
                return Ok(());
            }
        }
    }

    let mut entries = tokio::fs::read_dir(parent_root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else { continue };
        if name_str.starts_with('.') {
            continue;
        }
        let meta = entry.file_type().await?;
        if !meta.is_file() {
            continue;
        }
        let dest = project_dir.join(&name);
        if tokio::fs::metadata(&dest).await.is_ok() {
            continue;
        }
        if let Err(e) = tokio::fs::rename(entry.path(), &dest).await {
            warn!(file = %name_str, error = %e, "could not move into default project");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Stand up a minimally-wired `VaultManager` with one registered parent
    /// vault directory so project tests don't need the full network stack.
    async fn manager_with_parent(
        tmp: &TempDir,
        parent: [u8; 32],
    ) -> Arc<VaultManager> {
        let vm = Arc::new(
            VaultManager::new(tmp.path().to_path_buf())
                .await
                .expect("manager"),
        );
        // Pre-register the parent vault path without standing up a real Vault;
        // create_project only reads `paths`, never the Vault handle.
        let parent_dir = tmp.path().join("vaults").join("parent");
        tokio::fs::create_dir_all(&parent_dir).await.unwrap();
        vm.paths.insert(parent, parent_dir);
        vm
    }

    #[tokio::test]
    async fn project_path_composes_under_parent_vault() {
        let tmp = TempDir::new().unwrap();
        let parent = [7u8; 32];
        let vm = manager_with_parent(&tmp, parent).await;

        let project_id = [9u8; 32];
        let got = vm.project_path(&parent, &project_id).expect("path");
        let expected = tmp
            .path()
            .join("vaults")
            .join("parent")
            .join("projects")
            .join(hex_bytes(&project_id));
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn project_path_returns_none_for_unknown_parent() {
        let tmp = TempDir::new().unwrap();
        let vm = Arc::new(
            VaultManager::new(tmp.path().to_path_buf()).await.unwrap(),
        );
        assert!(vm.project_path(&[0u8; 32], &[1u8; 32]).is_none());
    }

    #[tokio::test]
    async fn create_project_then_open_round_trip() {
        let tmp = TempDir::new().unwrap();
        let parent = [1u8; 32];
        let vm = manager_with_parent(&tmp, parent).await;

        let info = vm
            .create_project(&parent, "my-project")
            .await
            .expect("create");
        assert_eq!(info.parent, parent);
        assert_eq!(info.name, "my-project");
        // Empty manifest blob is real and present.
        assert!(info.manifest_head.size > 0);

        let project_dir = vm.project_path(&parent, &info.id).expect("path");
        assert!(project_dir.exists(), "project folder created on disk");

        // Wipe and re-materialize via open_project to confirm the head is
        // persisted and the empty manifest survives a round-trip.
        tokio::fs::remove_dir_all(&project_dir).await.unwrap();
        vm.open_project(&parent, &info.id).await.expect("open");
        assert!(project_dir.exists(), "project folder re-materialized");
    }

    #[test]
    fn snapshot_all_projects_noop_when_unchanged() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let parent = [3u8; 32];
            let vm = manager_with_parent(&tmp, parent).await;

            let info = vm
                .create_project(&parent, "noop-proj")
                .await
                .expect("create");
            let original_head = info.manifest_head;

            // Immediately snapshot — directory is empty, byte-identical to
            // the empty manifest blob stored at create time.
            let changed = vm
                .snapshot_all_projects()
                .await
                .expect("snapshot");
            assert!(changed.is_empty(), "no files changed, changed list must be empty");

            // Head must be unchanged.
            let head_after = vm.project_heads.get(&info.id).map(|e| *e.value());
            assert_eq!(head_after, Some(original_head), "head must not change");
        });
    }

    #[test]
    fn snapshot_all_projects_detects_file_add() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let parent = [4u8; 32];
            let vm = manager_with_parent(&tmp, parent).await;

            let info = vm
                .create_project(&parent, "file-add-proj")
                .await
                .expect("create");
            let original_head = info.manifest_head;

            // Write a file into the project folder synchronously so no
            // extra async executor setup is needed.
            let project_dir = vm.project_path(&parent, &info.id).expect("path");
            std::fs::write(project_dir.join("hello.txt"), b"hello world").unwrap();

            let changed = vm
                .snapshot_all_projects()
                .await
                .expect("snapshot");
            assert_eq!(
                changed,
                vec![info.id],
                "project with new file must appear in changed list"
            );

            // Head must differ from the empty-manifest head.
            let new_head =
                vm.project_heads.get(&info.id).map(|e| *e.value()).unwrap();
            assert_ne!(new_head, original_head, "head must update after file add");
        });
    }

    #[tokio::test]
    async fn projects_of_lists_registered_projects() {
        let tmp = TempDir::new().unwrap();
        let parent = [2u8; 32];
        let vm = manager_with_parent(&tmp, parent).await;

        assert!(vm.projects_of(&parent).is_empty());

        let a = vm.create_project(&parent, "a").await.unwrap();
        let b = vm.create_project(&parent, "b").await.unwrap();

        let listed = vm.projects_of(&parent);
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&a.id));
        assert!(listed.contains(&b.id));

        // Different parent has no projects.
        assert!(vm.projects_of(&[99u8; 32]).is_empty());
    }

    #[tokio::test]
    async fn default_project_promotes_loose_files_once() {
        let tmp = TempDir::new().unwrap();
        let parent = [3u8; 32];
        let vm = manager_with_parent(&tmp, parent).await;
        let parent_dir = vm.vault_path(&parent).expect("parent dir");

        // Seed the parent vault with a loose file, a dotfile, and a sub-dir.
        tokio::fs::write(parent_dir.join("HelloWorld.md"), "hello").await.unwrap();
        tokio::fs::write(parent_dir.join(".DS_Store"), b"\0").await.unwrap();
        tokio::fs::create_dir_all(parent_dir.join(".obsidian")).await.unwrap();
        tokio::fs::create_dir_all(parent_dir.join("nested_dir")).await.unwrap();

        let pid = vm.default_project(&parent).await.expect("default");
        let project_dir = vm.project_path(&parent, &pid).expect("path");

        // Regular file moved in; dotfile + dirs preserved at realm root.
        assert!(project_dir.join("HelloWorld.md").exists(),
            "loose file must move into the Home project");
        assert!(!parent_dir.join("HelloWorld.md").exists(),
            "source must be gone after promotion");
        assert!(parent_dir.join(".DS_Store").exists(), "dotfile must stay");
        assert!(parent_dir.join(".obsidian").is_dir(), "dotdir must stay");
        assert!(parent_dir.join("nested_dir").is_dir(), "subdir must stay");

        // Promotion is one-shot: a new file dropped at the root afterwards
        // does NOT get moved because the default project already exists.
        tokio::fs::write(parent_dir.join("Later.md"), "late").await.unwrap();
        let _ = vm.default_project(&parent).await.expect("re-resolve");
        assert!(parent_dir.join("Later.md").exists(),
            "files added after default creation must remain at the realm root");
    }

    #[tokio::test]
    async fn default_project_migrates_when_folder_still_empty() {
        // Simulates the real-world case where a user had a default project
        // created before the migration existed: the project exists in the
        // registry but its folder is empty, while loose files sit at the
        // realm root. The next default_project call should pick those up.
        let tmp = TempDir::new().unwrap();
        let parent = [5u8; 32];
        let vm = manager_with_parent(&tmp, parent).await;
        let parent_dir = vm.vault_path(&parent).expect("parent dir");

        // Create the default with no loose files yet (folder stays empty).
        let pid = vm.default_project(&parent).await.expect("create default");
        let project_dir = vm.project_path(&parent, &pid).expect("path");
        assert!(project_dir.exists());

        // Now drop a loose file at the realm root; the folder is still empty.
        tokio::fs::write(parent_dir.join("HelloWorld.md"), "hi").await.unwrap();

        // Resolve the default again — this should migrate the loose file.
        let pid_again = vm.default_project(&parent).await.expect("resolve");
        assert_eq!(pid_again, pid, "same default project id returned");

        assert!(project_dir.join("HelloWorld.md").exists(),
            "loose file must migrate into still-empty default");
        assert!(!parent_dir.join("HelloWorld.md").exists(),
            "source must be gone after migration");
    }
}
