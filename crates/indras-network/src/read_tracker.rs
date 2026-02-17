//! Read tracking for realms.
//!
//! Tracks the last-read position per member in a realm, enabling
//! unread count calculations for chat UIs.

use crate::document::DocumentSchema;
use crate::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// CRDT document for tracking read positions per member.
///
/// Each member's last-read sequence number is stored as a u64.
/// The sequence number corresponds to the event log position
/// at the time `mark_read()` was called.
///
/// # Example
///
/// ```ignore
/// let doc = realm.document::<ReadTrackerDocument>("read_tracker").await?;
///
/// // Mark current position as read
/// doc.update(|d| {
///     d.mark_read(my_id, current_seq);
/// }).await?;
///
/// // Check last-read position
/// let last = doc.read().await.last_read_seq(&my_id);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadTrackerDocument {
    /// Last-read sequence number per member.
    ///
    /// Key is the 32-byte member ID, value is the sequence number
    /// (event count) at the time of last read.
    pub last_read: HashMap<MemberId, u64>,
}

impl ReadTrackerDocument {
    /// Create a new empty read tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a position as read for a member.
    ///
    /// Only advances forward - if the current stored position is
    /// already beyond `seq`, this is a no-op (LWW semantics).
    pub fn mark_read(&mut self, member: MemberId, seq: u64) {
        let entry = self.last_read.entry(member).or_insert(0);
        if seq > *entry {
            *entry = seq;
        }
    }

    /// Get the last-read sequence number for a member.
    ///
    /// Returns 0 if the member has never marked anything as read.
    pub fn last_read_seq(&self, member: &MemberId) -> u64 {
        self.last_read.get(member).copied().unwrap_or(0)
    }

    /// Get all members who have read up to or past a given sequence number.
    pub fn readers_at(&self, seq: u64) -> Vec<MemberId> {
        self.last_read
            .iter()
            .filter(|&(_, &s)| s >= seq)
            .map(|(m, _)| *m)
            .collect()
    }
}

impl DocumentSchema for ReadTrackerDocument {
    /// Max-wins merge: for each member, keep the higher sequence number.
    fn merge(&mut self, remote: Self) {
        for (member, remote_seq) in remote.last_read {
            let entry = self.last_read.entry(member).or_insert(0);
            if remote_seq > *entry {
                *entry = remote_seq;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member_a() -> MemberId {
        [1u8; 32]
    }

    fn member_b() -> MemberId {
        [2u8; 32]
    }

    #[test]
    fn test_mark_read_advances() {
        let mut tracker = ReadTrackerDocument::new();

        tracker.mark_read(member_a(), 10);
        assert_eq!(tracker.last_read_seq(&member_a()), 10);

        // Advances forward
        tracker.mark_read(member_a(), 20);
        assert_eq!(tracker.last_read_seq(&member_a()), 20);

        // Does not go backward
        tracker.mark_read(member_a(), 5);
        assert_eq!(tracker.last_read_seq(&member_a()), 20);
    }

    #[test]
    fn test_default_is_zero() {
        let tracker = ReadTrackerDocument::new();
        assert_eq!(tracker.last_read_seq(&member_a()), 0);
    }

    #[test]
    fn test_independent_members() {
        let mut tracker = ReadTrackerDocument::new();

        tracker.mark_read(member_a(), 10);
        tracker.mark_read(member_b(), 5);

        assert_eq!(tracker.last_read_seq(&member_a()), 10);
        assert_eq!(tracker.last_read_seq(&member_b()), 5);
    }

    #[test]
    fn test_readers_at() {
        let mut tracker = ReadTrackerDocument::new();
        tracker.mark_read(member_a(), 10);
        tracker.mark_read(member_b(), 5);

        let readers = tracker.readers_at(5);
        assert_eq!(readers.len(), 2); // both >= 5

        let readers = tracker.readers_at(10);
        assert_eq!(readers.len(), 1); // only member_a >= 10

        let readers = tracker.readers_at(11);
        assert!(readers.is_empty()); // nobody >= 11
    }

    #[test]
    fn test_merge_max_wins() {
        let mut local = ReadTrackerDocument::new();
        local.mark_read(member_a(), 10);
        local.mark_read(member_b(), 20);

        let mut remote = ReadTrackerDocument::new();
        remote.mark_read(member_a(), 15); // remote is ahead
        remote.mark_read(member_b(), 5);  // local is ahead

        local.merge(remote);

        assert_eq!(local.last_read_seq(&member_a()), 15); // took remote
        assert_eq!(local.last_read_seq(&member_b()), 20); // kept local
    }
}
