//! Persistent storage implementations
//!
//! This module provides file-based persistent storage for pending events,
//! using an append-only log format for durability.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use dashmap::DashMap;
use indras_core::{EventId, PeerIdentity};
use serde::{Deserialize, Serialize};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::RwLock;
use tracing::{debug, info, trace, warn};

use crate::PendingStore;
use crate::error::StorageError;
use crate::quota::QuotaManager;

/// Entry type in the append-only log
#[derive(Debug, Clone, Serialize, Deserialize)]
enum LogEntry {
    /// Mark an event as pending for a peer
    MarkPending {
        /// Peer bytes (since PeerIdentity doesn't have consistent string format)
        peer_bytes: Vec<u8>,
        /// The event ID
        event_id: EventId,
    },
    /// Mark an event as delivered
    MarkDelivered {
        /// Peer bytes
        peer_bytes: Vec<u8>,
        /// The event ID that was delivered
        event_id: EventId,
    },
    /// Mark all events up to a certain ID as delivered
    MarkDeliveredUpTo {
        /// Peer bytes
        peer_bytes: Vec<u8>,
        /// The event ID to deliver up to
        up_to: EventId,
    },
    /// Clear all pending for a peer
    ClearPending {
        /// Peer bytes
        peer_bytes: Vec<u8>,
    },
}

/// Persistent implementation of PendingStore
///
/// Uses an append-only log file for persistence, with an in-memory
/// DashMap for fast access. The log is replayed on startup to
/// reconstruct the in-memory state.
#[derive(Debug)]
pub struct PersistentPendingStore {
    /// Path to the storage directory
    storage_path: PathBuf,
    /// In-memory cache of pending events
    pending: DashMap<Vec<u8>, BTreeSet<EventId>>,
    /// Quota manager for capacity limits
    quota: QuotaManager,
    /// Total count of pending events
    total_count: AtomicUsize,
    /// Write handle for the append-only log
    writer: Arc<RwLock<Option<BufWriter<File>>>>,
    /// Whether to sync writes immediately (durability vs performance)
    sync_writes: bool,
}

impl PersistentPendingStore {
    /// Create a new persistent pending store at the given path
    pub async fn new(storage_path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::with_options(storage_path, QuotaManager::default(), true).await
    }

    /// Create with custom quota manager and sync options
    pub async fn with_options(
        storage_path: impl AsRef<Path>,
        quota: QuotaManager,
        sync_writes: bool,
    ) -> Result<Self, StorageError> {
        let storage_path = storage_path.as_ref().to_path_buf();

        // Ensure the storage directory exists
        tokio::fs::create_dir_all(&storage_path).await?;

        let store = Self {
            storage_path,
            pending: DashMap::new(),
            quota,
            total_count: AtomicUsize::new(0),
            writer: Arc::new(RwLock::new(None)),
            sync_writes,
        };

        // Load existing data and open writer
        store.load().await?;
        store.open_writer().await?;

        Ok(store)
    }

    /// Get the log file path
    fn log_path(&self) -> PathBuf {
        self.storage_path.join("pending.log")
    }

    /// Load existing log entries and replay them
    async fn load(&self) -> Result<(), StorageError> {
        let log_path = self.log_path();

        if !log_path.exists() {
            debug!(path = ?log_path, "No existing log file, starting fresh");
            return Ok(());
        }

        info!(path = ?log_path, "Loading pending events from log");

        let file = File::open(&log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut loaded_count = 0;
        let mut error_count = 0;

        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            match postcard::from_bytes::<LogEntry>(line.as_bytes()) {
                Ok(entry) => {
                    self.apply_entry(entry);
                    loaded_count += 1;
                }
                Err(e) => {
                    // Try to decode from base64 (for binary postcard data)
                    // The log format uses newline-delimited base64-encoded postcard entries
                    if let Ok(decoded) = base64_decode(&line)
                        && let Ok(entry) = postcard::from_bytes::<LogEntry>(&decoded)
                    {
                        self.apply_entry(entry);
                        loaded_count += 1;
                        continue;
                    }
                    error_count += 1;
                    warn!(error = %e, "Failed to parse log entry, skipping");
                }
            }
        }

        info!(
            loaded = loaded_count,
            errors = error_count,
            total_pending = self.total_count.load(Ordering::SeqCst),
            peer_count = self.pending.len(),
            "Finished loading pending events"
        );

        Ok(())
    }

