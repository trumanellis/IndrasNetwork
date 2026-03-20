//! Node log entry types

use serde::{Deserialize, Serialize};

use super::event::NodeEvent;

/// Monotonic sequence number for the node log
pub type NodeSequence = u64;

/// A single entry in the node-level event log
///
/// Each entry records a state-mutating action with a hash chain
/// linking it to the previous entry for tamper detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLogEntry {
    /// Monotonically increasing sequence number
    pub sequence: NodeSequence,
    /// Wall-clock timestamp (milliseconds since epoch)
    pub timestamp_millis: i64,
    /// The event that occurred
    pub event: NodeEvent,
    /// BLAKE3 hash of the previous entry's serialized bytes; `[0; 32]` for genesis
    pub prev_hash: [u8; 32],
}

/// Metadata about the node log, persisted in redb
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLogMeta {
    /// Current (next) sequence number
    pub current_sequence: NodeSequence,
    /// BLAKE3 hash of the last written entry
    pub last_hash: [u8; 32],
    /// Total number of entries written
    pub entry_count: u64,
    /// Current file size in bytes
    pub file_size: u64,
    /// Sequence of the last snapshot (for future compaction)
    pub last_snapshot_sequence: Option<NodeSequence>,
}
