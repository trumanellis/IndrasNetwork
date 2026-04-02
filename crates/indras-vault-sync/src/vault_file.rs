//! Vault file types — tracked files and conflict records.

use indras_network::member::MemberId;
use serde::{Deserialize, Serialize};

/// Default conflict detection window in milliseconds (60 seconds).
pub const CONFLICT_WINDOW_MS: i64 = 60_000;

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
    /// The member who last modified this file.
    pub author: MemberId,
    /// Tombstone: if true, this file has been deleted.
    #[serde(default)]
    pub deleted: bool,
}

impl VaultFile {
    /// Create a new vault file entry.
    pub fn new(path: impl Into<String>, hash: [u8; 32], size: u64, author: MemberId) -> Self {
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
        author: MemberId,
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

/// Record of a detected conflict (two peers edited same file concurrently).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConflictRecord {
    /// The file path that had a conflict.
    pub path: String,
    /// Hash of the winning version (kept as the main file).
    pub winner_hash: [u8; 32],
    /// Hash of the losing version (saved as .conflict-<hash> copy).
    pub loser_hash: [u8; 32],
    /// The member who authored the losing version.
    pub loser_author: MemberId,
    /// When the conflict was detected (Unix milliseconds).
    pub detected_ms: i64,
    /// Whether this conflict has been resolved by the user.
    #[serde(default)]
    pub resolved: bool,
}

impl ConflictRecord {
    /// Unique identity for dedup: (path, loser_hash).
    pub fn dedup_key(&self) -> (&str, &[u8; 32]) {
        (&self.path, &self.loser_hash)
    }

    /// The filename for the conflict copy, e.g. "notes/daily.conflict-ab1234.md"
    pub fn conflict_filename(&self) -> String {
        let short = hex::encode(&self.loser_hash[..6]);
        if let Some(dot_pos) = self.path.rfind('.') {
            let (stem, ext) = self.path.split_at(dot_pos);
            format!("{}.conflict-{}{}", stem, short, ext)
        } else {
            format!("{}.conflict-{}", self.path, short)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member() -> MemberId {
        [1u8; 32]
    }

    fn other_member() -> MemberId {
        [2u8; 32]
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
    fn conflict_filename_with_extension() {
        let loser_hash = [0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0, 0,
                          0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                          0, 0, 0, 0, 0, 0, 0, 0];
        let conflict = ConflictRecord {
            path: "notes/daily.md".into(),
            winner_hash: [0; 32],
            loser_hash,
            loser_author: other_member(),
            detected_ms: 1000,
            resolved: false,
        };
        assert_eq!(conflict.conflict_filename(), "notes/daily.conflict-abcdef123456.md");
    }

    #[test]
    fn conflict_filename_without_extension() {
        let loser_hash = [0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0, 0,
                          0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                          0, 0, 0, 0, 0, 0, 0, 0];
        let conflict = ConflictRecord {
            path: "LICENSE".into(),
            winner_hash: [0; 32],
            loser_hash,
            loser_author: other_member(),
            detected_ms: 1000,
            resolved: false,
        };
        assert_eq!(conflict.conflict_filename(), "LICENSE.conflict-abcdef123456");
    }

    #[test]
    fn conflict_dedup_key() {
        let loser_hash = [1u8; 32];
        let conflict = ConflictRecord {
            path: "a.md".into(),
            winner_hash: [0; 32],
            loser_hash,
            loser_author: other_member(),
            detected_ms: 1000,
            resolved: false,
        };
        assert_eq!(conflict.dedup_key(), ("a.md", &loser_hash));
    }

    #[test]
    fn serialization_round_trip_vault_file() {
        let hash = blake3::hash(b"content").as_bytes().to_owned();
        let file = VaultFile::new("path/to/file.md", hash, 42, test_member());
        let bytes = postcard::to_allocvec(&file).unwrap();
        let decoded: VaultFile = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(file, decoded);
    }

    #[test]
    fn serialization_round_trip_conflict() {
        let conflict = ConflictRecord {
            path: "a.md".into(),
            winner_hash: [1; 32],
            loser_hash: [2; 32],
            loser_author: other_member(),
            detected_ms: 12345,
            resolved: false,
        };
        let bytes = postcard::to_allocvec(&conflict).unwrap();
        let decoded: ConflictRecord = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(conflict, decoded);
    }
}
