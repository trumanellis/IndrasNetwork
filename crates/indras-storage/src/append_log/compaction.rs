//! Log compaction utilities
//!
//! Provides compaction for event logs, creating snapshots and truncating old entries.

use serde::{Deserialize, Serialize};

use super::event_log::BlobRef;

/// Configuration for log compaction
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Minimum number of entries before compaction
    pub min_entries: usize,
    /// Minimum log size (bytes) before compaction
    pub min_size: u64,
    /// Maximum age of entries to keep (milliseconds)
    pub max_age_millis: i64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_entries: 10000,
            min_size: 50 * 1024 * 1024, // 50MB
            max_age_millis: 7 * 24 * 60 * 60 * 1000, // 7 days
        }
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Number of entries compacted
    pub entries_compacted: usize,
    /// Bytes freed
    pub bytes_freed: u64,
    /// Reference to the snapshot blob (if created)
    pub snapshot_ref: Option<BlobRef>,
    /// New log start sequence
    pub new_start_sequence: u64,
    /// Timestamp of compaction
    pub compacted_at_millis: i64,
}

impl CompactionResult {
    /// Create a new compaction result
    pub fn new(
        entries_compacted: usize,
        bytes_freed: u64,
        snapshot_ref: Option<BlobRef>,
        new_start_sequence: u64,
    ) -> Self {
        Self {
            entries_compacted,
            bytes_freed,
            snapshot_ref,
            new_start_sequence,
            compacted_at_millis: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Snapshot metadata stored alongside the blob
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Interface ID
    pub interface_id: [u8; 32],
    /// Sequence number at snapshot time
    pub sequence: u64,
    /// Number of events included in snapshot
    pub event_count: usize,
    /// Reference to the snapshot blob
    pub blob_ref: BlobRef,
    /// When the snapshot was created
    pub created_at_millis: i64,
    /// The Automerge document heads at snapshot time
    pub document_heads: Vec<[u8; 32]>,
}

impl SnapshotMetadata {
    /// Create new snapshot metadata
    pub fn new(
        interface_id: [u8; 32],
        sequence: u64,
        event_count: usize,
        blob_ref: BlobRef,
        document_heads: Vec<[u8; 32]>,
    ) -> Self {
        Self {
            interface_id,
            sequence,
            event_count,
            blob_ref,
            created_at_millis: chrono::Utc::now().timestamp_millis(),
            document_heads,
        }
    }
}
