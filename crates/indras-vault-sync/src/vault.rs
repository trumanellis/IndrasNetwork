//! Vault — high-level P2P vault sync orchestrator.
//!
//! Ties together the vault-index document, file watcher, blob store,
//! and sync-to-disk into a single ergonomic API.

use crate::realm_vault::RealmVault;
use crate::sync_to_disk::SyncToDisk;
use crate::watcher::{should_ignore, VaultWatcher};

use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::{IndrasNetwork, InviteCode, Realm};
use indras_storage::{BlobStore, BlobStoreConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// A P2P-synced vault directory.
///
/// Each vault maps to a Realm. Files are tracked in a CRDT document
/// (`VaultFileDocument`) with LWW-per-file merge and conflict detection.
pub struct Vault {
    /// The realm backing this vault.
    realm: Realm,
    /// Path to the vault directory on disk.
    vault_path: PathBuf,
    /// Content-addressed blob storage.
    blob_store: Arc<BlobStore>,
    /// Our member ID.
    member_id: MemberId,
    /// File system watcher (local -> network).
    watcher: Option<VaultWatcher>,
    /// Sync-to-disk task (network -> local).
    sync: Option<SyncToDisk>,
}

impl Vault {
    /// Create a new vault and its backing realm.
    ///
    /// Returns the vault and the realm's invite code for sharing.
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

        let vault = Self::setup(realm, vault_path, member_id).await?;
        Ok((vault, invite))
    }

    /// Join an existing vault using an invite code.
    pub async fn join(
        network: &IndrasNetwork,
        invite: &str,
        vault_path: PathBuf,
    ) -> Result<Self> {
        let realm = network.join(invite).await?;
        let member_id = network.id();
        Self::setup(realm, vault_path, member_id).await
    }

    /// Common setup: initialize blob store, watcher, and sync-to-disk.
    async fn setup(realm: Realm, vault_path: PathBuf, member_id: MemberId) -> Result<Self> {
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

        // Start watcher (local FS -> vault-index)
        let watcher = VaultWatcher::start(
            vault_path.clone(),
            realm.clone(),
            Arc::clone(&blob_store),
            member_id,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        // Start sync-to-disk (vault-index -> local FS)
        let doc = realm.vault_index().await?;
        let sync = SyncToDisk::start(doc, vault_path.clone(), Arc::clone(&blob_store), &watcher);

        info!(
            vault = %vault_path.display(),
            "Vault started"
        );

        Ok(Self {
            realm,
            vault_path,
            blob_store,
            member_id,
            watcher: Some(watcher),
            sync: Some(sync),
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

                    let rel_path = path
                        .strip_prefix(&self.vault_path)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");

                    self.realm
                        .upsert_file(&rel_path, hash, size, self.member_id)
                        .await?;
                    count += 1;
                }
            }
        }

        info!(count, "Initial vault scan complete");
        Ok(count)
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
