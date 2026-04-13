//! File system watcher for vault directories.
//!
//! Monitors a vault directory for local changes (creates, edits, deletes),
//! hashes changed files with BLAKE3, stores blobs, and updates the
//! vault-index document directly via a cached `Document<VaultFileDocument>`.

use super::relay_sync::RelayBlobSync;
use super::vault_document::VaultFileDocument;
use super::vault_file::{UserId, VaultFile};

use dashmap::DashMap;
use indras_network::document::Document;
use indras_storage::BlobStore;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Debounce window for FS events.
const DEBOUNCE_MS: u64 = 100;

/// File system watcher that feeds local changes into the vault-index.
pub struct VaultWatcher {
    /// The watcher handle (kept alive).
    _watcher: RecommendedWatcher,
    /// Background task processing events.
    handle: JoinHandle<()>,
    /// Paths currently suppressed (to prevent echo from sync-to-disk writes).
    pub(crate) suppressed: Arc<DashMap<PathBuf, Instant>>,
    /// Last-known content hash per relative path. The watcher skips re-indexing
    /// when the disk hash matches, preventing stale re-upserts that create
    /// spurious conflicts after CRDT merges update the index ahead of the disk.
    pub(crate) known_hashes: Arc<DashMap<String, [u8; 32]>>,
}

