//! Vault file types — tracked files and conflict records.

use serde::{Deserialize, Serialize};

/// User-level identity: BLAKE3 hash of the PQ verifying key.
///
/// Shared across all devices belonging to the same user. Unlike `MemberId`
/// (derived from iroh transport key, unique per device), `UserId` identifies
/// the human user. Used for vault authorship and conflict detection so that
/// same-user edits from different devices don't create spurious conflicts.
pub type UserId = [u8; 32];

/// A tracked file in the vault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaultFile {
    /// Relative path from vault root (forward slashes), e.g. "notes/daily.md".
    pub path: String,
    /// BLAKE3 hash of the file content.
    pub hash: [u8; 32],
    /// File size in bytes.
    pub size: u64,
    /// Last modification time (Unix milliseconds) — LWW tiebreaker.
    pub modified_ms: i64,
    /// The user who last modified this file (PQ-derived UserId, not device MemberId).
    pub author: UserId,
    /// Tombstone: if true, this file has been deleted.
    pub deleted: bool,
}

impl VaultFile {
    /// Create a new vault file entry.
    pub fn new(path: impl Into<String>, hash: [u8; 32], size: u64, author: UserId) -> Self {
        Self {
            path: path.into(),
            hash,
            size,
            modified_ms: chrono::Utc::now().timestamp_millis(),
            author,
            deleted: false,
        }
    }

    /// Create a vault file with an explicit timestamp.
    pub fn with_timestamp(
        path: impl Into<String>,
        hash: [u8; 32],
        size: u64,
        author: UserId,
        modified_ms: i64,
    ) -> Self {
        Self {
            path: path.into(),
            hash,
            size,
            modified_ms,
            author,
            deleted: false,
        }
    }

    /// Short hex hash for display/conflict file naming.
    pub fn short_hash(&self) -> String {
        hex::encode(&self.hash[..6])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member() -> UserId {
        [1u8; 32]
    }

    #[test]
    fn vault_file_creation() {
        let hash = blake3::hash(b"hello").as_bytes().to_owned();
        let file = VaultFile::new("notes/daily.md", hash, 5, test_member());
        assert_eq!(file.path, "notes/daily.md");
        assert_eq!(file.hash, hash);
        assert_eq!(file.size, 5);
        assert!(!file.deleted);
        assert!(file.modified_ms > 0);
    }

    #[test]
    fn vault_file_short_hash() {
        let hash = [0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x9a,
                     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                     0, 0, 0, 0, 0, 0, 0, 0];
        let file = VaultFile::new("test.md", hash, 0, test_member());
        assert_eq!(file.short_hash(), "abcdef123456");
    }

    #[test]
    fn serialization_round_trip_vault_file() {
        let hash = blake3::hash(b"content").as_bytes().to_owned();
        let file = VaultFile::new("path/to/file.md", hash, 42, test_member());
        let bytes = postcard::to_allocvec(&file).unwrap();
        let decoded: VaultFile = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(file, decoded);
    }
}
