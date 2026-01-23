//! Append-only event log storage
//!
//! This module provides an immutable, append-only log for interface events.
//! Events are written sequentially and can only be appended, never modified.
//!
//! ## Features
//!
//! - Sequential append-only writes
//! - Efficient range queries (events since a sequence number)
//! - Compaction via snapshots (compact old events into a snapshot blob)
//! - Recovery through replay
//!
//! ## Storage Format
//!
//! Each log file contains length-prefixed, postcard-serialized events:
//! ```text
//! [4 bytes: len][len bytes: serialized event][4 bytes: len][...]
//! ```

pub mod event_log;
mod compaction;

pub use event_log::{EventLog, EventLogConfig, EventLogEntry, BlobRef};
pub use compaction::{CompactionConfig, CompactionResult};
