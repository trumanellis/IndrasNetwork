//! Directory snapshot: walk a directory tree and produce a [`PatchManifest`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use indras_storage::BlobStore;

use crate::braid::changeset::{PatchFile, PatchManifest};

/// Top-level dotfile/directory names to skip when snapshotting.
///
/// These are matched against the **first path component** relative to the
/// snapshot root, so `.git` at the top level is skipped but a nested
/// `.git` directory is not (unlikely in practice but correct behaviour).
const DEFAULT_IGNORES: &[&str] = &[".git", ".claude", ".svn", ".hg", ".DS_Store"];

/// Walk `dir` recursively and build a [`PatchManifest`] from its contents.
///
/// For each regular file encountered:
/// 1. The raw bytes are read and hashed with BLAKE3.
/// 2. The blob is stored in `blob_store` (deduplication is handled there).
/// 3. A [`PatchFile`] entry is pushed with the vault-relative path
///    (forward slashes), the BLAKE3 hash, and the byte size.
///
/// Top-level entries whose name appears in [`DEFAULT_IGNORES`] are skipped.
/// The returned manifest is sorted by path for determinism, matching the
/// ordering guarantee of [`PatchManifest::new`].
///
/// # Errors
///
/// Returns an `std::io::Error`-based error if any directory read or file
/// read fails, or if storing a blob fails.
pub async fn snapshot_dir(
    dir: &Path,
    blob_store: &Arc<BlobStore>,
) -> Result<PatchManifest, std::io::Error> {
    let mut files: Vec<PatchFile> = Vec::new();

    // Iterative DFS using a work stack of (path, depth).
    // depth == 0 means we're processing entries directly inside `dir`.
    let mut stack: Vec<(PathBuf, u32)> = vec![(dir.to_path_buf(), 0)];

    while let Some((current, depth)) = stack.pop() {
        let mut read_dir = tokio::fs::read_dir(&current).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Skip top-level ignored names.
            if depth == 0 && DEFAULT_IGNORES.contains(&name.as_ref()) {
                continue;
            }

            let ft = entry.file_type().await?;

            if ft.is_dir() {
                stack.push((path, depth + 1));
            } else if ft.is_file() {
                let data = tokio::fs::read(&path).await?;
                let hash = *blake3::hash(&data).as_bytes();
                let size = data.len() as u64;

                blob_store
                    .store(&data)
                    .await
                    .map_err(|e| std::io::Error::other(e.to_string()))?;

                let rel_path = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");

                files.push(PatchFile {
                    path: rel_path,
                    hash,
                    size,
                });
            }
            // Symlinks and other special files are ignored.
        }
    }

    // PatchManifest::new sorts for us.
    Ok(PatchManifest::new(files))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indras_storage::{BlobStore, BlobStoreConfig};
    use std::collections::HashMap;

    /// Build a BlobStore backed by a subdirectory of `tmp`.
    async fn make_blob_store(tmp: &tempfile::TempDir) -> Arc<BlobStore> {
        let blob_dir = tmp.path().join("blobs");
        let store = BlobStore::new(BlobStoreConfig {
            base_dir: blob_dir,
            ..Default::default()
        })
        .await
        .expect("blob store");
        Arc::new(store)
    }

    /// Write `files` (relative-path → bytes) under `dir`.
    async fn write_files(dir: &Path, files: &[(&str, &[u8])]) {
        for (rel, data) in files {
            let full = dir.join(rel);
            if let Some(parent) = full.parent() {
                tokio::fs::create_dir_all(parent).await.unwrap();
            }
            tokio::fs::write(&full, data).await.unwrap();
        }
    }

    /// Round-trip: snapshot → wipe → materialize → verify bytes are identical.
    #[tokio::test]
    async fn round_trip_basic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().join("project");
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        let blob_store = make_blob_store(&tmp).await;

        let files: &[(&str, &[u8])] = &[
            ("hello.txt", b"hello world"),
            ("src/lib.rs", b"fn main() {}"),
            ("src/nested/deep.rs", b"// deep"),
        ];
        write_files(&project_dir, files).await;

        // Snapshot.
        let manifest = snapshot_dir(&project_dir, &blob_store)
            .await
            .expect("snapshot");
        assert_eq!(manifest.files.len(), 3);

        // Wipe project dir.
        tokio::fs::remove_dir_all(&project_dir).await.unwrap();
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        // Materialize.
        crate::project::materialize::materialize_to(&manifest, &project_dir, &blob_store)
            .await
            .expect("materialize");

        // Verify bytes.
        for (rel, expected) in files {
            let actual = tokio::fs::read(project_dir.join(rel)).await.unwrap();
            assert_eq!(actual.as_slice(), *expected, "mismatch for {rel}");
        }
    }

    /// Modify one file; only that file's hash should change in the new manifest.
    #[tokio::test]
    async fn incremental_hash_change() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().join("project");
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        let blob_store = make_blob_store(&tmp).await;

        let files: &[(&str, &[u8])] = &[
            ("a.txt", b"aaa"),
            ("b.txt", b"bbb"),
            ("c.txt", b"ccc"),
        ];
        write_files(&project_dir, files).await;

        let manifest1 = snapshot_dir(&project_dir, &blob_store)
            .await
            .expect("snapshot1");

        // Modify b.txt only.
        tokio::fs::write(project_dir.join("b.txt"), b"CHANGED")
            .await
            .unwrap();

        let manifest2 = snapshot_dir(&project_dir, &blob_store)
            .await
            .expect("snapshot2");

        // Build lookup maps by path.
        let m1: HashMap<&str, &PatchFile> = manifest1
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();
        let m2: HashMap<&str, &PatchFile> = manifest2
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();

        assert_eq!(m1["a.txt"].hash, m2["a.txt"].hash, "a.txt unchanged");
        assert_eq!(m1["c.txt"].hash, m2["c.txt"].hash, "c.txt unchanged");
        assert_ne!(m1["b.txt"].hash, m2["b.txt"].hash, "b.txt should change");
    }

    /// Empty directory produces an empty manifest and materializes without error.
    #[tokio::test]
    async fn empty_dir_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().join("project");
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        let blob_store = make_blob_store(&tmp).await;

        let manifest = snapshot_dir(&project_dir, &blob_store)
            .await
            .expect("snapshot");
        assert!(manifest.files.is_empty());

        let dest = tmp.path().join("dest");
        tokio::fs::create_dir_all(&dest).await.unwrap();

        crate::project::materialize::materialize_to(&manifest, &dest, &blob_store)
            .await
            .expect("materialize empty");
    }

    /// Top-level dotfiles are skipped; nested ones are not.
    #[tokio::test]
    async fn dotfile_skip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().join("project");
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        let blob_store = make_blob_store(&tmp).await;

        let files: &[(&str, &[u8])] = &[
            (".git/config", b"[core]"), // top-level .git → skipped
            ("src/.hidden", b"visible"), // nested dot → included
            ("readme.txt", b"hi"),
        ];
        write_files(&project_dir, files).await;

        let manifest = snapshot_dir(&project_dir, &blob_store)
            .await
            .expect("snapshot");

        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert!(
            !paths.iter().any(|p| p.starts_with(".git")),
            ".git should be skipped"
        );
        assert!(
            paths.contains(&"src/.hidden"),
            "nested dot file should be present"
        );
        assert!(paths.contains(&"readme.txt"));
    }

    /// Manifest entries are sorted by path.
    #[tokio::test]
    async fn manifest_is_sorted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_dir = tmp.path().join("project");
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        let blob_store = make_blob_store(&tmp).await;

        write_files(
            &project_dir,
            &[("z.txt", b"z"), ("a.txt", b"a"), ("m.txt", b"m")],
        )
        .await;

        let manifest = snapshot_dir(&project_dir, &blob_store)
            .await
            .expect("snapshot");

        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "manifest must be sorted by path");
    }
}