    /// Apply a log entry to the in-memory state
    fn apply_entry(&self, entry: LogEntry) {
        match entry {
            LogEntry::MarkPending {
                peer_bytes,
                event_id,
            } => {
                let mut events = self.pending.entry(peer_bytes).or_default();
                if events.insert(event_id) {
                    self.total_count.fetch_add(1, Ordering::SeqCst);
                }
            }
            LogEntry::MarkDelivered {
                peer_bytes,
                event_id,
            } => {
                if let Some(mut events) = self.pending.get_mut(&peer_bytes)
                    && events.remove(&event_id)
                {
                    self.total_count.fetch_sub(1, Ordering::SeqCst);
                }
            }
            LogEntry::MarkDeliveredUpTo { peer_bytes, up_to } => {
                if let Some(mut events) = self.pending.get_mut(&peer_bytes) {
                    let to_remove: Vec<_> = events
                        .iter()
                        .filter(|id| {
                            id.sender_hash == up_to.sender_hash && id.sequence <= up_to.sequence
                        })
                        .copied()
                        .collect();
                    let count = to_remove.len();
                    for id in to_remove {
                        events.remove(&id);
                    }
                    if count > 0 {
                        self.total_count.fetch_sub(count, Ordering::SeqCst);
                    }
                }
            }
            LogEntry::ClearPending { peer_bytes } => {
                if let Some((_, events)) = self.pending.remove(&peer_bytes) {
                    self.total_count.fetch_sub(events.len(), Ordering::SeqCst);
                }
            }
        }
    }

    /// Open the log file for writing
    async fn open_writer(&self) -> Result<(), StorageError> {
        let log_path = self.log_path();

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await?;

        let writer = BufWriter::new(file);
        *self.writer.write().await = Some(writer);

        debug!(path = ?log_path, "Opened log file for writing");
        Ok(())
    }

