//! VaultFileDocument — local-only index tracking the vault file tree.
//!
//! This is a local checkout index derived from the peer's current DAG head.
//! It is NOT a shared CRDT — there is no merge across peers. The braid DAG
//! is the single source of truth for file state across peers.

use super::vault_file::{UserId, VaultFile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Local-only index tracking all files in a vault checkout.
///
/// Derived from the peer's current DAG head. Not synced across peers —
/// the braid DAG is the shared source of truth.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultFileDocument {
    /// All tracked files, keyed by relative path.
    pub files: BTreeMap<String, VaultFile>,
}

impl VaultFileDocument {
    /// Insert or update a file by path.
    pub fn upsert(&mut self, file: VaultFile) {
        self.files.insert(file.path.clone(), file);
    }

    /// Tombstone a file (mark as deleted).
    pub fn remove(&mut self, path: &str, author: UserId) {
        if let Some(file) = self.files.get_mut(path) {
            file.deleted = true;
            file.modified_ms = chrono::Utc::now().timestamp_millis();
            file.author = author;
        }
    }

    /// Return all non-deleted files.
    pub fn active_files(&self) -> Vec<&VaultFile> {
        self.files.values().filter(|f| !f.deleted).collect()
    }

    /// Count of active (non-deleted) files.
    pub fn len(&self) -> usize {
        self.files.values().filter(|f| !f.deleted).count()
    }

    /// Whether there are no active files.
    pub fn is_empty(&self) -> bool {
        !self.files.values().any(|f| !f.deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member_a() -> UserId {
        [1u8; 32]
    }
    fn member_b() -> UserId {
        [2u8; 32]
    }

    #[test]
    fn upsert_and_remove() {
        let mut doc = VaultFileDocument::default();
        let hash = *blake3::hash(b"hello").as_bytes();
        doc.upsert(VaultFile::new("a.md", hash, 5, member_a()));
        assert_eq!(doc.len(), 1);

        doc.remove("a.md", member_b());
        assert_eq!(doc.len(), 0);
        assert!(doc.files["a.md"].deleted);
    }

    #[test]
    fn active_files_excludes_deleted() {
        let mut doc = VaultFileDocument::default();
        let h1 = *blake3::hash(b"one").as_bytes();
        let h2 = *blake3::hash(b"two").as_bytes();
        doc.upsert(VaultFile::new("a.md", h1, 3, member_a()));
        doc.upsert(VaultFile::new("b.md", h2, 3, member_b()));
        assert_eq!(doc.active_files().len(), 2);

        doc.remove("a.md", member_a());
        assert_eq!(doc.active_files().len(), 1);
        assert_eq!(doc.active_files()[0].path, "b.md");
    }

    #[test]
    fn serialization_round_trip() {
        let mut doc = VaultFileDocument::default();
        let h1 = *blake3::hash(b"aaa").as_bytes();
        let h2 = *blake3::hash(b"bbb").as_bytes();
        doc.upsert(VaultFile::new("a.md", h1, 3, member_a()));
        doc.upsert(VaultFile::new("b.md", h2, 3, member_b()));

        let bytes = postcard::to_allocvec(&doc).unwrap();
        let decoded: VaultFileDocument = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(doc.files.len(), decoded.files.len());
        for (path, file) in &doc.files {
            assert_eq!(file, &decoded.files[path]);
        }
    }
}
