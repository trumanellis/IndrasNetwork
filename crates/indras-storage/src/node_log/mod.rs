//! Node-level event log
//!
//! A unified, append-only event log that records every state-mutating action
//! at the node level. Unlike per-interface event logs, this provides a single
//! audit trail across all interfaces.
//!
//! ## Storage Format
//!
//! Uses the same length-prefixed postcard pattern as `EventLog`:
//! ```text
//! [4-byte BE length][postcard NodeLogEntry][4-byte BE length][postcard NodeLogEntry]...
//! ```
//!
//! ## Hash Chain
//!
//! Each entry's `prev_hash` is the BLAKE3 hash of the previous entry's
//! serialized bytes, forming a tamper-evident chain. The genesis entry
//! uses `[0; 32]` as its `prev_hash`.

pub mod entry;
pub mod event;

pub use entry::{NodeLogEntry, NodeLogMeta, NodeSequence};
pub use event::NodeEvent;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::error::StorageError;
use crate::structured::{NODE_LOG_INDEX, NODE_LOG_META, RedbStorage};

/// Node-level event log
///
/// Records every state-mutating action across all interfaces in a single
/// append-only log with BLAKE3 hash chaining for integrity verification.
pub struct NodeLog {
    /// Path to the log file (reserved for future snapshot/compaction)
    #[allow(dead_code)]
    log_path: PathBuf,
    /// The log file handle (mutex for exclusive write access)
    log_file: Mutex<File>,
    /// Current sequence number (atomic for lock-free reads)
    sequence: AtomicU64,
    /// BLAKE3 hash of the last entry (mutex-protected)
    last_hash: Mutex<[u8; 32]>,
    /// redb storage for index and metadata
    redb: Arc<RedbStorage>,
}

impl NodeLog {
    /// Open or create the node log
    ///
    /// Loads metadata from redb to recover sequence and hash state.
    /// If no metadata exists, starts a fresh log.
    pub async fn open(base_dir: &std::path::Path, redb: Arc<RedbStorage>) -> Result<Self, StorageError> {
        tokio::fs::create_dir_all(base_dir)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let log_path = base_dir.join("node_log.bin");

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&log_path)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Try to load metadata from redb
        let (sequence, last_hash) = match redb.get(NODE_LOG_META, b"meta")? {
            Some(meta_bytes) => {
                let meta: NodeLogMeta = postcard::from_bytes(&meta_bytes)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                (meta.current_sequence, meta.last_hash)
            }
            None => (0u64, [0u8; 32]),
        };

        info!(
            path = %log_path.display(),
            sequence = sequence,
            "Node log opened"
        );

