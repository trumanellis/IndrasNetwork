//! Per-realm vault sync manager.
//!
//! Owns a `Vault` instance per shared realm, wiring up `VaultWatcher`,
//! `SyncToDisk`, and `RelayBlobSync` so files automatically propagate
//! between realm members.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use indras_network::{IndrasNetwork, Realm};
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::vault::vault_file::VaultFile;
use indras_sync_engine::vault::Vault;
use tokio::sync::RwLock;
use tracing::info;

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
            data_dir,
            blob_store,
        })
    }

    /// Ensure vault sync is running for a realm.
    ///
    /// Idempotent — returns immediately if the vault already exists.
    /// Creates the vault directory, attaches the sync pipeline, and
    /// runs an initial scan of any pre-existing files.
    pub async fn ensure_vault(
        &self,
        network: &IndrasNetwork,
        realm: &Realm,
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

        let hex_id: String = rid.iter().take(8).map(|b| format!("{b:02x}")).collect();
        let vault_path = self.data_dir.join("vaults").join(&hex_id);

        let vault = Vault::attach(
                network,
                realm.clone(),
                vault_path,
                Arc::clone(&self.blob_store),
            )
            .await
            .map_err(|e| format!("vault attach: {e}"))?;

        let count = vault
            .initial_scan()
            .await
            .map_err(|e| format!("initial scan: {e}"))?;

        info!(realm = %hex_id, files = count, "Vault sync started");
        vaults.insert(rid, vault);
        Ok(())
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
    /// Returns `None` if the vault hasn't been initialized yet.
    pub async fn vault_path(&self, realm_id: &[u8; 32]) -> Option<PathBuf> {
        let vaults = self.vaults.read().await;
        vaults.get(realm_id).map(|v| v.path().to_path_buf())
    }
}
