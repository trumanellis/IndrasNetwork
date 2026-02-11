//! Event log implementation
//!
//! Provides per-interface append-only event logs with efficient seeking.

use std::collections::BTreeMap;
use std::io::SeekFrom;
use std::path::PathBuf;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

use indras_core::{EventId, InterfaceId, PeerIdentity};

use crate::error::StorageError;

/// Configuration for an event log
#[derive(Debug, Clone)]
pub struct EventLogConfig {
    /// Base directory for log files
    pub base_dir: PathBuf,
    /// Maximum size of a single log segment before rotation
    pub max_segment_size: u64,
    /// Whether to sync writes to disk immediately
    pub sync_on_write: bool,
    /// Index entries to keep in memory
    pub index_cache_size: usize,
}

impl Default for EventLogConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("./data/logs"),
            max_segment_size: 100 * 1024 * 1024, // 100MB
            sync_on_write: true,
            index_cache_size: 10000,
        }
    }
}

/// A single entry in the event log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLogEntry<I: PeerIdentity> {
    /// Event identifier
    pub event_id: EventId,
    /// Global sequence number in this log
    pub sequence: u64,
    /// Timestamp when the event was logged
    pub timestamp_millis: i64,
    /// The event payload (serialized InterfaceEvent)
    pub payload: Bytes,
    /// Optional blob reference for large payloads
    pub blob_ref: Option<BlobRef>,
    /// Phantom data for the identity type
    #[serde(skip)]
    pub _marker: std::marker::PhantomData<I>,
}

impl<I: PeerIdentity> EventLogEntry<I> {
    /// Create a new log entry
    pub fn new(event_id: EventId, sequence: u64, payload: Bytes) -> Self {
        Self {
            event_id,
            sequence,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            payload,
            blob_ref: None,
            _marker: std::marker::PhantomData,
        }
    }

    /// Create a log entry with a blob reference
    pub fn with_blob_ref(event_id: EventId, sequence: u64, blob_ref: BlobRef) -> Self {
        Self {
            event_id,
            sequence,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            payload: Bytes::new(),
            blob_ref: Some(blob_ref),
            _marker: std::marker::PhantomData,
        }
    }
}

/// Reference to a blob stored externally
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlobRef {
    /// BLAKE3 hash of the content
    pub hash: [u8; 32],
    /// Size of the blob in bytes
    pub size: u64,
}

impl BlobRef {
    /// Create a new blob reference
    pub fn new(hash: [u8; 32], size: u64) -> Self {
        Self { hash, size }
    }

    /// Get the hash as a hex string
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }
}

/// Per-interface append-only event log
pub struct EventLog<I: PeerIdentity> {
    /// Interface this log belongs to
    interface_id: InterfaceId,
    /// Configuration
    config: EventLogConfig,
    /// Current log file
    log_file: RwLock<Option<File>>,
    /// Path to the current log file
    log_path: PathBuf,
    /// In-memory index: event_id -> file offset
    index: RwLock<BTreeMap<EventId, u64>>,
    /// Current sequence number
    sequence: RwLock<u64>,
    /// Current file offset
    offset: RwLock<u64>,
    /// Marker for identity type
    _marker: std::marker::PhantomData<I>,
}

impl<I: PeerIdentity> EventLog<I> {
    /// Create a new event log for an interface
    #[instrument(skip_all)]
    pub async fn new(
        interface_id: InterfaceId,
        config: EventLogConfig,
    ) -> Result<Self, StorageError> {
        // Ensure base directory exists
        tokio::fs::create_dir_all(&config.base_dir)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let log_path = config
            .base_dir
            .join(format!("{}.log", hex::encode(interface_id.as_bytes())));

        info!(path = %log_path.display(), "Opening event log");

        let log = Self {
            interface_id,
            config,
            log_file: RwLock::new(None),
            log_path,
            index: RwLock::new(BTreeMap::new()),
            sequence: RwLock::new(0),
            offset: RwLock::new(0),
            _marker: std::marker::PhantomData,
        };

        // Open and replay to build index
        log.open_and_replay().await?;

        Ok(log)
    }

