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

use crate::braid::agent_braid::AgentBraid;
use crate::braid::dag::BraidDag;
use crate::braid::RealmBraid;
use relay_sync::RelayBlobSync;
use sync_to_disk::SyncToDisk;
use trust::LocalTrustStore;
use vault_document::VaultFileDocument;
use vault_file::{UserId, VaultFile};
use watcher::{should_ignore, VaultWatcher};

use indras_crypto::PQIdentity;
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
    /// Our PQ signing identity (ML-DSA-65) for signing changesets.
    pq_identity: PQIdentity,
    /// File system watcher (local -> local index).
    watcher: Option<VaultWatcher>,
    /// Sync-to-disk task (DAG -> local).
    sync: Option<SyncToDisk>,
    /// Relay blob sync (push/pull file content via relay).
    relay: Option<Arc<RelayBlobSync>>,
    /// Per-peer trust store (local-only, persisted to disk).
    trust_store: Arc<RwLock<LocalTrustStore>>,
    /// Inner braid: local-only DAG for agent-level work. Agent commits go
    /// here; the user merges agent HEADs and then promotes to the outer DAG.
    inner_braid: Arc<RwLock<AgentBraid>>,
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
        let pq_identity = network.node().pq_identity().clone();
        let user_id = pq_identity.user_id();

        // Set up relay blob sync: local relay for pulling (no peer relay yet)
        let relay = relay_sync::connect_relays(
            network,
            realm.node_arc(),
            None,
            realm.id(),
        )
        .await;

        let vault = Self::setup(realm, vault_path, member_id, user_id, pq_identity, relay, blob_store).await?;
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
        let pq_identity = network.node().pq_identity().clone();
        let user_id = pq_identity.user_id();

        // Set up relay blob sync: push to creator's relay, pull from local relay
        let relay = relay_sync::connect_relays(
            network,
            realm.node_arc(),
            creator_relay_addr,
            realm.id(),
        )
        .await;

        Self::setup(realm, vault_path, member_id, user_id, pq_identity, relay, blob_store).await
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
        let pq_identity = network.node().pq_identity().clone();
        let user_id = pq_identity.user_id();
        let relay = relay_sync::connect_relays(
            network,
            realm.node_arc(),
            None,
            realm.id(),
        )
        .await;
        Self::setup(realm, vault_path, member_id, user_id, pq_identity, relay, blob_store).await
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
        pq_identity: PQIdentity,
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

        // Publish our PQ verifying key to the peer key directory.
        let key_dir = realm
            .document::<crate::peer_key_directory::PeerKeyDirectory>("peer-keys")
            .await?;
        let vk_bytes = pq_identity.verifying_key_bytes();
        key_dir
            .update(|d| {
                d.publish(user_id, vk_bytes);
            })
            .await?;

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
            pq_identity.clone(),
        );

        info!(
            vault = %vault_path.display(),
            relay = relay.is_some(),
            "Vault started"
        );

        let inner_braid = Arc::new(RwLock::new(AgentBraid::new(
            user_id,
            Arc::clone(&blob_store),
        )));

        Ok(Self {
            realm,
            local_index,
            dag,
            vault_path,
            blob_store,
            member_id,
            user_id,
            pq_identity,
            watcher: Some(watcher),
            sync: Some(sync),
            relay,
            trust_store,
            inner_braid,
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

    /// Materialize the file versions named by a [`SymlinkIndex`].
    ///
    /// For each `(path, addr)` entry:
    /// 1. Fetch the blob from the local content-addressed store; if absent,
    ///    pull from the relay (when configured) and retry.
    /// 2. Write the bytes to disk via
    ///    [`write_file_content`](Self::write_file_content), which handles
    ///    directory creation, watcher suppression, and local index update.
    ///
    /// This is the "checkout" primitive: the braid layer calls this with an
    /// index from a verified changeset to replay a peer's verified vault
    /// state locally. Fails if any blob cannot be loaded.
    pub async fn apply_index(
        &self,
        index: &super::content_addr::SymlinkIndex,
    ) -> Result<()> {
        use indras_storage::ContentRef;

        for (path, addr) in index.iter() {
            let content_ref = ContentRef::new(addr.hash, addr.size);
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
                    "apply_index: blob for {} not available: {e}",
                    path
                ))
            })?;
            self.write_file_content(path.as_str(), &bytes).await?;
        }
        Ok(())
    }

    /// Legacy alias for [`apply_index`](Self::apply_index).
    pub async fn apply_manifest(
        &self,
        manifest: &super::braid::PatchManifest,
    ) -> Result<()> {
        let index: super::content_addr::SymlinkIndex = manifest.into();
        self.apply_index(&index).await
    }

    /// Check out a braid changeset: apply its [`SymlinkIndex`] to the vault.
    ///
    /// Looks up the changeset by id in the vault's braid DAG and calls
    /// [`apply_index`](Self::apply_index). Returns an error if the
    /// changeset is unknown locally (the DAG must have propagated first).
    pub async fn checkout(
        &self,
        change_id: super::braid::ChangeId,
    ) -> Result<()> {
        let index = {
            let guard = self.dag.read().await;
            match guard.get(&change_id) {
                Some(cs) => cs.index.clone(),
                None => {
                    return Err(std::io::Error::other(format!(
                        "unknown changeset: {change_id}"
                    ))
                    .into());
                }
            }
        };
        self.apply_index(&index).await
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

    /// Number of files changed since the last sync.
    pub fn dirty_count(&self) -> usize {
        match self.watcher {
            Some(ref w) => w.dirty_paths.len(),
            None => 0,
        }
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

    /// Get the PQ signing identity.
    pub fn pq_identity(&self) -> &PQIdentity {
        &self.pq_identity
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
    /// Checks for dirty paths (anything changed since last sync), then
    /// snapshots the **entire** local index into a [`SymlinkIndex`] — a
    /// full snapshot of the vault state at commit time, not a delta.
    /// Pushes blobs to the relay, creates a changeset with
    /// `Evidence::Human`, inserts it into the DAG, and updates this
    /// peer's HEAD. This is the "sync button" action.
    pub async fn sync(
        &self,
        intent: String,
        message: Option<String>,
    ) -> Result<super::braid::ChangeId> {
        use super::braid::changeset::{Changeset, Evidence};
        use super::content_addr::{ContentAddr, LogicalPath, SymlinkIndex};

        // 1. Check dirty paths — gate against no-op syncs.
        let dirty = match self.watcher {
            Some(ref w) => w.take_dirty(),
            None => Vec::new(),
        };
        if dirty.is_empty() {
            return Err(std::io::Error::other("nothing to sync").into());
        }

        // 2. Build SymlinkIndex as a FULL SNAPSHOT of all active files.
        //    Dirty paths gate whether we sync at all, but the index
        //    captures the complete vault state so that merge_from_peer
        //    can compute correct unions and detect deletions.
        let local = self.local_index.read().await;
        let mut index = SymlinkIndex::new();
        for vf in local.active_files() {
            if !vf.deleted {
                index.set(
                    LogicalPath::new(&vf.path),
                    ContentAddr::new(vf.hash, vf.size),
                );
            }
        }
        drop(local);

        if index.is_empty() {
            return Err(std::io::Error::other("no active files to sync").into());
        }

        // 3. Push blobs to relay (deferred from watcher time).
        if let Some(ref relay) = self.relay {
            for (path, addr) in index.iter() {
                let content_ref = indras_storage::ContentRef::new(addr.hash, addr.size);
                match self.blob_store.load(&content_ref).await {
                    Ok(data) => {
                        if let Err(e) = relay.push_blob(&addr.hash, &data).await {
                            tracing::warn!(path = %path, error = %e, "blob relay push failed");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(path = %path, error = %e, "blob not in local store during sync");
                    }
                }
            }
        }

        // 4. Create Evidence::Human.
        let evidence = Evidence::human(self.user_id, message);

        // 5. Build changeset with parents = my current head or DAG heads.
        let dag_guard = self.dag.read().await;
        let (parents, parent_index) = match dag_guard.peer_head(&self.user_id) {
            Some(ps) => (vec![ps.head], Some(ps.head_index.clone())),
            None => {
                let heads = dag_guard.heads();
                if heads.is_empty() {
                    (Vec::new(), None)
                } else {
                    let mut h: Vec<_> = heads.into_iter().collect();
                    h.sort();
                    (h, None)
                }
            }
        };
        drop(dag_guard);

        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let changeset = Changeset::with_index(
            self.user_id,
            parents,
            intent,
            index.clone(),
            parent_index.as_ref(),
            evidence,
            timestamp_millis,
            &self.pq_identity,
        );
        let change_id = changeset.id;

        // 6. Insert into DAG and update my peer head.
        self.dag
            .update(|d| {
                d.insert(changeset);
                d.update_peer_head(self.user_id, change_id, index);
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
        use super::braid::changeset::{Changeset, Evidence};
        use super::content_addr::SymlinkIndex;

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

        // Merge indexes (both are full snapshots):
        // - Start with the peer's snapshot (peer wins on path conflicts).
        // - Add entries from my snapshot that the peer doesn't have
        //   (files I created that the peer hasn't seen yet).
        let my_index = my_state
            .as_ref()
            .map(|ms| &ms.head_index)
            .cloned()
            .unwrap_or_default();
        let mut merged = peer_state.head_index.clone();
        // Add my entries that the peer doesn't have (my unique additions).
        for (path, addr) in my_index.iter() {
            if merged.get(path).is_none() {
                merged.set(path.clone(), *addr);
            }
        }

        let evidence = Evidence::human(self.user_id, Some("merge".to_string()));
        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let changeset = Changeset::with_index(
            self.user_id,
            parents,
            format!("merge from peer {}", hex::encode(&peer_id[..4])),
            merged.clone(),
            Some(&my_index),
            evidence,
            timestamp_millis,
            &self.pq_identity,
        );
        let change_id = changeset.id;

        // Insert into DAG and update our HEAD.
        self.dag
            .update(|d| {
                d.insert(changeset);
                d.update_peer_head(self.user_id, change_id, merged.clone());
            })
            .await?;

        // Materialize the merged index to disk.
        self.apply_index(&merged).await?;

        // Remove files from disk that were in my old index but are
        // absent from the merged index (peer deletions).
        for (path, _) in my_index.iter() {
            if merged.get(path).is_none() {
                let _ = self.delete_file_content(path.as_str()).await;
            }
        }

        info!(change = %change_id, peer = %hex::encode(&peer_id[..4]), "Merged from peer");
        Ok(change_id)
    }

    /// Show what changed between my head and a peer's head as an [`IndexDelta`].
    pub async fn diff_fork(
        &self,
        peer_id: UserId,
    ) -> super::content_addr::IndexDelta {
        let dag = self.dag.read().await;
        let peer_state = match dag.peer_head(&peer_id) {
            Some(ps) => ps,
            None => return super::content_addr::IndexDelta::new(),
        };
        let my_index = match dag.peer_head(&self.user_id) {
            Some(ms) => &ms.head_index,
            None => {
                return super::content_addr::IndexDelta::from_root(&peer_state.head_index);
            }
        };
        peer_state.head_index.diff(my_index)
    }

    /// Auto-merge all pending forks from trusted peers.
    ///
    /// Returns a list of `(peer_id, merge_change_id)` for each merge
    /// that was performed.
    pub async fn auto_merge_trusted(&self) -> Vec<(UserId, super::braid::ChangeId)> {
        let forks = self.pending_forks().await;
        let mut merged = Vec::new();
        for (peer_id, _) in forks {
            if self.is_peer_trusted(&peer_id).await {
                if let Ok(id) = self.merge_from_peer(peer_id).await {
                    merged.push((peer_id, id));
                }
            }
        }
        merged
    }

    // ── Inner braid routing (Phase 3) ──────────────────────────────────

    /// Access the inner (agent-level, local-only) braid.
    pub fn inner_braid(&self) -> &Arc<RwLock<AgentBraid>> {
        &self.inner_braid
    }

    /// Promote the user's inner HEAD to the outer peer-synced DAG.
    ///
    /// Bridges merged agent work into the peer-visible braid: takes the
    /// current inner HEAD's `SymlinkIndex`, creates a signed changeset in
    /// the outer DAG parented on the current outer HEAD, inserts it, and
    /// advances the user's outer peer_head to the promoted state.
    ///
    /// Errors if there is no inner HEAD to promote.
    pub async fn promote(
        &self,
        intent: String,
    ) -> Result<super::braid::ChangeId> {
        use super::braid::changeset::{Changeset, Evidence};

        // 1. Snapshot the user's inner HEAD.
        let inner_index = {
            let guard = self.inner_braid.read().await;
            guard
                .user_head()
                .ok_or_else(|| std::io::Error::other("nothing to promote"))?
                .head_index
                .clone()
        };

        // 2. Snapshot the outer HEAD for parent linkage.
        let outer_head = {
            let guard = self.dag.read().await;
            guard.peer_head(&self.user_id).cloned()
        };

        let parents = outer_head
            .as_ref()
            .map(|h| vec![h.head])
            .unwrap_or_default();
        let parent_index = outer_head.as_ref().map(|h| h.head_index.clone());

        let evidence = Evidence::human(self.user_id, Some(intent.clone()));
        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let changeset = Changeset::with_index(
            self.user_id,
            parents,
            intent,
            inner_index.clone(),
            parent_index.as_ref(),
            evidence,
            timestamp_millis,
            &self.pq_identity,
        );
        let change_id = changeset.id;

        let head_index = inner_index.clone();
        self.dag
            .update(|d| {
                d.insert(changeset);
                d.update_peer_head(self.user_id, change_id, head_index.clone());
            })
            .await?;

        // Inner-braid GC policy: aggressively roll up to the user HEAD
        // post-promote. The inner DAG's historical changesets have
        // already been captured in the outer signed changeset we just
        // inserted; keeping them around would let the inner DAG grow
        // unbounded across promote cycles.
        self.inner_braid.write().await.rollup_to_user_head();

        info!(change = %change_id, "Promoted inner HEAD to outer DAG");
        Ok(change_id)
    }

    /// Garbage-collect unreferenced blobs from the shared blob store.
    ///
    /// Builds the union of every [`ContentAddr`] referenced by the outer
    /// peer-synced DAG (via `all_referenced_addrs`) and the inner
    /// agent-local braid, then runs [`BlobStore::gc`] with a keep-set
    /// predicate: any blob whose address is in the union is retained,
    /// everything else is deleted.
    ///
    /// Call after a rollup or when free-disk pressure warrants it.
    pub async fn gc_blobs(&self) -> Result<indras_storage::GcResult> {
        use super::content_addr::ContentAddr;

        let outer_refs = self.dag.read().await.all_referenced_addrs();
        let inner_refs = self.inner_braid.read().await.all_referenced_addrs();
        let all_refs: std::collections::HashSet<ContentAddr> =
            outer_refs.union(&inner_refs).copied().collect();

        let result = self
            .blob_store
            .gc(|cr| all_refs.contains(&ContentAddr::from(*cr)))
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        info!(
            deleted = result.deleted_count,
            retained = result.retained_count,
            bytes_freed = result.bytes_freed,
            "Blob GC complete"
        );
        Ok(result)
    }

    /// Route an agent's verified changeset into the inner braid.
    ///
    /// Thin wrapper around [`AgentBraid::agent_land`] that takes the
    /// write lock. Returns the new [`ChangeId`] in the inner DAG.
    pub async fn agent_land(
        &self,
        agent: &crate::team::LogicalAgentId,
        intent: String,
        index: crate::content_addr::SymlinkIndex,
        evidence: super::braid::changeset::Evidence,
    ) -> super::braid::ChangeId {
        let mut inner = self.inner_braid.write().await;
        inner.agent_land(agent, intent, index, evidence)
    }

    /// Merge an agent's inner HEAD into the user's inner HEAD.
    ///
    /// Thin wrapper around [`AgentBraid::merge_agent`] that takes the
    /// write lock. Returns `None` if the agent has no HEAD yet.
    pub async fn merge_agent(
        &self,
        agent: &crate::team::LogicalAgentId,
    ) -> Option<super::braid::agent_braid::MergeResult> {
        let mut inner = self.inner_braid.write().await;
        inner.merge_agent(agent)
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
