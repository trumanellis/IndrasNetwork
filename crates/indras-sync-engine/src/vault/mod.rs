//! Vault — high-level P2P vault sync orchestrator.
//!
//! Ties together the braid DAG, local file index, file watcher, blob store,
//! and sync-from-DAG into a single ergonomic API. The braid DAG is the
//! single source of truth for file state across peers; the local index
//! tracks the current checkout on this device.

pub mod relay_sync;
pub mod sync_to_disk;
pub mod trust;
pub mod vault_document;
pub mod vault_file;
pub mod watcher;

use crate::braid::dag::BraidDag;
use crate::braid::RealmBraid;
use relay_sync::RelayBlobSync;
use sync_to_disk::SyncToDisk;
use trust::LocalTrustStore;
use vault_document::VaultFileDocument;
use vault_file::{UserId, VaultFile};
use watcher::{should_ignore, VaultWatcher};

use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::{IndrasNetwork, InviteCode, Realm};
use indras_storage::BlobStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

/// A P2P-synced vault directory.
///
/// Each vault maps to a Realm. The braid DAG (on the vault realm) is the
/// shared source of truth. The local index (`VaultFileDocument`) tracks
/// what's currently checked out on this device — it is NOT a shared CRDT.
pub struct Vault {
    /// The realm backing this vault.
    realm: Realm,
    /// Local-only file index — tracks current checkout state on this device.
    local_index: Arc<RwLock<VaultFileDocument>>,
    /// Braid DAG document — the single source of truth for VCS state.
    dag: Document<BraidDag>,
    /// Path to the vault directory on disk.
    vault_path: PathBuf,
    /// Content-addressed blob storage.
    blob_store: Arc<BlobStore>,
    /// Our member ID (device-level, from iroh transport key).
    member_id: MemberId,
    /// Our user ID (user-level, from PQ signing key — shared across devices).
    user_id: UserId,
    /// File system watcher (local -> local index).
    watcher: Option<VaultWatcher>,
    /// Sync-to-disk task (DAG -> local).
    sync: Option<SyncToDisk>,
    /// Relay blob sync (push/pull file content via relay).
    relay: Option<Arc<RelayBlobSync>>,
    /// Per-peer trust store (local-only, persisted to disk).
    trust_store: Arc<RwLock<LocalTrustStore>>,
}

impl Vault {
    /// Create a new vault and its backing realm.
    ///
    /// Returns the vault and the realm's invite code for sharing.
    pub async fn create(
        network: &IndrasNetwork,
        name: &str,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
    ) -> Result<(Self, InviteCode)> {
        let realm = network.create_realm(name).await?;
        let invite = realm
            .invite_code()
            .cloned()
            .expect("newly created realm should have invite code");
        let member_id = network.id();
        let user_id = network.node().pq_identity().user_id();

        // Set up relay blob sync: local relay for pulling (no peer relay yet)
        let relay = relay_sync::connect_relays(
            network,
            realm.node_arc(),
            None,
            realm.id(),
        )
        .await;

        let vault = Self::setup(realm, vault_path, member_id, user_id, relay, blob_store).await?;
        Ok((vault, invite))
    }

    /// Join an existing vault using an invite code.
    ///
    /// Connects to both own relay (for pushing) and the creator's relay
    /// (from the invite bootstrap peers) for pulling, so blobs flow
    /// bidirectionally between peers.
    pub async fn join(
        network: &IndrasNetwork,
        invite: &str,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
    ) -> Result<Self> {
        // Parse invite to extract creator's relay address BEFORE consuming it
        let invite_code = InviteCode::parse(invite)?;
        let creator_relay_addr = invite_code
            .invite_key()
            .bootstrap_peers
            .first()
            .and_then(|bytes| postcard::from_bytes::<iroh::EndpointAddr>(bytes).ok());

        let realm = network.join(invite).await?;
        let member_id = network.id();
        let user_id = network.node().pq_identity().user_id();

        // Set up relay blob sync: push to creator's relay, pull from local relay
        let relay = relay_sync::connect_relays(
            network,
            realm.node_arc(),
            creator_relay_addr,
            realm.id(),
        )
        .await;

        Self::setup(realm, vault_path, member_id, user_id, relay, blob_store).await
    }