    /// Open the log file and replay to build index
    async fn open_and_replay(&self) -> Result<(), StorageError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.log_path)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let metadata = file
            .metadata()
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let file_size = metadata.len();

        if file_size > 0 {
            // Replay the log to build index
            self.replay_from_file(&file, file_size).await?;
        }

        *self.log_file.write().await = Some(file);
        *self.offset.write().await = file_size;

        let seq = *self.sequence.read().await;
        let entries = self.index.read().await.len();
        debug!(
            sequence = seq,
            offset = file_size,
            entries = entries,
            "Event log opened"
        );

        Ok(())
    }

    /// Replay a log file to rebuild the index
    async fn replay_from_file(&self, file: &File, file_size: u64) -> Result<(), StorageError> {
        let mut reader = BufReader::new(
            file.try_clone()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?,
        );
        let mut index = self.index.write().await;
        let mut sequence = self.sequence.write().await;
        let mut offset = 0u64;

        while offset < file_size {
            // Read length prefix (4 bytes)
            let mut len_buf = [0u8; 4];
            if reader.read_exact(&mut len_buf).await.is_err() {
                warn!(offset = offset, "Truncated log entry, stopping replay");
                break;
            }

            let entry_len = u32::from_be_bytes(len_buf) as usize;

            if entry_len == 0 || entry_len > 10 * 1024 * 1024 {
                warn!(offset = offset, len = entry_len, "Invalid entry length");
                break;
            }

            // Read the entry
            let mut entry_buf = vec![0u8; entry_len];
            if reader.read_exact(&mut entry_buf).await.is_err() {
                warn!(offset = offset, "Failed to read entry, stopping replay");
                break;
            }

            // Deserialize
            match postcard::from_bytes::<EventLogEntry<I>>(&entry_buf) {
                Ok(entry) => {
                    index.insert(entry.event_id, offset);
                    *sequence = (*sequence).max(entry.sequence + 1);
                }
                Err(e) => {
                    warn!(offset = offset, error = %e, "Failed to deserialize entry");
                    break;
                }
            }

            offset += 4 + entry_len as u64;
        }

        info!(entries = index.len(), "Replayed event log");
        Ok(())
    }

    /// Append an event to the log
    #[instrument(skip(self, payload), fields(event_id = ?event_id))]
    pub async fn append(&self, event_id: EventId, payload: Bytes) -> Result<u64, StorageError> {
        let sequence = {
            let mut seq = self.sequence.write().await;
            let current = *seq;
            *seq += 1;
            current
        };

        let entry = EventLogEntry::<I>::new(event_id, sequence, payload);
        self.write_entry(&entry).await?;

        debug!(sequence = sequence, "Appended event");
        Ok(sequence)
    }

    /// Append an event with a blob reference (for large payloads)
    pub async fn append_with_blob(
        &self,
        event_id: EventId,
        blob_ref: BlobRef,
    ) -> Result<u64, StorageError> {
        let sequence = {
            let mut seq = self.sequence.write().await;
            let current = *seq;
            *seq += 1;
            current
        };

        let entry = EventLogEntry::<I>::with_blob_ref(event_id, sequence, blob_ref);
        self.write_entry(&entry).await?;

        debug!(sequence = sequence, "Appended event with blob ref");
        Ok(sequence)
    }

    /// Write an entry to the log file
    async fn write_entry(&self, entry: &EventLogEntry<I>) -> Result<(), StorageError> {
        let serialized =
            postcard::to_allocvec(entry).map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut file_guard = self.log_file.write().await;
        let file = file_guard
            .as_mut()
            .ok_or_else(|| StorageError::Io("Log file not open".into()))?;

        // Write length prefix
        let len_bytes = (serialized.len() as u32).to_be_bytes();

        // Seek to end
        let offset = file
            .seek(SeekFrom::End(0))
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        file.write_all(&len_bytes)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;
        file.write_all(&serialized)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        if self.config.sync_on_write {
            file.sync_data()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }

        // Update index
        self.index.write().await.insert(entry.event_id, offset);
        *self.offset.write().await = offset + 4 + serialized.len() as u64;

        Ok(())
    }

    /// Read a specific event by ID
    pub async fn read_event(
        &self,
        event_id: EventId,
    ) -> Result<Option<EventLogEntry<I>>, StorageError> {
        let offset = {
            let index = self.index.read().await;
            match index.get(&event_id) {
                Some(&o) => o,
                None => return Ok(None),
            }
        };

        self.read_at_offset(offset).await.map(Some)
    }

    /// Read an event at a specific file offset
    async fn read_at_offset(&self, offset: u64) -> Result<EventLogEntry<I>, StorageError> {
        let file_guard = self.log_file.read().await;
        let file = file_guard
            .as_ref()
            .ok_or_else(|| StorageError::Io("Log file not open".into()))?;

        let mut file = file
            .try_clone()
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Read length
        let mut len_buf = [0u8; 4];
        file.read_exact(&mut len_buf)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let entry_len = u32::from_be_bytes(len_buf) as usize;

        // Read entry
        let mut entry_buf = vec![0u8; entry_len];
        file.read_exact(&mut entry_buf)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        postcard::from_bytes(&entry_buf).map_err(|e| StorageError::Deserialization(e.to_string()))
    }

    /// Read events since a sequence number
    pub async fn read_since(
        &self,
        since_sequence: u64,
    ) -> Result<Vec<EventLogEntry<I>>, StorageError> {
        let offsets: Vec<u64> = {
            let file_guard = self.log_file.read().await;
            if file_guard.is_none() {
                return Ok(Vec::new());
            }

            // We need to scan through the index to find entries with sequence >= since_sequence
            // Since index is keyed by EventId, we need to read all entries
            let index = self.index.read().await;
            index.values().copied().collect()
        };

        let mut entries = Vec::new();
        for offset in offsets {
            let entry = self.read_at_offset(offset).await?;
            if entry.sequence >= since_sequence {
                entries.push(entry);
            }
        }

        // Sort by sequence
        entries.sort_by_key(|e| e.sequence);

        Ok(entries)
    }

    /// Read all events (for recovery/replay)
    pub async fn read_all(&self) -> Result<Vec<EventLogEntry<I>>, StorageError> {
        self.read_since(0).await
    }

    /// Get the current sequence number
    pub async fn current_sequence(&self) -> u64 {
        *self.sequence.read().await
    }

    /// Get the number of events in the log
    pub async fn event_count(&self) -> usize {
        self.index.read().await.len()
    }

    /// Get the interface ID
    pub fn interface_id(&self) -> InterfaceId {
        self.interface_id
    }

    /// Close the log file
    pub async fn close(&self) -> Result<(), StorageError> {
        let mut file_guard = self.log_file.write().await;
        if let Some(file) = file_guard.take() {
            file.sync_all()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }
        Ok(())
    }
}

