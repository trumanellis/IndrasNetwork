//! Device-local working-tree state for an agent folder.
//!
//! A [`LocalWorkspaceIndex`] tracks the content hash + size of every file
//! an agent is editing in a bound worktree. It is **never synced via
//! CRDT** — the entire point is to keep half-written, broken, or
//! intermediate work invisible to teammates until the agent explicitly
//! commits via `try_land`.
//!
//! The index is the sole source of truth for `PatchManifest` assembly in
//! the commit path. Blobs are pushed into the shared [`BlobStore`] so
//! peers that later check out a changeset can pull the content on demand.
//!
//! See `project_braid_local_working_tree.md` in auto-memory for the
//! three-state model (working tree / committed / checked out) and why
//! CRDT-synced in-flight state was explicitly ruled out.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fs4::fs_std::FileExt;
use indras_storage::{BlobStore, ContentRef, StorageError};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use crate::braid::PatchFile;

/// Filename used for the advisory single-writer lock inside a bound folder.
const LOCK_FILENAME: &str = ".syncengine-lock";

/// One file's entry in the working-tree index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceEntry {
    /// BLAKE3 hash of the current on-disk content.
    pub hash: [u8; 32],
    /// File size in bytes.
    pub size: u64,
}

impl From<ContentRef> for WorkspaceEntry {
    fn from(cr: ContentRef) -> Self {
        Self {
            hash: cr.hash,
            size: cr.size,
        }
    }
}

/// Device-local index of an agent's working tree. Never synced.
///
/// Keyed by relative path from `root`. Populated by the fs-watcher as the
/// agent edits files; consumed by `try_land` when building a changeset.
pub struct LocalWorkspaceIndex {
    root: PathBuf,
    files: RwLock<HashMap<String, WorkspaceEntry>>,
    blob_store: Arc<BlobStore>,
}

impl LocalWorkspaceIndex {
    /// Build an empty index rooted at `root`, pushing blobs into `blob_store`.
    pub fn new(root: PathBuf, blob_store: Arc<BlobStore>) -> Self {
        Self {
            root,
            files: RwLock::new(HashMap::new()),
            blob_store,
        }
    }

    /// Absolute root of this workspace on disk.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Hash `bytes`, push them to the blob store, and record the entry
    /// under `rel_path`. Returns the computed [`ContentRef`]. Idempotent
    /// at the blob layer — duplicate writes are de-duplicated by hash.
    pub async fn ingest_bytes(
        &self,
        rel_path: impl Into<String>,
        bytes: &[u8],
    ) -> Result<ContentRef, StorageError> {
        let content_ref = self.blob_store.store(bytes).await?;
        self.files.write().await.insert(
            rel_path.into(),
            WorkspaceEntry {
                hash: content_ref.hash,
                size: content_ref.size,
            },
        );
        Ok(content_ref)
    }

    /// Drop the entry for `rel_path`, returning the previous value if any.
    /// Does not delete the blob — content-addressed blobs are referenced
    /// by past changesets and must persist.
    pub async fn forget(&self, rel_path: &str) -> Option<WorkspaceEntry> {
        self.files.write().await.remove(rel_path)
    }

    /// Read the current entry for `rel_path`.
    pub async fn get(&self, rel_path: &str) -> Option<WorkspaceEntry> {
        self.files.read().await.get(rel_path).copied()
    }

    /// Number of tracked paths.
    pub async fn len(&self) -> usize {
        self.files.read().await.len()
    }

    /// Whether no paths are tracked.
    pub async fn is_empty(&self) -> bool {
        self.files.read().await.is_empty()
    }

