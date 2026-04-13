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
    /// Recorded vault directory per realm so `vault_path()` returns
    /// the same name-based path that `ensure_vault` chose.
    paths: RwLock<HashMap<[u8; 32], PathBuf>>,
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
            paths: RwLock::new(HashMap::new()),
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
        self.paths.write().await.insert(rid, vault_path);
        self.name_to_realm.write().await.insert(final_name, rid);
        Ok(())
    }

    /// Start the user's private (home) vault under a name-based dir.
    ///
    /// The home vault lives at `{data_dir}/vaults/<sanitize(self_name)>/`
    /// alongside peer DM vaults, so Obsidian can open the parent
    /// `vaults/` folder as one workspace. Returns the chosen path.
    pub async fn start_private_vault(&self, self_name: &str) -> PathBuf {
        let sanitized = sanitize(self_name).unwrap_or_else(|| "home".to_string());
        // Reserve the name so a peer with the same name can't collide.
        {
            let mut n2r = self.name_to_realm.write().await;
            // Use a sentinel realm id (all zeros) for the home vault.
            n2r.entry(sanitized.clone()).or_insert([0u8; 32]);
        }
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
    /// Returns `None` if the vault hasn't been initialized yet.
    pub async fn vault_path(&self, realm_id: &[u8; 32]) -> Option<PathBuf> {
        self.paths.read().await.get(realm_id).cloned()
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
