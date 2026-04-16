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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use indras_storage::{BlobStore, ContentRef, StorageError};
use tokio::sync::RwLock;

use crate::braid::PatchFile;

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
}
