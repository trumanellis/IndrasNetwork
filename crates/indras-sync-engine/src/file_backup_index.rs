//! Home-realm index of every file the account currently backs up.
//!
//! When the sender publishes a new file via `publish_file_shards`,
//! it also bumps an entry in this doc. On recovery the new device
//! reads the index to learn *which* `file_id`s to fetch shards
//! for; without the index a new device can't enumerate the
//! `_file_shard:{file_id}:*` universe (internal `_`-prefixed doc
//! keys don't show up in `Realm::document_names`).
//!
//! Stored under the well-known key [`FILE_BACKUP_INDEX_DOC_KEY`]
//! in the home realm. Merge is per-file LWW on
//! `last_updated_at_millis` so concurrent writes from multiple
//! trusted devices compose cleanly.

use serde::{Deserialize, Serialize};

use indras_network::document::DocumentSchema;

/// Home-realm doc key for the backup index.
pub const FILE_BACKUP_INDEX_DOC_KEY: &str = "_file_backup_index";

/// One entry per file currently known to the account's backup
/// pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileBackupEntry {
    /// 32-byte content-addressed hash of the plaintext.
    pub file_id: [u8; 32],
    /// Human-readable path/label at last publish time.
    pub label: String,
    /// Total shards published (= peer count at publish time).
    pub total_shards: u8,
    /// K-of-N reconstruction threshold.
    pub data_threshold: u8,
    /// Wall-clock millis of the most recent publish.
    pub last_updated_at_millis: i64,
    /// `true` once the user deletes the file. Readers stop
    /// expecting shards, but the entry remains so stale peers
    /// don't re-add deleted files on the next sync.
    pub tombstoned: bool,
}

/// Home-realm document listing every backed-up file for the
/// account. One entry per `file_id`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileBackupIndex {
    pub entries: Vec<FileBackupEntry>,
}

impl FileBackupIndex {
    /// Upsert an entry, preserving the newer timestamp on conflict.
    pub fn upsert(&mut self, entry: FileBackupEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.file_id == entry.file_id)
        {
            if entry.last_updated_at_millis > existing.last_updated_at_millis {
                *existing = entry;
            }
        } else {
            self.entries.push(entry);
        }
    }

    /// Active (non-tombstoned) entries.
    pub fn active(&self) -> impl Iterator<Item = &FileBackupEntry> {
        self.entries.iter().filter(|e| !e.tombstoned)
    }
}

impl DocumentSchema for FileBackupIndex {
    fn merge(&mut self, remote: Self) {
        for entry in remote.entries {
            self.upsert(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(id: u8, label: &str, ts: i64) -> FileBackupEntry {
        let mut file_id = [0u8; 32];
        file_id[0] = id;
        FileBackupEntry {
            file_id,
            label: label.into(),
            total_shards: 5,
            data_threshold: 3,
            last_updated_at_millis: ts,
            tombstoned: false,
        }
    }

    #[test]
    fn upsert_adds_new_and_replaces_older() {
        let mut idx = FileBackupIndex::default();
        idx.upsert(make(1, "a", 100));
        assert_eq!(idx.entries.len(), 1);

        idx.upsert(make(2, "b", 200));
        assert_eq!(idx.entries.len(), 2);

        idx.upsert(make(1, "a-v2", 300));
        assert_eq!(idx.entries.len(), 2);
        assert_eq!(idx.entries.iter().find(|e| e.file_id[0] == 1).unwrap().label, "a-v2");

        // Older writes lose.
        idx.upsert(make(1, "stale", 50));
        assert_eq!(idx.entries.iter().find(|e| e.file_id[0] == 1).unwrap().label, "a-v2");
    }

    #[test]
    fn tombstone_hides_from_active_iter() {
        let mut idx = FileBackupIndex::default();
        idx.upsert(make(1, "keep", 100));
        let mut gone = make(2, "gone", 100);
        gone.tombstoned = true;
        idx.upsert(gone);
        let actives: Vec<_> = idx.active().collect();
        assert_eq!(actives.len(), 1);
        assert_eq!(actives[0].label, "keep");
    }

    #[test]
    fn merge_composes_upserts_from_two_sources() {
        let mut a = FileBackupIndex::default();
        a.upsert(make(1, "a-v1", 100));
        a.upsert(make(2, "b", 100));

        let mut b = FileBackupIndex::default();
        b.upsert(make(1, "a-v2", 200));
        b.upsert(make(3, "c", 100));

        a.merge(b);
        assert_eq!(a.entries.len(), 3);
        assert_eq!(a.entries.iter().find(|e| e.file_id[0] == 1).unwrap().label, "a-v2");
    }
}
