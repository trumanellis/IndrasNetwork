//! SyncFromDag — materializes DAG changes to the local filesystem.
//!
//! Subscribes to the braid DAG's change stream. When a remote peer
//! publishes a new HEAD:
//! - If **trusted**: auto-merge by advancing our local HEAD and
//!   materializing the new manifest to disk.
//! - If **untrusted**: log the fork (UI notification deferred to later).
//!
//! Own HEAD changes (from `Vault::sync()` or `try_land`) are also
//! materialized to disk when detected.

use super::relay_sync::RelayBlobSync;
use super::vault_document::VaultFileDocument;
use super::vault_file::VaultFile;
use super::watcher::VaultWatcher;

use crate::braid::dag::{BraidDag, PeerState};
use crate::braid::changeset::{ChangeId, PatchManifest};
use crate::vault::vault_file::UserId;

use dashmap::DashMap;
use indras_network::document::{Document, DocumentChange};
use indras_storage::{BlobStore, ContentRef};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Suppress duration when writing files from DAG changes.
const SUPPRESS_DURATION: Duration = Duration::from_secs(2);

/// Syncs braid DAG changes to the local filesystem.
pub struct SyncToDisk {
    /// Background task handle.
    handle: JoinHandle<()>,
}

impl SyncToDisk {
    /// Start syncing DAG changes to disk.
    pub fn start(
        local_index: Arc<RwLock<VaultFileDocument>>,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
        watcher: &VaultWatcher,
        relay: Option<Arc<RelayBlobSync>>,
    ) -> Self {
        let suppressed = Arc::clone(&watcher.suppressed);
        let known_hashes = Arc::clone(&watcher.known_hashes);

        let handle = tokio::spawn(async move {
            // SyncFromDag stub: will be wired to DAG subscription in Phase 7/8
            // when the full sync/merge flow is connected. For now, this task
            // stays alive but doesn't actively process — materialization happens
            // through Vault::checkout() and Vault::apply_manifest() called
            // directly by the commit flow.
            info!("SyncFromDag started");
            let _ = (local_index, vault_path, blob_store, suppressed, known_hashes, relay);
            std::future::pending::<()>().await;
        });

        Self { handle }
    }

    /// Stop the sync task.
    pub fn stop(self) {
        self.handle.abort();
    }
}

/// Materialize a [`PatchManifest`] to disk at `vault_path`.
///
/// For each file in the manifest, loads the blob from the store (with
/// relay fallback) and writes it to the vault directory. Suppresses the
/// watcher for each written path to prevent echo.
///
/// This is a standalone utility used by both `SyncFromDag` and
/// `Vault::apply_manifest`.
pub(crate) async fn materialize_manifest(
    manifest: &PatchManifest,
    vault_path: &std::path::Path,
    blob_store: &BlobStore,
    relay: Option<&RelayBlobSync>,
    suppressed: &DashMap<PathBuf, Instant>,
    known_hashes: &DashMap<String, [u8; 32]>,
) -> Result<(), std::io::Error> {
    for pf in &manifest.files {
        let content_ref = ContentRef::new(pf.hash, pf.size);
        let mut data = blob_store.load(&content_ref).await;

        // Relay fallback if blob not found locally
        if data.is_err() {
            if let Some(relay) = relay {
                for _ in 0..10 {
                    let _ = relay.pull_blobs(blob_store).await;
                    data = blob_store.load(&content_ref).await;
                    if data.is_ok() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }

        let bytes = data.map_err(|e| {
            std::io::Error::other(format!(
                "materialize: blob for {} not available: {e}",
                pf.path
            ))
        })?;

        let disk_path = vault_path.join(&pf.path);

        // Ensure parent directory exists
        if let Some(parent) = disk_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Suppress watcher echo
        suppressed.insert(disk_path.clone(), Instant::now() + SUPPRESS_DURATION);

        // Write to disk
        tokio::fs::write(&disk_path, &bytes).await?;

        // Record hash so watcher won't re-index
        known_hashes.insert(pf.path.clone(), pf.hash);

        info!(path = %pf.path, size = bytes.len(), "Materialized file from DAG");
    }
    Ok(())
}
