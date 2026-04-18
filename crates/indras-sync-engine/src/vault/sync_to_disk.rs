//! SyncFromDag — materializes braid DAG changes to the local filesystem.
//!
//! Subscribes to the braid DAG's change stream. When a remote peer
//! publishes a new HEAD:
//! - If **trusted**: auto-merge by materializing their manifest to disk
//!   and advancing our local HEAD.
//! - If **untrusted**: log the fork for later UI notification.
//!
//! Own HEAD changes are materialized to disk when detected.

use super::relay_sync::RelayBlobSync;
use super::trust::LocalTrustStore;
use super::vault_document::VaultFileDocument;
use super::vault_file::UserId;
use super::watcher::VaultWatcher;

use crate::braid::changeset::{Changeset, Evidence, PatchFile, PatchManifest};
use crate::braid::dag::BraidDag;

use dashmap::DashMap;
use indras_network::document::Document;
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
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        dag: Document<BraidDag>,
        _local_index: Arc<RwLock<VaultFileDocument>>,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
        watcher: &VaultWatcher,
        relay: Option<Arc<RelayBlobSync>>,
        trust_store: Arc<RwLock<LocalTrustStore>>,
        user_id: UserId,
    ) -> Self {
        let rx = dag.subscribe();
        let suppressed = Arc::clone(&watcher.suppressed);
        let known_hashes = Arc::clone(&watcher.known_hashes);

        let handle = tokio::spawn(Self::sync_loop(
            rx,
            dag,
            vault_path,
            blob_store,
            suppressed,
            known_hashes,
            relay,
            trust_store,
            user_id,
        ));

        info!("SyncFromDag started");
        Self { handle }
    }

    /// Stop the sync task.
    pub fn stop(self) {
        self.handle.abort();
    }

    /// Main sync loop: receive DAG changes and materialize to disk.
    #[allow(clippy::too_many_arguments)]
    async fn sync_loop(
        mut rx: tokio::sync::broadcast::Receiver<
            indras_network::document::DocumentChange<BraidDag>,
        >,
        dag: Document<BraidDag>,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
        suppressed: Arc<DashMap<PathBuf, Instant>>,
        known_hashes: Arc<DashMap<String, [u8; 32]>>,
        relay: Option<Arc<RelayBlobSync>>,
        trust_store: Arc<RwLock<LocalTrustStore>>,
        user_id: UserId,
    ) {
        // Track last-known peer heads to detect changes.
        let mut last_peer_heads: HashMap<UserId, crate::braid::changeset::ChangeId> =
            HashMap::new();

        loop {
            let change = match rx.recv().await {
                Ok(c) => c,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SyncFromDag lagged, will process next change");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };

            // Only react to remote changes.
            if !change.is_remote {
                // Update our tracking of peer heads from local changes.
                for (uid, ps) in change.new_state.all_peer_heads() {
                    last_peer_heads.insert(*uid, ps.head);
                }
                continue;
            }

            let new_state = &change.new_state;

            // Check each peer's HEAD for changes.
            for (peer_id, peer_state) in new_state.all_peer_heads() {
                if *peer_id == user_id {
                    continue; // Skip our own HEAD.
                }

                let old_head = last_peer_heads.get(peer_id).copied();
                if old_head == Some(peer_state.head) {
                    continue; // No change for this peer.
                }

                let peer_short = hex::encode(&peer_id[..4]);
                let trusted = trust_store.read().await.is_trusted(peer_id);

                if trusted {
                    info!(
                        peer = %peer_short,
                        head = %peer_state.head,
                        "Trusted peer advanced HEAD — auto-materializing"
                    );

                    // Materialize their manifest to disk.
                    if let Err(e) = materialize_manifest(
                        &peer_state.head_manifest,
                        &vault_path,
                        &blob_store,
                        relay.as_deref(),
                        &suppressed,
                        &known_hashes,
                    )
                    .await
                    {
                        warn!(
                            peer = %peer_short,
                            error = %e,
                            "Failed to materialize trusted peer's manifest"
                        );
                    }

                    // Create a merge changeset advancing our HEAD.
                    let my_head = new_state.peer_head(&user_id);
                    let mut parents = vec![peer_state.head];
                    if let Some(mh) = my_head {
                        parents.push(mh.head);
                    }

                    let evidence = Evidence::human(
                        user_id,
                        Some("auto-merge from trusted peer".to_string()),
                    );
                    let ts = chrono::Utc::now().timestamp_millis();
                    let manifest = peer_state.head_manifest.clone();
                    let changeset = Changeset::new(
                        user_id,
                        parents,
                        format!("auto-merge from {peer_short}"),
                        manifest.clone(),
                        evidence,
                        ts,
                    );
                    let change_id = changeset.id;

                    if let Err(e) = dag
                        .update(|d| {
                            d.insert(changeset);
                            d.update_peer_head(user_id, change_id, manifest);
                        })
                        .await
                    {
                        warn!(error = %e, "Failed to insert auto-merge changeset");
                    }
                } else {
                    info!(
                        peer = %peer_short,
                        head = %peer_state.head,
                        "Untrusted peer advanced HEAD — fork available for review"
                    );
                }
            }

            // Update tracking.
            for (uid, ps) in new_state.all_peer_heads() {
                last_peer_heads.insert(*uid, ps.head);
            }
        }
    }
}

/// Materialize a [`PatchManifest`] to disk at `vault_path`.
///
/// For each file in the manifest, loads the blob from the store (with
/// relay fallback) and writes it to the vault directory. Suppresses the
/// watcher for each written path to prevent echo.
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