impl VaultWatcher {
    /// Start watching `vault_path` for changes, updating the vault-index document directly.
    ///
    /// `vault_path` is canonicalized on start so that events emitted by the OS
    /// (which on macOS come back as canonical paths like `/private/tmp/...` even
    /// when the watch was registered on the `/tmp/...` symlink) strip-prefix
    /// cleanly in the event loop.
    pub fn start(
        vault_path: PathBuf,
        doc: Document<VaultFileDocument>,
        blob_store: Arc<BlobStore>,
        user_id: UserId,
        relay: Option<Arc<RelayBlobSync>>,
    ) -> Result<Self, notify::Error> {
        // Resolve symlinks so strip_prefix in the event loop matches the
        // canonicalized paths that FSEvents/inotify deliver.
        let vault_path = std::fs::canonicalize(&vault_path).unwrap_or(vault_path);

        let (tx, rx) = mpsc::channel::<Event>(512);
        let suppressed: Arc<DashMap<PathBuf, Instant>> = Arc::new(DashMap::new());
        let known_hashes: Arc<DashMap<String, [u8; 32]>> = Arc::new(DashMap::new());

        let tx_clone = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx_clone.blocking_send(event);
            }
        })?;

        watcher.watch(&vault_path, RecursiveMode::Recursive)?;

        let suppressed_clone = Arc::clone(&suppressed);
        let known_hashes_clone = Arc::clone(&known_hashes);
        let handle = tokio::spawn(Self::event_loop(
            rx,
            vault_path,
            doc,
            blob_store,
            user_id,
            suppressed_clone,
            known_hashes_clone,
            relay,
        ));

        info!("VaultWatcher started");
        Ok(Self {
            _watcher: watcher,
            handle,
            suppressed,
            known_hashes,
        })
    }

    /// Suppress watcher events for `path` for `duration`.
    ///
    /// Used by sync-to-disk to prevent echo when writing remote changes.
    pub fn suppress(&self, path: &Path, duration: Duration) {
        self.suppressed
            .insert(path.to_path_buf(), Instant::now() + duration);
    }

    /// Stop the watcher.
    pub fn stop(self) {
        self.handle.abort();
    }

    /// Check if a path is currently suppressed.
    fn is_suppressed(suppressed: &DashMap<PathBuf, Instant>, path: &Path) -> bool {
        if let Some(entry) = suppressed.get(path) {
            if Instant::now() < *entry.value() {
                return true;
            }
            // Expired — remove
            drop(entry);
            suppressed.remove(path);
        }
        false
    }

    /// Record that a file was written with the given hash (by `write_file_content`).
    /// Prevents the watcher from re-indexing the same content with a new timestamp.
    pub fn record_hash(&self, rel_path: &str, hash: [u8; 32]) {
        self.known_hashes.insert(rel_path.to_string(), hash);
    }

    /// Event processing loop with debounce.
    async fn event_loop(
        mut rx: mpsc::Receiver<Event>,
        vault_path: PathBuf,
        doc: Document<VaultFileDocument>,
        blob_store: Arc<BlobStore>,
        user_id: UserId,
        suppressed: Arc<DashMap<PathBuf, Instant>>,
        known_hashes: Arc<DashMap<String, [u8; 32]>>,
        relay: Option<Arc<RelayBlobSync>>,
    ) {
        // Debounce: collect events then process after quiet period
        let mut pending: std::collections::HashMap<PathBuf, EventKind> =
            std::collections::HashMap::new();

        loop {
            // Wait for first event
            let event = match rx.recv().await {
                Some(e) => e,
                None => break,
            };

            // Accumulate this event
            for path in &event.paths {
                pending.insert(path.clone(), event.kind);
            }

            // Drain further events within debounce window
            let deadline = tokio::time::Instant::now()
                + tokio::time::Duration::from_millis(DEBOUNCE_MS);
            loop {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Some(e)) => {
                        for path in &e.paths {
                            pending.insert(path.clone(), e.kind);
                        }
                    }
                    _ => break, // Timeout or channel closed
                }
            }

            // Process accumulated events
            let batch: Vec<_> = pending.drain().collect();
            for (path, kind) in batch {
                if should_ignore(&path, &vault_path) {
                    continue;
                }
                if Self::is_suppressed(&suppressed, &path) {
                    debug!(path = %path.display(), "Suppressed watcher event");
                    continue;
                }

                let rel_path = match path.strip_prefix(&vault_path) {
                    Ok(r) => r.to_string_lossy().replace('\\', "/"),
                    Err(_) => continue,
                };

                match kind {
                    EventKind::Remove(_) => {
                        debug!(path = %rel_path, "File removed");
                        let rp = rel_path.clone();
                        if let Err(e) = doc.update(|d| d.remove(&rp, user_id)).await {
                            warn!(path = %rel_path, error = %e, "Failed to delete file from vault index");
                        }
                    }
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        // Read file, hash, store blob, upsert
                        let data = match tokio::fs::read(&path).await {
                            Ok(d) => d,
                            Err(e) => {
                                // File may have been deleted between event and read
                                debug!(path = %rel_path, error = %e, "Failed to read file");
                                continue;
                            }
                        };
                        let hash = *blake3::hash(&data).as_bytes();
                        let size = data.len() as u64;

                        // Skip if hash matches the last-known hash for this path.
                        // This prevents re-indexing stale content with a new timestamp
                        // when a CRDT merge has updated the index but SyncToDisk hasn't
                        // yet written the winner content to disk.
                        if let Some(known) = known_hashes.get(&rel_path) {
                            if *known == hash {
                                debug!(path = %rel_path, "File hash unchanged from last index, skipping");
                                continue;
                            }
                        }

                        // Store in blob store
                        if let Err(e) = blob_store.store(&data).await {
                            warn!(path = %rel_path, error = %e, "Failed to store blob");
                            continue;
                        }

                        // Push to relay for remote peers
                        if let Some(ref relay) = relay {
                            let _ = relay.push_blob(&hash, &data).await;
                        }

                        let file = VaultFile::with_content(&rel_path, hash, size, user_id, data.clone());
                        if let Err(e) = doc.update(|d| d.upsert(file)).await {
                            warn!(path = %rel_path, error = %e, "Failed to upsert file in vault index");
                        } else {
                            known_hashes.insert(rel_path.clone(), hash);
                            info!(path = %rel_path, size, hash = %hex::encode(&hash[..6]), "Vault file indexed (local write)");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Check whether a path should be ignored by the watcher.
pub(crate) fn should_ignore(path: &Path, vault_root: &Path) -> bool {
    let rel = match path.strip_prefix(vault_root) {
        Ok(r) => r,
        Err(_) => return true,
    };

    let rel_str = rel.to_string_lossy();

    // .git directory
    if rel_str.starts_with(".git/") || rel_str == ".git" {
        return true;
    }

    // .trash directory
    if rel_str.starts_with(".trash/") || rel_str == ".trash" {
        return true;
    }

    // .indras directory (our own metadata)
    if rel_str.starts_with(".indras/") || rel_str == ".indras" {
        return true;
    }

    // Obsidian transient workspace files
    if rel_str == ".obsidian/workspace.json" || rel_str == ".obsidian/workspace-mobile.json" {
        return true;
    }

    // Conflict files generated by us
    if rel_str.contains(".conflict-") {
        return true;
    }

    // Hidden files (but allow .obsidian/ directory contents through)
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') && !rel_str.starts_with(".obsidian/") {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from("/vault")
    }

    #[test]
    fn ignore_git() {
        assert!(should_ignore(&root().join(".git/HEAD"), &root()));
        assert!(should_ignore(&root().join(".git"), &root()));
    }

    #[test]
    fn ignore_trash() {
        assert!(should_ignore(&root().join(".trash/note.md"), &root()));
    }

    #[test]
    fn ignore_indras_dir() {
        assert!(should_ignore(
            &root().join(".indras/blobs/ab/cd/hash"),
            &root()
        ));
    }

    #[test]
    fn ignore_workspace_json() {
        assert!(should_ignore(
            &root().join(".obsidian/workspace.json"),
            &root()
        ));
        assert!(should_ignore(
            &root().join(".obsidian/workspace-mobile.json"),
            &root()
        ));
    }

    #[test]
    fn ignore_conflict_files() {
        assert!(should_ignore(
            &root().join("notes/daily.conflict-abcdef.md"),
            &root()
        ));
    }

    #[test]
    fn allow_obsidian_config() {
        assert!(!should_ignore(
            &root().join(".obsidian/appearance.json"),
            &root()
        ));
    }

    #[test]
    fn ignore_hidden_files() {
        assert!(should_ignore(&root().join(".DS_Store"), &root()));
        assert!(should_ignore(&root().join("sub/.hidden"), &root()));
    }

    #[test]
    fn allow_normal_files() {
        assert!(!should_ignore(&root().join("notes/daily.md"), &root()));
        assert!(!should_ignore(&root().join("README.md"), &root()));
    }
}
