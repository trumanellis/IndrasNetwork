//! Vault — high-level P2P vault sync orchestrator.
//!
//! Ties together the vault-index document, file watcher, blob store,
//! and sync-to-disk into a single ergonomic API.

use crate::realm_vault::RealmVault;
use crate::relay_sync::RelayBlobSync;
use crate::sync_to_disk::SyncToDisk;
use crate::vault_document::VaultFileDocument;
use crate::vault_file::{ConflictRecord, UserId, VaultFile};
use crate::watcher::{should_ignore, VaultWatcher};

use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::{IndrasNetwork, InviteCode, Realm};
use indras_storage::{BlobStore, BlobStoreConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// A P2P-synced vault directory.
///
/// Each vault maps to a Realm. Files are tracked in a CRDT document
/// (`VaultFileDocument`) with LWW-per-file merge and conflict detection.
///
/// The vault holds a single cached `Document<VaultFileDocument>` handle
/// so that all operations share the same in-memory state. This avoids
/// stale reads that occur when creating fresh Document handles per call.
pub struct Vault {
    /// The realm backing this vault.
    realm: Realm,
    /// Cached vault-index document (shared state via Arc<RwLock>).
    doc: Document<VaultFileDocument>,
    /// Path to the vault directory on disk.
    vault_path: PathBuf,
    /// Content-addressed blob storage.
    blob_store: Arc<BlobStore>,
    /// Our member ID (device-level, from iroh transport key).
    member_id: MemberId,
    /// Our user ID (user-level, from PQ signing key — shared across devices).
    user_id: UserId,
    /// File system watcher (local -> network).
    watcher: Option<VaultWatcher>,
    /// Sync-to-disk task (network -> local).
    sync: Option<SyncToDisk>,
    /// Relay blob sync (push/pull file content via relay).
    relay: Option<Arc<RelayBlobSync>>,
}

impl Vault {
    /// Create a new vault and its backing realm.
    ///
    /// Returns the vault and the realm's invite code for sharing.
    /// If `relay_addr` is provided, connects to the relay for blob replication.
    pub async fn create(
        network: &IndrasNetwork,
        name: &str,
        vault_path: PathBuf,
    ) -> Result<(Self, InviteCode)> {
        let realm = network.create_realm(name).await?;
        let invite = realm
            .invite_code()
            .cloned()
            .expect("newly created realm should have invite code");
        let member_id = network.id();
        let user_id = network.node().pq_identity().user_id();

        // Set up relay blob sync: local relay for pulling (no peer relay yet)
        let relay = crate::relay_sync::connect_relays(
            network,
            realm.node_arc(),
            None,
            realm.id(),
        )
        .await;

        let vault = Self::setup(realm, vault_path, member_id, user_id, relay).await?;
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
        let relay = crate::relay_sync::connect_relays(
            network,
            realm.node_arc(),
            creator_relay_addr,
            realm.id(),
        )
        .await;

        Self::setup(realm, vault_path, member_id, user_id, relay).await
    }

    /// Common setup: initialize blob store, watcher, and sync-to-disk.
    async fn setup(
        realm: Realm,
        vault_path: PathBuf,
        member_id: MemberId,
        user_id: UserId,
        relay: Option<Arc<RelayBlobSync>>,
    ) -> Result<Self> {
        // Ensure vault directory exists
        tokio::fs::create_dir_all(&vault_path).await?;

        // Initialize blob store under vault_path/.indras/blobs/
        let blob_dir = vault_path.join(".indras/blobs");
        let blob_config = BlobStoreConfig {
            base_dir: blob_dir,
            ..Default::default()
        };
        let blob_store = Arc::new(
            BlobStore::new(blob_config)
                .await
                .map_err(|e| std::io::Error::other(e.to_string()))?,
        );

        // Start blob listener (receives blobs from peers via direct transport)
        if let Some(ref rs) = relay {
            crate::relay_sync::start_listener_spawned(rs, Arc::clone(&blob_store));
        }

        // Create a single cached document handle for the vault index.
        // All reads and writes go through this handle, ensuring consistent state.
        // Created before the watcher so they share the same Document.
        let doc = realm.vault_index().await?;

        // Start watcher (local FS -> vault-index) using a clone of the cached doc handle
        let watcher = VaultWatcher::start(
            vault_path.clone(),
            doc.clone(),
            Arc::clone(&blob_store),
            user_id,
            relay.clone(),
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        // Start sync-to-disk (vault-index -> local FS) using a clone of the doc handle
        let sync = SyncToDisk::start(
            doc.clone(),
            vault_path.clone(),
            Arc::clone(&blob_store),
            &watcher,
            relay.clone(),
        );

        info!(
            vault = %vault_path.display(),
            relay = relay.is_some(),
            "Vault started"
        );

        Ok(Self {
            realm,
            doc,
            vault_path,
            blob_store,
            member_id,
            user_id,
            watcher: Some(watcher),
            sync: Some(sync),
            relay,
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

                    self.upsert_file(&rel_path, hash, size, self.user_id, Some(data))
                        .await?;
                    count += 1;
                }
            }
        }

        info!(count, "Initial vault scan complete");
        Ok(count)
    }

    /// Insert or update a file in the vault index.
    pub async fn upsert_file(
        &self,
        path: &str,
        hash: [u8; 32],
        size: u64,
        author: UserId,
        data: Option<Vec<u8>>,
    ) -> Result<()> {
        let file = match data {
            Some(d) => VaultFile::with_content(path, hash, size, author, d),
            None => VaultFile::new(path, hash, size, author),
        };
        self.doc.update(|d| d.upsert(file)).await
    }

    /// Mark a file as deleted (tombstone) in the vault index.
    pub async fn delete_file(&self, path: &str, author: UserId) -> Result<()> {
        self.doc.update(|d| d.remove(path, author)).await
    }

    /// Write file content to disk, store blob, and update the CRDT index.
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

        // Update CRDT index (inline content for small files)
        self.upsert_file(rel_path, hash, size, self.user_id, Some(data.to_vec())).await
    }

    /// Delete a file from disk and mark it as deleted in the CRDT index.
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

        // Mark deleted in index
        self.delete_file(rel_path, self.user_id).await
    }

    /// List all active (non-deleted) files in the vault.
    pub async fn list_files(&self) -> Vec<VaultFile> {
        self.doc
            .read()
            .await
            .active_files()
            .into_iter()
            .cloned()
            .collect()
    }

    /// List all unresolved conflicts.
    pub async fn list_conflicts(&self) -> Vec<ConflictRecord> {
        self.doc
            .read()
            .await
            .unresolved_conflicts()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Resolve a conflict by marking it as resolved.
    pub async fn resolve_conflict(&self, path: &str, loser_hash: &[u8; 32]) -> Result<()> {
        self.doc
            .update(|d| d.resolve_conflict(path, loser_hash))
            .await
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
            crate::relay_sync::add_peer_relay(relay, network, peer_addr).await
        } else {
            false
        }
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