        Ok(Self {
            log_path,
            log_file: Mutex::new(file),
            sequence: AtomicU64::new(sequence),
            last_hash: Mutex::new(last_hash),
            redb,
        })
    }

    /// Append an event to the node log
    ///
    /// Atomically: increments sequence, computes hash chain, serializes entry,
    /// writes to file with fsync, and updates redb index + metadata.
    ///
    /// Returns the sequence number assigned to this entry.
    pub async fn append(&self, event: NodeEvent) -> Result<NodeSequence, StorageError> {
        let mut file = self.log_file.lock().await;
        let mut last_hash = self.last_hash.lock().await;

        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);

        let entry = NodeLogEntry {
            sequence,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            event,
            prev_hash: *last_hash,
        };

        let serialized = postcard::to_allocvec(&entry)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        // Compute hash of this entry for the chain
        let entry_hash = blake3::hash(&serialized);
        let entry_hash_bytes: [u8; 32] = *entry_hash.as_bytes();

        // Write length-prefixed entry to file
        let offset = file
            .seek(std::io::SeekFrom::End(0))
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let len_bytes = (serialized.len() as u32).to_be_bytes();
        file.write_all(&len_bytes)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;
        file.write_all(&serialized)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;
        file.sync_data()
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let new_file_size = offset + 4 + serialized.len() as u64;

        // Update redb index: sequence -> file offset
        self.redb.put(
            NODE_LOG_INDEX,
            &sequence.to_be_bytes(),
            &offset.to_be_bytes(),
        )?;

        // Update redb metadata
        let new_sequence = sequence + 1;
        let meta = NodeLogMeta {
            current_sequence: new_sequence,
            last_hash: entry_hash_bytes,
            entry_count: new_sequence,
            file_size: new_file_size,
            last_snapshot_sequence: None,
        };
        let meta_bytes = postcard::to_allocvec(&meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        self.redb.put(NODE_LOG_META, b"meta", &meta_bytes)?;

        // Update in-memory state
        *last_hash = entry_hash_bytes;

        debug!(sequence = sequence, "Appended node log entry");
        Ok(sequence)
    }

    /// Read a single entry by sequence number
    pub async fn read_entry(&self, sequence: NodeSequence) -> Result<Option<NodeLogEntry>, StorageError> {
        // Look up offset in redb index
        let seq_bytes = sequence.to_be_bytes();
        let offset_bytes: Vec<u8> = match self.redb.get(NODE_LOG_INDEX, &seq_bytes)? {
            Some(b) => b,
            None => return Ok(None),
        };

        if offset_bytes.len() != 8 {
            return Err(StorageError::Deserialization("invalid offset length".into()));
        }
        let offset = u64::from_be_bytes(<[u8; 8]>::try_from(offset_bytes.as_slice()).unwrap());

        self.read_at_offset(offset).await.map(Some)
    }

    /// Read all entries since (and including) `since_sequence`
    pub async fn read_since(&self, since_sequence: NodeSequence) -> Result<Vec<NodeLogEntry>, StorageError> {
        let current = self.current_sequence();
        let mut entries = Vec::new();

        for seq in since_sequence..current {
            if let Some(entry) = self.read_entry(seq).await? {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Get the current (next) sequence number
    pub fn current_sequence(&self) -> NodeSequence {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Verify the hash chain integrity between two sequence numbers (inclusive)
    pub async fn verify_chain(&self, from: NodeSequence, to: NodeSequence) -> Result<bool, StorageError> {
        if from > to {
            return Ok(true);
        }

        let entries = self.read_since(from).await?;
        if entries.is_empty() {
            return Ok(true);
        }

        // Verify first entry's prev_hash
        if from > 0 {
            if let Some(prev_entry) = self.read_entry(from - 1).await? {
                let prev_serialized = postcard::to_allocvec(&prev_entry)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                let expected_hash = blake3::hash(&prev_serialized);
                if entries[0].prev_hash != *expected_hash.as_bytes() {
                    return Ok(false);
                }
            }
        } else if entries[0].prev_hash != [0u8; 32] {
            return Ok(false); // Genesis must have zero hash
        }

        // Verify chain continuity
        for window in entries.windows(2) {
            let prev_serialized = postcard::to_allocvec(&window[0])
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            let expected_hash = blake3::hash(&prev_serialized);
            if window[1].prev_hash != *expected_hash.as_bytes() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Read an entry at a specific file offset
    async fn read_at_offset(&self, offset: u64) -> Result<NodeLogEntry, StorageError> {
        let mut file = self.log_file.lock().await;

        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Read length prefix
        let mut len_buf = [0u8; 4];
        file.read_exact(&mut len_buf)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let entry_len = u32::from_be_bytes(len_buf) as usize;

        // Read entry bytes
        let mut entry_buf = vec![0u8; entry_len];
        file.read_exact(&mut entry_buf)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        postcard::from_bytes(&entry_buf)
            .map_err(|e| StorageError::Deserialization(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::structured::RedbStorageConfig;

    async fn create_test_log() -> (NodeLog, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let redb_config = RedbStorageConfig {
            db_path: temp_dir.path().join("test.redb"),
            ..Default::default()
        };
        let redb = Arc::new(RedbStorage::open(redb_config).unwrap());
        let log = NodeLog::open(temp_dir.path(), redb).await.unwrap();
        (log, temp_dir)
    }

    #[tokio::test]
    async fn test_append_and_read() {
        let (log, _temp) = create_test_log().await;

        let seq = log.append(NodeEvent::NodeStarted {
            identity_fingerprint: [0x42; 32],
        }).await.unwrap();
        assert_eq!(seq, 0);

        let seq = log.append(NodeEvent::InterfaceCreated {
            interface_id: indras_core::InterfaceId::new([0xAB; 32]),
            name: Some("Test".to_string()),
        }).await.unwrap();
        assert_eq!(seq, 1);

        // Read back
        let entry = log.read_entry(0).await.unwrap().unwrap();
        assert_eq!(entry.sequence, 0);
        matches!(entry.event, NodeEvent::NodeStarted { .. });

        let entry = log.read_entry(1).await.unwrap().unwrap();
        assert_eq!(entry.sequence, 1);
        matches!(entry.event, NodeEvent::InterfaceCreated { .. });

        // Non-existent
        assert!(log.read_entry(99).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let redb_config = RedbStorageConfig {
            db_path: temp_dir.path().join("test.redb"),
            ..Default::default()
        };
        let redb = Arc::new(RedbStorage::open(redb_config.clone()).unwrap());

        // Write some entries
        {
            let log = NodeLog::open(temp_dir.path(), redb).await.unwrap();
            for i in 0..5 {
                log.append(NodeEvent::InterfaceCreated {
                    interface_id: indras_core::InterfaceId::new([i as u8; 32]),
                    name: None,
                }).await.unwrap();
            }
            assert_eq!(log.current_sequence(), 5);
        }

        // Reopen and verify sequence continues
        let redb2 = Arc::new(RedbStorage::open(redb_config).unwrap());
        let log = NodeLog::open(temp_dir.path(), redb2).await.unwrap();
        assert_eq!(log.current_sequence(), 5);

        // Append more
        let seq = log.append(NodeEvent::NodeStopped).await.unwrap();
        assert_eq!(seq, 5);
        assert_eq!(log.current_sequence(), 6);

        // Can read old entries
        let entry = log.read_entry(2).await.unwrap().unwrap();
        assert_eq!(entry.sequence, 2);
    }

    #[tokio::test]
    async fn test_hash_chain() {
        let (log, _temp) = create_test_log().await;

        // Genesis entry should have zero prev_hash
        log.append(NodeEvent::NodeStarted {
            identity_fingerprint: [0x01; 32],
        }).await.unwrap();

        let genesis = log.read_entry(0).await.unwrap().unwrap();
        assert_eq!(genesis.prev_hash, [0u8; 32]);

        // Subsequent entries should chain
        for i in 1..5 {
            log.append(NodeEvent::InterfaceCreated {
                interface_id: indras_core::InterfaceId::new([i as u8; 32]),
                name: None,
            }).await.unwrap();
        }

        // Verify chain
        assert!(log.verify_chain(0, 4).await.unwrap());
    }

    #[tokio::test]
    async fn test_read_since() {
        let (log, _temp) = create_test_log().await;

        for i in 0..10 {
            log.append(NodeEvent::InterfaceCreated {
                interface_id: indras_core::InterfaceId::new([i as u8; 32]),
                name: None,
            }).await.unwrap();
        }

        let entries = log.read_since(5).await.unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].sequence, 5);
        assert_eq!(entries[4].sequence, 9);

        let all = log.read_since(0).await.unwrap();
        assert_eq!(all.len(), 10);
    }
}