    /// Attach vault sync to an existing realm.
    ///
    /// Use this when the realm was created or joined through normal network
    /// APIs and you want to add bidirectional file sync to it. The vault
    /// directory is created if it doesn't exist.
    pub async fn attach(
        network: &IndrasNetwork,
        realm: Realm,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
    ) -> Result<Self> {
        let member_id = network.id();
        let user_id = network.node().pq_identity().user_id();
        let relay = relay_sync::connect_relays(
            network,
            realm.node_arc(),
            None,
            realm.id(),
        )
        .await;
        Self::setup(realm, vault_path, member_id, user_id, relay, blob_store).await
    }

    /// Common setup: wire up watcher, sync-to-disk, and relay.
    ///
    /// All vaults share a single content-addressed blob store so that
    /// identical files are stored only once on disk.
    async fn setup(
        realm: Realm,
        vault_path: PathBuf,
        member_id: MemberId,
        user_id: UserId,
        relay: Option<Arc<RelayBlobSync>>,
        blob_store: Arc<BlobStore>,
    ) -> Result<Self> {
        // Ensure vault directory exists
        tokio::fs::create_dir_all(&vault_path).await?;

        // Start blob listener (receives blobs from peers via direct transport)
        if let Some(ref rs) = relay {
            relay_sync::start_listener_spawned(rs, Arc::clone(&blob_store));
        }

        // Create a local-only file index (not a shared CRDT).
        let local_index = Arc::new(RwLock::new(VaultFileDocument::default()));

        // Load per-peer trust store from disk.
        let trust_store = Arc::new(RwLock::new(LocalTrustStore::load(&vault_path).await));

        // Create the braid DAG document on the vault realm.
        let dag = realm.braid_dag().await?;

        // Start watcher (local FS -> local index)
        let watcher = VaultWatcher::start(
            vault_path.clone(),
            local_index.clone(),
            Arc::clone(&blob_store),
            user_id,
            relay.clone(),
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        // Start sync-from-DAG (DAG changes -> local FS)
        let sync = SyncToDisk::start(
            dag.clone(),
            local_index.clone(),
            vault_path.clone(),
            Arc::clone(&blob_store),
            &watcher,
            relay.clone(),
            Arc::clone(&trust_store),
            user_id,
        );

        info!(
            vault = %vault_path.display(),
            relay = relay.is_some(),
            "Vault started"
        );

        Ok(Self {
            realm,
            local_index,
            dag,
            vault_path,
            blob_store,
            member_id,
            user_id,
            watcher: Some(watcher),
            sync: Some(sync),
            relay,
            trust_store,
        })
    }

    /// Scan the vault directory and index all existing files.
    ///
    /// Returns the number of files indexed.
    pub async fn initial_scan(&self) -> Result<usize> {
        let mut count = 0usize;
        let mut dirs = vec![self.vault_path.clone()];

        while let Some(dir) = dirs.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if should_ignore(&path, &self.vault_path) {
                    continue;
                }

                let ft = entry.file_type().await?;

                if ft.is_dir() {
                    dirs.push(path);
                } else if ft.is_file() {
                    let data = tokio::fs::read(&path).await?;

                    let hash = *blake3::hash(&data).as_bytes();
                    let size = data.len() as u64;

                    // Store in blob store
                    self.blob_store
                        .store(&data)
                        .await
                        .map_err(|e| std::io::Error::other(e.to_string()))?;

                    // Push to relay for remote peers
                    if let Some(ref relay) = self.relay {
                        let _ = relay.push_blob(&hash, &data).await;
                    }

                    let rel_path = path
                        .strip_prefix(&self.vault_path)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");

                    self.upsert_file(&rel_path, hash, size, self.user_id)
                        .await;
                    count += 1;
                }
            }
        }

