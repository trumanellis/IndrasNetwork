//! Sync remote vault changes to the local filesystem.
//!
//! Subscribes to the vault-index document's change stream and writes
//! remote file additions/edits to disk, removes deleted files, and
//! materializes conflict copies.

use crate::relay_sync::RelayBlobSync;
use crate::vault_document::VaultFileDocument;
use crate::vault_file::VaultFile;
use crate::watcher::VaultWatcher;

use dashmap::DashMap;
use indras_network::document::Document;
use indras_storage::{BlobStore, ContentRef};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Suppress duration when writing files from remote changes.
const SUPPRESS_DURATION: Duration = Duration::from_secs(2);

/// Syncs remote vault-index changes to the local filesystem.
pub struct SyncToDisk {
    /// Background task handle.
    handle: JoinHandle<()>,
}

impl SyncToDisk {
    /// Start syncing remote changes to disk.
    ///
    /// Subscribes to the vault-index document and writes remote changes
    /// to `vault_path`, loading blob content from `blob_store`.
    pub fn start(
        doc: Document<VaultFileDocument>,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
        watcher: &VaultWatcher,
        relay: Option<Arc<RelayBlobSync>>,
    ) -> Self {
        let rx = doc.subscribe();
        let suppressed = Arc::clone(&watcher.suppressed);
        let known_hashes = Arc::clone(&watcher.known_hashes);

        let handle = tokio::spawn(Self::sync_loop(
            rx,
            vault_path,
            blob_store,
            suppressed,
            known_hashes,
            relay,
        ));

        info!("SyncToDisk started");
        Self { handle }
    }

    /// Stop the sync task.
    pub fn stop(self) {
        self.handle.abort();
    }

    /// Main sync loop: receive document changes and write to disk.
    #[allow(clippy::too_many_arguments)]
    async fn sync_loop(
        mut rx: broadcast::Receiver<indras_network::document::DocumentChange<VaultFileDocument>>,
        vault_path: PathBuf,
        blob_store: Arc<BlobStore>,
        suppressed: Arc<DashMap<PathBuf, Instant>>,
        known_hashes: Arc<DashMap<String, [u8; 32]>>,
        relay: Option<Arc<RelayBlobSync>>,
    ) {
        // Track last-known state for diffing
        let mut last_files: BTreeMap<String, VaultFile> = BTreeMap::new();

        loop {
            let change = match rx.recv().await {
                Ok(c) => c,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SyncToDisk lagged, will process next change");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };

            // Only process remote changes
            if !change.is_remote {
                // Still update our snapshot so diffs are correct
                last_files = change.new_state.files.clone();
                continue;
            }

            let new_state = &change.new_state;

            // Diff files
            for (path, new_file) in &new_state.files {
                let changed = match last_files.get(path) {
                    Some(old) => old.hash != new_file.hash || old.deleted != new_file.deleted,
                    None => true,
                };
                if !changed {
                    continue;
                }

                let disk_path = vault_path.join(path);

                if new_file.deleted {
                    // Remove from disk
                    suppress_path(&suppressed, &disk_path, SUPPRESS_DURATION);
                    if disk_path.exists() {
                        if let Err(e) = tokio::fs::remove_file(&disk_path).await {
                            warn!(path = %path, error = %e, "Failed to remove deleted file");
                        } else {
                            debug!(path = %path, "Removed deleted file from disk");
                        }
                    }
                } else {
                    // Try inline content first (embedded in CRDT, most reliable).
                    // If inline content is available, store it in the blob store
                    // and use it directly — no relay fallback needed.
                    if let Some(ref inline) = new_file.content {
                        let _ = blob_store.store(inline).await;
                    }
                    let content_ref = ContentRef::new(new_file.hash, new_file.size);
                    let mut data = blob_store.load(&content_ref).await;

                    // If still not found, try relay fallback
                    if data.is_err() {
                        if let Some(ref relay) = relay {
                            debug!(
                                path = %path,
                                hash = %hex::encode(&new_file.hash[..6]),
                                "Blob not local, pulling from relay"
                            );
                            let content_ref = ContentRef::new(new_file.hash, new_file.size);
                            for attempt in 0..10 {
                                if attempt > 0 {
                                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                }
                                let _ = relay.pull_blobs(&blob_store).await;
                                data = blob_store.load(&content_ref).await;
                                if data.is_ok() {
                                    break;
                                }
                            }
                        }
                    }

                    match data {
                        Ok(data) => {
                            // Ensure parent directory exists
                            if let Some(parent) = disk_path.parent() {
                                let _ = tokio::fs::create_dir_all(parent).await;
                            }
                            suppress_path(&suppressed, &disk_path, SUPPRESS_DURATION);
                            if let Err(e) = tokio::fs::write(&disk_path, &data).await {
                                warn!(path = %path, error = %e, "Failed to write file to disk");
                            } else {
                                // Record the hash so the watcher won't re-index
                                // this content with a new timestamp.
                                known_hashes.insert(path.clone(), new_file.hash);
                                let written_len = data.len();
                                debug!(path = %path, size = written_len, "Wrote remote file to disk");
                            }
                        }
                        Err(e) => {
                            warn!(
                                path = %path,
                                hash = %hex::encode(&new_file.hash[..6]),
                                error = %e,
                                "Failed to load blob for remote file (not in local store or relay)"
                            );
                        }
                    }
                }
            }

            // Write conflict copies for new unresolved conflicts
            for conflict in &new_state.conflicts {
                if conflict.resolved {
                    continue;
                }
                let conflict_path = vault_path.join(conflict.conflict_filename());
                if conflict_path.exists() {
                    continue; // Already written
                }

                let content_ref = ContentRef::new(conflict.loser_hash, 0);
                // We don't know the exact size, but BlobStore looks up by hash path
                // Try to load -- if the blob is available, write it
                match blob_store.load(&content_ref).await {
                    Ok(data) => {
                        if let Some(parent) = conflict_path.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        suppress_path(&suppressed, &conflict_path, SUPPRESS_DURATION);
                        if let Err(e) = tokio::fs::write(&conflict_path, &data).await {
                            warn!(path = %conflict_path.display(), error = %e, "Failed to write conflict file");
                        } else {
                            info!(
                                path = %conflict.path,
                                conflict_file = %conflict_path.display(),
                                "Wrote conflict copy"
                            );
                        }
                    }
                    Err(e) => {
                        debug!(
                            path = %conflict.path,
                            error = %e,
                            "Conflict blob not available locally (may arrive later)"
                        );
                    }
                }
            }

            // Update snapshot
            last_files = new_state.files.clone();
        }
    }
}

/// Insert a path into the suppression map with a deadline.
fn suppress_path(suppressed: &DashMap<PathBuf, Instant>, path: &Path, duration: Duration) {
    suppressed.insert(path.to_path_buf(), Instant::now() + duration);
}