    /// Write a log entry to the append-only log
    async fn write_entry(&self, entry: &LogEntry) -> Result<(), StorageError> {
        let mut guard = self.writer.write().await;
        let writer = guard
            .as_mut()
            .ok_or_else(|| StorageError::Io("Log file not open".to_string()))?;

        let bytes =
            postcard::to_allocvec(entry).map_err(|e| StorageError::serialization(e.to_string()))?;

        // Write as base64-encoded line for text-based log
        let encoded = base64_encode(&bytes);
        writer.write_all(encoded.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        if self.sync_writes {
            writer.flush().await?;
        }

        Ok(())
    }

    /// Flush any buffered writes
    pub async fn flush(&self) -> Result<(), StorageError> {
        let mut guard = self.writer.write().await;
        if let Some(writer) = guard.as_mut() {
            writer.flush().await?;
        }
        Ok(())
    }

    /// Compact the log file by rewriting only the current state
    ///
    /// This reduces log file size by removing redundant entries.
    pub async fn compact(&self) -> Result<(), StorageError> {
        let log_path = self.log_path();
        let temp_path = self.storage_path.join("pending.log.tmp");

        info!("Compacting pending events log");

        // Write current state to temp file
        {
            let file = File::create(&temp_path).await?;
            let mut writer = BufWriter::new(file);

            for entry in self.pending.iter() {
                let peer_bytes = entry.key().clone();
                for event_id in entry.value().iter() {
                    let log_entry = LogEntry::MarkPending {
                        peer_bytes: peer_bytes.clone(),
                        event_id: *event_id,
                    };
                    let bytes = postcard::to_allocvec(&log_entry)
                        .map_err(|e| StorageError::serialization(e.to_string()))?;
                    let encoded = base64_encode(&bytes);
                    writer.write_all(encoded.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                }
            }
            writer.flush().await?;
        }

        // Close current writer
        *self.writer.write().await = None;

        // Atomic rename
        tokio::fs::rename(&temp_path, &log_path).await?;

        // Reopen writer
        self.open_writer().await?;

        info!(
            pending_count = self.total_count.load(Ordering::SeqCst),
            "Log compaction complete"
        );

        Ok(())
    }

    /// Get the total count of pending events
    pub fn total_pending(&self) -> usize {
        self.total_count.load(Ordering::SeqCst)
    }

    /// Get the number of peers with pending events
    pub fn peer_count(&self) -> usize {
        self.pending.len()
    }

    /// Check if there are any pending events
    pub fn is_empty(&self) -> bool {
        self.total_count.load(Ordering::SeqCst) == 0
    }

    /// Get a reference to the quota manager
    pub fn quota(&self) -> &QuotaManager {
        &self.quota
    }
}

#[async_trait]
impl<I: PeerIdentity> PendingStore<I> for PersistentPendingStore {
    async fn mark_pending(&self, peer: &I, event_id: EventId) -> Result<(), StorageError> {
        let peer_bytes = peer.as_bytes();
        trace!(peer = %peer, event = %event_id, "Marking event as pending (persistent)");

        // Check total quota
        let current_total = self.total_count.load(Ordering::SeqCst);
        if self.quota.would_exceed_total_quota(current_total) {
            return Err(StorageError::CapacityExceeded);
        }

        // Write to log first (for durability)
        let entry = LogEntry::MarkPending {
            peer_bytes: peer_bytes.clone(),
            event_id,
        };
        self.write_entry(&entry).await?;

        // Then apply to in-memory state
        let mut events = self.pending.entry(peer_bytes).or_default();

        // Check peer quota and apply eviction if needed
        if self.quota.would_exceed_peer_quota(events.len()) {
            let to_evict = self.quota.events_to_evict_for_peer(events.len(), 1);
            let evict_ids = self.quota.select_for_eviction(&events, to_evict);
            for id in evict_ids {
                if events.remove(&id) {
                    self.total_count.fetch_sub(1, Ordering::SeqCst);
                    // Note: We don't write eviction to log here as it will be
                    // handled during compaction or the evicted entries won't
                    // affect the final state
                }
            }
        }

        if events.insert(event_id) {
            self.total_count.fetch_add(1, Ordering::SeqCst);
        }

        Ok(())
    }

    async fn pending_for(&self, peer: &I) -> Result<Vec<EventId>, StorageError> {
        let key = peer.as_bytes();

        match self.pending.get(&key) {
            Some(events) => Ok(events.iter().copied().collect()),
            None => Ok(Vec::new()),
        }
    }

    async fn mark_delivered(&self, peer: &I, event_id: EventId) -> Result<(), StorageError> {
        let peer_bytes = peer.as_bytes();
        trace!(peer = %peer, event = %event_id, "Marking event as delivered (persistent)");

        // Write to log first
        let entry = LogEntry::MarkDelivered {
            peer_bytes: peer_bytes.clone(),
            event_id,
        };
        self.write_entry(&entry).await?;

        // Apply to in-memory state
        if let Some(mut events) = self.pending.get_mut(&peer_bytes)
            && events.remove(&event_id)
        {
            self.total_count.fetch_sub(1, Ordering::SeqCst);
        }

        Ok(())
    }

    async fn mark_delivered_up_to(&self, peer: &I, up_to: EventId) -> Result<(), StorageError> {
        let peer_bytes = peer.as_bytes();
        trace!(peer = %peer, up_to = %up_to, "Marking events as delivered up to (persistent)");

        // Write to log first
        let entry = LogEntry::MarkDeliveredUpTo {
            peer_bytes: peer_bytes.clone(),
            up_to,
        };
        self.write_entry(&entry).await?;

        // Apply to in-memory state
        if let Some(mut events) = self.pending.get_mut(&peer_bytes) {
            let to_remove: Vec<_> = events
                .iter()
                .filter(|id| id.sender_hash == up_to.sender_hash && id.sequence <= up_to.sequence)
                .copied()
                .collect();
            let count = to_remove.len();
            for id in to_remove {
                events.remove(&id);
            }
            if count > 0 {
                self.total_count.fetch_sub(count, Ordering::SeqCst);
            }
        }

        Ok(())
    }

    async fn clear_pending(&self, peer: &I) -> Result<(), StorageError> {
        let peer_bytes = peer.as_bytes();
        trace!(peer = %peer, "Clearing all pending events (persistent)");

        // Write to log first
        let entry = LogEntry::ClearPending {
            peer_bytes: peer_bytes.clone(),
        };
        self.write_entry(&entry).await?;

        // Apply to in-memory state
        if let Some((_, events)) = self.pending.remove(&peer_bytes) {
            self.total_count.fetch_sub(events.len(), Ordering::SeqCst);
        }

        Ok(())
    }
}

// Simple base64 encoding/decoding for the log format
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i] as usize;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as usize
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as usize
        } else {
            0
        };

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if i + 1 < data.len() {
            result.push(ALPHABET[((b1 & 0x0F) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    fn decode_char(c: char) -> Result<u8, &'static str> {
        match c {
            'A'..='Z' => Ok(c as u8 - b'A'),
            'a'..='z' => Ok(c as u8 - b'a' + 26),
            '0'..='9' => Ok(c as u8 - b'0' + 52),
            '+' => Ok(62),
            '/' => Ok(63),
            '=' => Ok(0),
            _ => Err("Invalid base64 character"),
        }
    }

    let input = input.trim();
    if !input.len().is_multiple_of(4) {
        return Err("Invalid base64 length");
    }

    let mut result = Vec::new();
    let chars: Vec<char> = input.chars().collect();

    for chunk in chars.chunks(4) {
        let b0 = decode_char(chunk[0])?;
        let b1 = decode_char(chunk[1])?;
        let b2 = decode_char(chunk[2])?;
        let b3 = decode_char(chunk[3])?;

        result.push((b0 << 2) | (b1 >> 4));

        if chunk[2] != '=' {
            result.push((b1 << 4) | (b2 >> 2));
        }
        if chunk[3] != '=' {
            result.push((b2 << 6) | b3);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;
    use std::env;

    async fn create_temp_store() -> (PersistentPendingStore, PathBuf) {
        let temp_dir = env::temp_dir().join(format!("indras_test_{}", rand::random::<u64>()));
        let store = PersistentPendingStore::new(&temp_dir).await.unwrap();
        (store, temp_dir)
    }

    async fn cleanup(path: PathBuf) {
        let _ = tokio::fs::remove_dir_all(path).await;
    }

    #[tokio::test]
    async fn test_persistent_store_basic_operations() {
        let (store, temp_dir) = create_temp_store().await;
        let peer = SimulationIdentity::new('A').unwrap();
        let event_id = EventId::new(1, 1);

        // Initially no pending
        let pending = store.pending_for(&peer).await.unwrap();
        assert!(pending.is_empty());

        // Mark pending
        store.mark_pending(&peer, event_id).await.unwrap();
        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 1);

        // Mark delivered
        store.mark_delivered(&peer, event_id).await.unwrap();
        let pending = store.pending_for(&peer).await.unwrap();
        assert!(pending.is_empty());

        cleanup(temp_dir).await;
    }

    #[tokio::test]
    async fn test_persistent_store_persistence() {
        let temp_dir = env::temp_dir().join(format!("indras_test_{}", rand::random::<u64>()));
        let peer = SimulationIdentity::new('A').unwrap();

        // First store instance
        {
            let store = PersistentPendingStore::new(&temp_dir).await.unwrap();
            store.mark_pending(&peer, EventId::new(1, 1)).await.unwrap();
            store.mark_pending(&peer, EventId::new(1, 2)).await.unwrap();
            store.mark_pending(&peer, EventId::new(1, 3)).await.unwrap();
            store.flush().await.unwrap();
        }

        // Second store instance - should reload data
        {
            let store = PersistentPendingStore::new(&temp_dir).await.unwrap();
            let pending = store.pending_for(&peer).await.unwrap();
            assert_eq!(pending.len(), 3);
        }

        cleanup(temp_dir).await;
    }

    #[tokio::test]
    async fn test_persistent_store_mark_delivered_up_to() {
        let (store, temp_dir) = create_temp_store().await;
        let peer = SimulationIdentity::new('A').unwrap();

        for i in 1..=5 {
            store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
        }

        store
            .mark_delivered_up_to(&peer, EventId::new(1, 3))
            .await
            .unwrap();

        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending.contains(&EventId::new(1, 4)));
        assert!(pending.contains(&EventId::new(1, 5)));

        cleanup(temp_dir).await;
    }

    #[tokio::test]
    async fn test_persistent_store_clear_pending() {
        let (store, temp_dir) = create_temp_store().await;
        let peer = SimulationIdentity::new('A').unwrap();

        for i in 1..=10 {
            store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
        }

        store.clear_pending(&peer).await.unwrap();
        assert!(store.pending_for(&peer).await.unwrap().is_empty());
        assert_eq!(store.total_pending(), 0);

        cleanup(temp_dir).await;
    }

    #[tokio::test]
    async fn test_persistent_store_compaction() {
        let temp_dir = env::temp_dir().join(format!("indras_test_{}", rand::random::<u64>()));
        let peer = SimulationIdentity::new('A').unwrap();

        {
            let store = PersistentPendingStore::new(&temp_dir).await.unwrap();

            // Add many events
            for i in 1..=100 {
                store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
            }

            // Deliver many of them
            for i in 1..=80 {
                store
                    .mark_delivered(&peer, EventId::new(1, i))
                    .await
                    .unwrap();
            }

            // Compact
            store.compact().await.unwrap();

            // Verify state is correct
            let pending = store.pending_for(&peer).await.unwrap();
            assert_eq!(pending.len(), 20);
        }

        // Verify after reload
        {
            let store = PersistentPendingStore::new(&temp_dir).await.unwrap();
            let pending = store.pending_for(&peer).await.unwrap();
            assert_eq!(pending.len(), 20);
        }

        cleanup(temp_dir).await;
    }

    #[test]
    fn test_base64_roundtrip() {
        let original = b"Hello, World!";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original.to_vec(), decoded);
    }

    #[test]
    fn test_base64_various_lengths() {
        for len in 0..=20 {
            let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let encoded = base64_encode(&data);
            let decoded = base64_decode(&encoded).unwrap();
            assert_eq!(data, decoded, "Failed for length {}", len);
        }
    }
}