// Reserved for future use (debugging/logging)
#[allow(dead_code)]
fn hex_encode_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;
    use tempfile::TempDir;

    async fn create_test_log() -> (EventLog<SimulationIdentity>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = EventLogConfig {
            base_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let interface_id = InterfaceId::new([0x42; 32]);
        let log = EventLog::new(interface_id, config).await.unwrap();
        (log, temp_dir)
    }

    #[tokio::test]
    async fn test_append_and_read() {
        let (log, _temp) = create_test_log().await;

        let event_id = EventId::new(12345, 1);
        let payload = Bytes::from("test payload");

        let seq = log.append(event_id, payload.clone()).await.unwrap();
        assert_eq!(seq, 0);

        let entry = log.read_event(event_id).await.unwrap().unwrap();
        assert_eq!(entry.event_id, event_id);
        assert_eq!(entry.sequence, 0);
        assert_eq!(entry.payload, payload);
    }

    #[tokio::test]
    async fn test_multiple_events() {
        let (log, _temp) = create_test_log().await;

        for i in 0..10 {
            let event_id = EventId::new(1, i);
            let payload = Bytes::from(format!("payload {}", i));
            let seq = log.append(event_id, payload).await.unwrap();
            assert_eq!(seq, i);
        }

        assert_eq!(log.event_count().await, 10);
        assert_eq!(log.current_sequence().await, 10);
    }

    #[tokio::test]
    async fn test_read_since() {
        let (log, _temp) = create_test_log().await;

        for i in 0..10 {
            let event_id = EventId::new(1, i);
            log.append(event_id, Bytes::from("data")).await.unwrap();
        }

        let entries = log.read_since(5).await.unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].sequence, 5);
        assert_eq!(entries[4].sequence, 9);
    }

    #[tokio::test]
    async fn test_persistence_and_replay() {
        let temp_dir = TempDir::new().unwrap();
        let interface_id = InterfaceId::new([0xAB; 32]);

        // Create and write events
        {
            let config = EventLogConfig {
                base_dir: temp_dir.path().to_path_buf(),
                ..Default::default()
            };
            let log: EventLog<SimulationIdentity> =
                EventLog::new(interface_id, config).await.unwrap();

            for i in 0..5 {
                log.append(EventId::new(1, i), Bytes::from("data"))
                    .await
                    .unwrap();
            }
            log.close().await.unwrap();
        }

        // Reopen and verify replay
        {
            let config = EventLogConfig {
                base_dir: temp_dir.path().to_path_buf(),
                ..Default::default()
            };
            let log: EventLog<SimulationIdentity> =
                EventLog::new(interface_id, config).await.unwrap();

            assert_eq!(log.event_count().await, 5);
            assert_eq!(log.current_sequence().await, 5);

            // Can still read events
            let entry = log.read_event(EventId::new(1, 2)).await.unwrap().unwrap();
            assert_eq!(entry.sequence, 2);
        }
    }
}