    /// Snapshot the index to a list of [`PatchFile`]s for a requested set
    /// of paths, sorted by path for deterministic hashing downstream.
    /// Paths absent from the index are silently skipped.
    pub async fn snapshot_patch(&self, paths: &[String]) -> Vec<PatchFile> {
        let files = self.files.read().await;
        let mut out: Vec<PatchFile> = paths
            .iter()
            .filter_map(|p| {
                files.get(p).map(|e| PatchFile {
                    path: p.clone(),
                    hash: e.hash,
                    size: e.size,
                })
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Snapshot every tracked path into [`PatchFile`]s.
    pub async fn snapshot_all(&self) -> Vec<PatchFile> {
        let files = self.files.read().await;
        let mut out: Vec<PatchFile> = files
            .iter()
            .map(|(p, e)| PatchFile {
                path: p.clone(),
                hash: e.hash,
                size: e.size,
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Walk `root` recursively, ingesting every regular file's current
    /// bytes into the index. Returns the count of files ingested.
    ///
    /// Call once on startup (or when a folder is newly bound) before
    /// spawning the fs-watcher, so the index reflects the on-disk state
    /// an agent may have pre-existing.
    pub async fn initial_scan(&self) -> std::io::Result<usize> {
        let mut count = 0;
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = match tokio::fs::read_dir(&dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let ft = entry.file_type().await?;
                if ft.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                let rel = match path.strip_prefix(&self.root) {
                    Ok(r) => r.to_string_lossy().replace('\\', "/"),
                    Err(_) => continue,
                };
                match tokio::fs::read(&path).await {
                    Ok(bytes) => {
                        if self.ingest_bytes(rel, &bytes).await.is_ok() {
                            count += 1;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(path = %path.display(), error = %e, "skip during initial_scan");
                    }
                }
            }
        }
        Ok(count)
    }
}

/// Debounce window between receiving the first fs event and draining
/// the queue before re-ingesting files.
const WATCH_DEBOUNCE_MS: u64 = 100;

/// fs-watcher that keeps a [`LocalWorkspaceIndex`] in sync with an
/// on-disk agent folder. The watcher drives the index only — no CRDT
/// sync, no blob broadcast beyond the local blob store.
///
/// Drop the returned `WorkspaceWatcher` to stop watching; the
/// background task aborts via `JoinHandle`.
pub struct WorkspaceWatcher {
    /// Held so the watcher keeps running; dropping it tears down the OS
    /// listener.
    _watcher: RecommendedWatcher,
    handle: JoinHandle<()>,
}

impl WorkspaceWatcher {
    /// Start watching `index.root()` recursively. Events modify `index`.
    ///
    /// Caller is responsible for a prior `index.initial_scan().await` if
    /// the folder may already contain files.
    pub fn start(index: Arc<LocalWorkspaceIndex>) -> Result<Self, notify::Error> {
        // Canonicalize so events (e.g. FSEvents' `/private/tmp/...`)
        // strip-prefix cleanly in the event loop.
        let root = std::fs::canonicalize(index.root())
            .unwrap_or_else(|_| index.root().to_path_buf());

        let (tx, rx) = mpsc::channel::<Event>(512);
        let tx_clone = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx_clone.blocking_send(event);
            }
        })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;

        let handle = tokio::spawn(event_loop(rx, root, Arc::clone(&index)));
        Ok(Self {
            _watcher: watcher,
            handle,
        })
    }

    /// Abort the background task immediately. Equivalent to dropping.
    pub fn abort(self) {
        self.handle.abort();
    }
}

async fn event_loop(
    mut rx: mpsc::Receiver<Event>,
    root: PathBuf,
    index: Arc<LocalWorkspaceIndex>,
) {
    while let Some(event) = rx.recv().await {
        // Debounce: brief pause, then drain additional events that
        // piled up during the pause so they all process in one batch.
        tokio::time::sleep(Duration::from_millis(WATCH_DEBOUNCE_MS)).await;
        let mut events = vec![event];
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        for ev in events {
            handle_event(&ev, &root, &index).await;
        }
    }
}

/// Advisory single-writer lock on a bound folder.
///
/// Prevents two syncengine processes from both mirroring the same folder
/// into a [`LocalWorkspaceIndex`] (which would race on the blob store and
/// produce duplicate ingests). The lock is an OS-level advisory exclusive
/// lock on `{folder}/.syncengine-lock` via [`fs4`], so it auto-releases
/// if the holding process crashes — no stale-lock cleanup needed.
///
/// Caller must keep the [`FolderLock`] alive for as long as the watcher
/// is running. Dropping it releases the OS lock (automatic on file drop)
/// and best-effort removes the lockfile.
#[derive(Debug)]
pub struct FolderLock {
    _file: File,
    path: PathBuf,
}

impl FolderLock {
    /// Try to acquire the lock on `folder`. Creates the folder and the
    /// lockfile if missing. Returns `Err(WouldBlock)` if another process
    /// holds the lock.
    pub fn acquire(folder: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(folder)?;
        let path = folder.join(LOCK_FILENAME);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        file.try_lock_exclusive().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!(
                    "another syncengine already owns {}: {e}",
                    folder.display()
                ),
            )
        })?;
        Ok(Self { _file: file, path })
    }