        info!(count, "Initial vault scan complete");
        Ok(count)
    }

    /// Insert or update a file in the local index.
    pub async fn upsert_file(
        &self,
        path: &str,
        hash: [u8; 32],
        size: u64,
        author: UserId,
    ) {
        let file = VaultFile::new(path, hash, size, author);
        self.local_index.write().await.upsert(file);
    }

    /// Mark a file as deleted (tombstone) in the local index.
    pub async fn delete_file(&self, path: &str, author: UserId) {
        self.local_index.write().await.remove(path, author);
    }

    /// Write file content to disk, store blob, and update the local index.
    ///
    /// Suppresses the watcher for this path to prevent echo (the watcher
    /// would otherwise pick up the disk write and create a redundant update).
    pub async fn write_file_content(&self, rel_path: &str, data: &[u8]) -> Result<()> {
        let full_path = self.vault_path.join(rel_path);

        // Hash content up front
        let hash = *blake3::hash(data).as_bytes();
        let size = data.len() as u64;

        // Suppress watcher echo and record the hash so the watcher
        // won't re-index this content with a new timestamp.
        if let Some(ref watcher) = self.watcher {
            watcher.suppress(&full_path, Duration::from_secs(2));
            watcher.record_hash(rel_path, hash);
        }

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write to disk
        tokio::fs::write(&full_path, data).await?;

        // Store blob
        self.blob_store
            .store(data)
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        // Push to relay for remote peers
        if let Some(ref relay) = self.relay {
            let _ = relay.push_blob(&hash, data).await;
        }

        // Update local index
        self.upsert_file(rel_path, hash, size, self.user_id).await;
        Ok(())
    }

    /// Materialize the file versions named by a [`PatchManifest`].
    ///
    /// For each `(path, hash, size)` entry:
    /// 1. Fetch the blob from the local content-addressed store; if absent,
    ///    pull from the relay (when configured) and retry.
    /// 2. Write the bytes to disk via
    ///    [`write_file_content`](Self::write_file_content), which handles
    ///    directory creation, watcher suppression, and local index update.
    ///
    /// This is the "checkout" primitive: the braid layer calls this with a
    /// manifest from a verified changeset to replay a peer's verified vault
    /// state locally. Fails if any blob cannot be loaded.
    pub async fn apply_manifest(
        &self,
        manifest: &super::braid::PatchManifest,
    ) -> Result<()> {
        use indras_storage::ContentRef;

        for pf in &manifest.files {
            let content_ref = ContentRef::new(pf.hash, pf.size);
            let mut data = self.blob_store.load(&content_ref).await;
            if data.is_err() {
                if let Some(ref relay) = self.relay {
                    for _ in 0..10 {
                        let _ = relay.pull_blobs(&self.blob_store).await;
                        data = self.blob_store.load(&content_ref).await;
                        if data.is_ok() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
            let bytes = data.map_err(|e| {
                std::io::Error::other(format!(
                    "apply_manifest: blob for {} not available: {e}",
                    pf.path
                ))
            })?;
            self.write_file_content(&pf.path, &bytes).await?;
        }
        Ok(())
    }

    /// Check out a braid changeset: apply its `PatchManifest` to the vault.
    ///
    /// Looks up the changeset by id in the vault's braid DAG and calls
    /// [`apply_manifest`](Self::apply_manifest). Returns an error if the
    /// changeset is unknown locally (the DAG must have propagated first).
    pub async fn checkout(
        &self,
        change_id: super::braid::ChangeId,
    ) -> Result<()> {
        let manifest = {
            let guard = self.dag.read().await;
            match guard.get(&change_id) {
                Some(cs) => cs.patch.clone(),
                None => {
                    return Err(std::io::Error::other(format!(
                        "unknown changeset: {change_id}"
                    ))
                    .into());
                }
            }
        };
        self.apply_manifest(&manifest).await
    }

    /// Delete a file from disk and mark it as deleted in the local index.
    ///
    /// Suppresses the watcher for this path to prevent echo.
    pub async fn delete_file_content(&self, rel_path: &str) -> Result<()> {
        let full_path = self.vault_path.join(rel_path);

        // Suppress watcher echo for this path
        if let Some(ref watcher) = self.watcher {
            watcher.suppress(&full_path, Duration::from_secs(2));
        }

        // Remove from disk
        if full_path.exists() {
            tokio::fs::remove_file(&full_path).await?;
        }

        // Mark deleted in local index
        self.delete_file(rel_path, self.user_id).await;
        Ok(())
    }

    /// List all active (non-deleted) files in the vault.
    pub async fn list_files(&self) -> Vec<VaultFile> {
        self.local_index
            .read()
            .await
            .active_files()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get a reference to the braid DAG document.
    pub fn dag(&self) -> &Document<BraidDag> {
        &self.dag
    }

    /// Get a reference to the underlying realm.
    pub fn realm(&self) -> &Realm {
        &self.realm
    }

    /// Get the vault directory path.
    pub fn path(&self) -> &Path {
        &self.vault_path
    }

    /// Get the member ID.
    pub fn member_id(&self) -> MemberId {
        self.member_id
    }

    /// Get a reference to the blob store.
    pub fn blob_store(&self) -> &Arc<BlobStore> {
        &self.blob_store
    }

    /// Get the user ID.
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    /// Get a reference to the watcher (for test access to dirty_paths).
    pub fn watcher_ref(&self) -> Option<&VaultWatcher> {
        self.watcher.as_ref()
    }

    /// Wait until the vault's realm has at least `expected` members, or timeout.
    ///
    /// Returns the actual member count when done. Useful as a convergence
    /// barrier in tests — call this after join to ensure CRDT membership
    /// has propagated before writing files.
    pub async fn await_members(
        &self,
        expected: usize,
        timeout: std::time::Duration,
    ) -> Result<usize> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let count = self.realm.member_count().await?;
            if count >= expected {
                return Ok(count);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(count);
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }

    /// Add a peer's relay for blob replication.
    ///
    /// Call this after a new peer joins the vault so the creator can
    /// push blobs to the joiner's relay. The `peer_addr` is the peer's
    /// endpoint address (from their node).
    pub async fn add_peer_relay(
        &self,
        network: &IndrasNetwork,
        peer_addr: iroh::EndpointAddr,
    ) -> bool {
        if let Some(ref relay) = self.relay {
            relay_sync::add_peer_relay(relay, network, peer_addr).await
        } else {
            false
        }
    }

    // ── Human sync path (Phase 7) ──────────────────────────────────────

    /// Explicitly sync local changes to the braid DAG.
    ///
    /// Collects dirty paths from the watcher, builds a `PatchManifest`,
    /// pushes blobs to the relay, creates a changeset with
    /// `Evidence::Human`, inserts it into the DAG, and updates this
    /// peer's HEAD. This is the "sync button" action.
    pub async fn sync(
        &self,
        intent: String,
        message: Option<String>,
    ) -> Result<super::braid::ChangeId> {
        use super::braid::changeset::{Changeset, Evidence, PatchFile, PatchManifest};

        // 1. Collect dirty paths from watcher.
        let dirty = match self.watcher {
            Some(ref w) => w.take_dirty(),
            None => Vec::new(),
        };
        if dirty.is_empty() {
            return Err(std::io::Error::other("nothing to sync").into());
        }

        // 2. Build PatchManifest from local index for dirty paths.
        let local = self.local_index.read().await;
        let mut files: Vec<PatchFile> = Vec::new();
        for path in &dirty {
            if let Some(vf) = local.files.get(path) {
                if !vf.deleted {
                    files.push(PatchFile {
                        path: vf.path.clone(),
                        hash: vf.hash,
                        size: vf.size,
                    });
                }
            }
        }
        drop(local);

        if files.is_empty() {
            return Err(std::io::Error::other("no active files to sync").into());
        }
        let manifest = PatchManifest::new(files);

        // 3. Push blobs to relay (deferred from watcher time).
        if let Some(ref relay) = self.relay {
            for pf in &manifest.files {
                let content_ref = indras_storage::ContentRef::new(pf.hash, pf.size);
                if let Ok(data) = self.blob_store.load(&content_ref).await {
                    let _ = relay.push_blob(&pf.hash, &data).await;
                }
            }
        }

        // 4. Create Evidence::Human.
        let evidence = Evidence::human(self.user_id, message);

        // 5. Build changeset with parents = my current head or DAG heads.
        let dag_guard = self.dag.read().await;
        let parents = match dag_guard.peer_head(&self.user_id) {
            Some(ps) => vec![ps.head],
            None => {
                let heads = dag_guard.heads();
                if heads.is_empty() {
                    Vec::new()
                } else {
                    let mut h: Vec<_> = heads.into_iter().collect();
                    h.sort();
                    h
                }
            }
        };
        drop(dag_guard);

        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let changeset = Changeset::new_unsigned(
            self.user_id,
            parents,
            intent,
            manifest.clone(),
            evidence,
            timestamp_millis,
        );
        let change_id = changeset.id;

        // 6. Insert into DAG and update my peer head.
        self.dag
            .update(|d| {
                d.insert(changeset);
                d.update_peer_head(self.user_id, change_id, manifest);
            })
            .await?;

        info!(change = %change_id, "Human sync committed");
        Ok(change_id)
    }

    // ── Merge consent (Phase 8) ─────────────────────────────────────────

    /// Set the trust level for a peer. Trusted peers' changes auto-merge.
    ///
    /// When transitioning from untrusted to trusted, any pending fork
    /// from that peer is auto-merged.
    pub async fn set_peer_trust(&self, peer_id: UserId, trusted: bool) -> Result<()> {
        self.trust_store.write().await.set_trust(peer_id, trusted).await;
        // If newly trusted and they have a fork, auto-merge it.
        if trusted {
            let dag = self.dag.read().await;
            let has_fork = match (dag.peer_head(&peer_id), dag.peer_head(&self.user_id)) {
                (Some(theirs), Some(mine)) => theirs.head != mine.head,
                (Some(_), None) => true,
                _ => false,
            };
            drop(dag);
            if has_fork {
                let _ = self.merge_from_peer(peer_id).await;
            }
        }
        Ok(())
    }

    /// Check if a peer is trusted (auto-merge enabled).
    pub async fn is_peer_trusted(&self, peer_id: &UserId) -> bool {
        self.trust_store.read().await.is_trusted(peer_id)
    }

    /// Return peer heads that diverge from this peer's HEAD.
    pub async fn pending_forks(
        &self,
    ) -> Vec<(UserId, super::braid::dag::PeerState)> {
        let dag = self.dag.read().await;
        let my_head = dag.peer_head(&self.user_id);
        dag.all_peer_heads()
            .iter()
            .filter(|(uid, ps)| {
                **uid != self.user_id
                    && my_head.map_or(true, |mh| ps.head != mh.head)
            })
            .map(|(uid, ps)| (*uid, ps.clone()))
            .collect()
    }

    /// Merge a peer's HEAD into ours, creating a merge changeset.
    pub async fn merge_from_peer(
        &self,
        peer_id: UserId,
    ) -> Result<super::braid::ChangeId> {
        use super::braid::changeset::{Changeset, Evidence, PatchFile, PatchManifest};

        let dag_guard = self.dag.read().await;
        let peer_state = dag_guard
            .peer_head(&peer_id)
            .ok_or_else(|| std::io::Error::other("peer has no HEAD"))?
            .clone();
        let my_state = dag_guard.peer_head(&self.user_id).cloned();
        drop(dag_guard);

        // Build merge parents: my head + their head.
        let mut parents = vec![peer_state.head];
        if let Some(ref ms) = my_state {
            parents.push(ms.head);
        }

        // Merge manifest: union of files, peer wins for conflicts (LWW).
        let mut merged: std::collections::BTreeMap<String, PatchFile> =
            std::collections::BTreeMap::new();
        if let Some(ref ms) = my_state {
            for pf in &ms.head_manifest.files {
                merged.insert(
                    pf.path.clone(),
                    PatchFile {
                        path: pf.path.clone(),
                        hash: pf.hash,
                        size: pf.size,
                    },
                );
            }
        }
        // Peer's files overwrite ours (peer wins on conflict).
        for pf in &peer_state.head_manifest.files {
            merged.insert(
                pf.path.clone(),
                PatchFile {
                    path: pf.path.clone(),
                    hash: pf.hash,
                    size: pf.size,
                },
            );
        }
        let manifest = PatchManifest::new(merged.into_values().collect());

        let evidence = Evidence::human(self.user_id, Some("merge".to_string()));
        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let changeset = Changeset::new_unsigned(
            self.user_id,
            parents,
            format!("merge from peer {}", hex::encode(&peer_id[..4])),
            manifest.clone(),
            evidence,
            timestamp_millis,
        );
        let change_id = changeset.id;

        // Insert into DAG and update our HEAD.
        self.dag
            .update(|d| {
                d.insert(changeset);
                d.update_peer_head(self.user_id, change_id, manifest.clone());
            })
            .await?;

        // Materialize the merged manifest to disk.
        self.apply_manifest(&manifest).await?;

        info!(change = %change_id, peer = %hex::encode(&peer_id[..4]), "Merged from peer");
        Ok(change_id)
    }

    /// Show what files differ between my head and a peer's head.
    pub async fn diff_fork(
        &self,
        peer_id: UserId,
    ) -> Vec<super::braid::PatchFile> {
        let dag = self.dag.read().await;
        let peer_state = match dag.peer_head(&peer_id) {
            Some(ps) => ps,
            None => return Vec::new(),
        };
        // Return the peer's manifest files that differ from ours.
        let my_files: std::collections::HashMap<String, [u8; 32]> =
            match dag.peer_head(&self.user_id) {
                Some(ms) => ms
                    .head_manifest
                    .files
                    .iter()
                    .map(|f| (f.path.clone(), f.hash))
                    .collect(),
                None => std::collections::HashMap::new(),
            };
        peer_state
            .head_manifest
            .files
            .iter()
            .filter(|pf| my_files.get(&pf.path).map_or(true, |h| *h != pf.hash))
            .cloned()
            .collect()
    }

    /// Stop the vault (watcher + sync).
    pub fn stop(mut self) {
        if let Some(w) = self.watcher.take() {
            w.stop();
        }
        if let Some(s) = self.sync.take() {
            s.stop();
        }
        info!("Vault stopped");
    }
}