    /// Path to the lockfile on disk. Exposed for diagnostics / tests.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FolderLock {
    fn drop(&mut self) {
        // File drop releases the OS advisory lock. Best-effort remove the
        // lockfile so the directory stays tidy; harmless if another
        // process grabbed it in between.
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn handle_event(event: &Event, root: &Path, index: &LocalWorkspaceIndex) {
    for path in &event.paths {
        let rel = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if rel.is_empty() {
            continue;
        }
        match event.kind {
            EventKind::Remove(_) => {
                index.forget(&rel).await;
            }
            _ => match tokio::fs::read(path).await {
                Ok(bytes) => {
                    if let Err(e) = index.ingest_bytes(rel.clone(), &bytes).await {
                        tracing::debug!(path = %rel, error = %e, "ingest failed in watcher");
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    index.forget(&rel).await;
                }
                Err(e) => {
                    tracing::debug!(path = %rel, error = %e, "read failed in watcher");
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_storage::BlobStoreConfig;
    use tempfile::TempDir;

    async fn tmp_blob_store() -> (TempDir, Arc<BlobStore>) {
        let tmp = TempDir::new().unwrap();
        let cfg = BlobStoreConfig {
            base_dir: tmp.path().join("blobs"),
            ..Default::default()
        };
        let store = Arc::new(BlobStore::new(cfg).await.unwrap());
        (tmp, store)
    }

    #[tokio::test]
    async fn ingest_then_get_round_trips() {
        let (_tmp, blob) = tmp_blob_store().await;
        let idx = LocalWorkspaceIndex::new(PathBuf::from("/fake"), blob);

        let bytes = b"hello working tree";
        let cref = idx.ingest_bytes("notes.md", bytes).await.unwrap();
        let entry = idx.get("notes.md").await.unwrap();
        assert_eq!(entry.hash, cref.hash);
        assert_eq!(entry.size, bytes.len() as u64);
    }

    #[tokio::test]
    async fn forget_drops_entry() {
        let (_tmp, blob) = tmp_blob_store().await;
        let idx = LocalWorkspaceIndex::new(PathBuf::from("/fake"), blob);

        idx.ingest_bytes("a.md", b"alpha").await.unwrap();
        assert_eq!(idx.len().await, 1);
        let prev = idx.forget("a.md").await.unwrap();
        assert_eq!(prev.size, 5);
        assert!(idx.is_empty().await);
    }

    #[tokio::test]
    async fn snapshot_patch_returns_requested_paths_sorted() {
        let (_tmp, blob) = tmp_blob_store().await;
        let idx = LocalWorkspaceIndex::new(PathBuf::from("/fake"), blob);

        idx.ingest_bytes("b.md", b"beta").await.unwrap();
        idx.ingest_bytes("a.md", b"alpha").await.unwrap();
        idx.ingest_bytes("c.md", b"gamma").await.unwrap();

        let snap = idx
            .snapshot_patch(&["b.md".to_string(), "a.md".to_string()])
            .await;
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].path, "a.md");
        assert_eq!(snap[1].path, "b.md");
    }

    #[tokio::test]
    async fn ingest_is_idempotent_for_same_content() {
        let (_tmp, blob) = tmp_blob_store().await;
        let idx = LocalWorkspaceIndex::new(PathBuf::from("/fake"), blob);

        let cref1 = idx.ingest_bytes("x.md", b"same").await.unwrap();
        let cref2 = idx.ingest_bytes("x.md", b"same").await.unwrap();
        assert_eq!(cref1.hash, cref2.hash);
        assert_eq!(idx.len().await, 1);
    }

    #[tokio::test]
    async fn snapshot_all_is_sorted() {
        let (_tmp, blob) = tmp_blob_store().await;
        let idx = LocalWorkspaceIndex::new(PathBuf::from("/fake"), blob);

        idx.ingest_bytes("z.md", b"z").await.unwrap();
        idx.ingest_bytes("a.md", b"a").await.unwrap();
        idx.ingest_bytes("m.md", b"m").await.unwrap();

        let all = idx.snapshot_all().await;
        let paths: Vec<&str> = all.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "m.md", "z.md"]);
    }

    #[tokio::test]
    async fn initial_scan_ingests_existing_files() {
        let (_tmp_blob, blob) = tmp_blob_store().await;
        let root_tmp = TempDir::new().unwrap();
        let root = root_tmp.path().to_path_buf();
        tokio::fs::create_dir_all(root.join("sub")).await.unwrap();
        tokio::fs::write(root.join("top.md"), b"top contents")
            .await
            .unwrap();
        tokio::fs::write(root.join("sub/nested.md"), b"nested")
            .await
            .unwrap();

        let idx = LocalWorkspaceIndex::new(root, blob);
        let count = idx.initial_scan().await.unwrap();
        assert_eq!(count, 2);
        assert_eq!(idx.get("top.md").await.unwrap().size, 12);
        assert_eq!(idx.get("sub/nested.md").await.unwrap().size, 6);
    }

    #[tokio::test]
    async fn watcher_picks_up_new_file() {
        let (_tmp_blob, blob) = tmp_blob_store().await;
        let root_tmp = TempDir::new().unwrap();
        let root = root_tmp.path().to_path_buf();

        let idx = Arc::new(LocalWorkspaceIndex::new(root.clone(), blob));
        let _watcher = WorkspaceWatcher::start(Arc::clone(&idx)).expect("watcher start");

        // Allow the watcher's OS listener to settle before the write.
        tokio::time::sleep(Duration::from_millis(150)).await;

        tokio::fs::write(root.join("new.md"), b"fresh bytes")
            .await
            .unwrap();

        // Poll for up to 5s; fs events on macOS can take a moment.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut seen = false;
        while tokio::time::Instant::now() < deadline {
            if let Some(entry) = idx.get("new.md").await {
                assert_eq!(entry.size, 11);
                seen = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(seen, "watcher must index new.md within 5s");
    }

    #[tokio::test]
    async fn folder_lock_prevents_second_acquire() {
        let tmp = TempDir::new().unwrap();
        let first = FolderLock::acquire(tmp.path()).expect("first acquire");
        let second = FolderLock::acquire(tmp.path());
        assert!(
            second.is_err(),
            "second lock on same folder must fail while first is held"
        );
        assert_eq!(
            second.unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "error kind should surface as WouldBlock for easy caller matching"
        );
        drop(first);
    }

    #[tokio::test]
    async fn folder_lock_releases_on_drop() {
        let tmp = TempDir::new().unwrap();
        {
            let _lock = FolderLock::acquire(tmp.path()).expect("first acquire");
        }
        let reacquired = FolderLock::acquire(tmp.path());
        assert!(
            reacquired.is_ok(),
            "lock must release when FolderLock is dropped"
        );
    }

    #[tokio::test]
    async fn folder_lock_creates_folder_if_missing() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("not-yet-created");
        let lock = FolderLock::acquire(&nested).expect("acquire with missing folder");
        assert!(nested.exists(), "acquire should create the folder");
        assert!(lock.path().exists(), "lockfile should exist");
    }

    #[tokio::test]
    async fn watcher_picks_up_deletion() {
        let (_tmp_blob, blob) = tmp_blob_store().await;
        let root_tmp = TempDir::new().unwrap();
        let root = root_tmp.path().to_path_buf();
        tokio::fs::write(root.join("doomed.md"), b"goodbye")
            .await
            .unwrap();

        let idx = Arc::new(LocalWorkspaceIndex::new(root.clone(), blob));
        idx.initial_scan().await.unwrap();
        assert!(idx.get("doomed.md").await.is_some());

        let _watcher = WorkspaceWatcher::start(Arc::clone(&idx)).expect("watcher start");
        tokio::time::sleep(Duration::from_millis(150)).await;

        tokio::fs::remove_file(root.join("doomed.md")).await.unwrap();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut gone = false;
        while tokio::time::Instant::now() < deadline {
            if idx.get("doomed.md").await.is_none() {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "watcher must forget doomed.md within 5s");
    }
}
