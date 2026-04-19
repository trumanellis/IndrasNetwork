//! Staged blob deletion with a grace period.
//!
//! Rather than delete unreferenced blobs immediately on GC, the braid
//! stages each candidate [`ContentAddr`] with a timestamp. If the blob
//! gets re-referenced during the grace period (e.g. a delayed peer sync
//! restores a changeset that names it), the staging is cancelled. Only
//! addresses whose grace period has fully elapsed are actually deleted.
//!
//! This is a pure in-memory data structure — persistence (survive
//! restarts) is a separate concern, layered on top when needed.

use std::collections::HashMap;

use crate::content_addr::ContentAddr;

/// One blob's pending deletion state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedDeletion {
    /// Address of the blob scheduled for deletion.
    pub addr: ContentAddr,
    /// Wall-clock time (Unix millis) the blob entered the staging list.
    pub staged_at_millis: i64,
    /// Grace period (ms) before this entry is eligible for actual deletion.
    pub grace_period_ms: i64,
}

impl StagedDeletion {
    /// Whether this entry is ready to delete given the current time.
    pub fn is_ready(&self, now_millis: i64) -> bool {
        now_millis.saturating_sub(self.staged_at_millis) >= self.grace_period_ms
    }
}

/// Default grace period before a staged blob becomes eligible for
/// deletion: seven days, in milliseconds.
pub const DEFAULT_GRACE_PERIOD_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// Set of blobs pending deletion, keyed by [`ContentAddr`].
///
/// Use [`stage`](Self::stage) to schedule a deletion, [`unstage`](Self::unstage)
/// if the blob becomes referenced again, and
/// [`take_ready_for_deletion`](Self::take_ready_for_deletion) to pull
/// every entry whose grace period has elapsed.
#[derive(Debug, Clone, Default)]
pub struct StagedDeletionSet {
    entries: HashMap<ContentAddr, StagedDeletion>,
}

impl StagedDeletionSet {
    /// Empty staging list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage `addr` for deletion at `now_millis` with `grace_period_ms`.
    ///
    /// If `addr` is already staged, its existing `staged_at_millis` is
    /// retained — re-staging does not extend the grace period.
    pub fn stage(
        &mut self,
        addr: ContentAddr,
        now_millis: i64,
        grace_period_ms: i64,
    ) {
        self.entries.entry(addr).or_insert(StagedDeletion {
            addr,
            staged_at_millis: now_millis,
            grace_period_ms,
        });
    }

    /// Remove `addr` from the staging list if present (e.g. because the
    /// blob became referenced again). Returns the previous entry.
    pub fn unstage(&mut self, addr: &ContentAddr) -> Option<StagedDeletion> {
        self.entries.remove(addr)
    }

    /// Whether `addr` is currently staged.
    pub fn contains(&self, addr: &ContentAddr) -> bool {
        self.entries.contains_key(addr)
    }

    /// Number of staged entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the staging list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove and return every entry whose grace period has elapsed at
    /// `now_millis`. Entries still within their grace window stay staged.
    pub fn take_ready_for_deletion(&mut self, now_millis: i64) -> Vec<StagedDeletion> {
        let ready: Vec<ContentAddr> = self
            .entries
            .iter()
            .filter_map(|(addr, entry)| entry.is_ready(now_millis).then_some(*addr))
            .collect();
        ready
            .into_iter()
            .filter_map(|addr| self.entries.remove(&addr))
            .collect()
    }

    /// Iterator over all currently-staged entries.
    pub fn iter(&self) -> impl Iterator<Item = &StagedDeletion> {
        self.entries.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> ContentAddr {
        ContentAddr::new([byte; 32], byte as u64)
    }

    #[test]
    fn stage_records_addr() {
        let mut set = StagedDeletionSet::new();
        set.stage(addr(1), 1000, DEFAULT_GRACE_PERIOD_MS);
        assert!(set.contains(&addr(1)));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn unstage_removes_addr() {
        let mut set = StagedDeletionSet::new();
        set.stage(addr(1), 1000, DEFAULT_GRACE_PERIOD_MS);
        let removed = set.unstage(&addr(1)).expect("entry was staged");
        assert_eq!(removed.addr, addr(1));
        assert!(!set.contains(&addr(1)));
    }

    #[test]
    fn restaging_does_not_extend_grace_period() {
        let mut set = StagedDeletionSet::new();
        set.stage(addr(1), 1_000, 5_000);
        set.stage(addr(1), 2_000, 5_000); // second stage ignored
        let entry = set.iter().next().unwrap();
        assert_eq!(entry.staged_at_millis, 1_000);
    }

    #[test]
    fn take_ready_respects_grace_period() {
        let mut set = StagedDeletionSet::new();
        set.stage(addr(1), 1_000, 5_000); // ready at 6_000
        set.stage(addr(2), 2_000, 5_000); // ready at 7_000

        // At t=6_500: only addr(1) is ready.
        let ready = set.take_ready_for_deletion(6_500);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].addr, addr(1));
        assert!(set.contains(&addr(2)), "addr(2) still within grace");

        // At t=7_500: addr(2) now ready.
        let ready = set.take_ready_for_deletion(7_500);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].addr, addr(2));
        assert!(set.is_empty());
    }

    #[test]
    fn is_ready_uses_elapsed_time() {
        let entry = StagedDeletion {
            addr: addr(1),
            staged_at_millis: 1_000,
            grace_period_ms: 500,
        };
        assert!(!entry.is_ready(1_400));
        assert!(entry.is_ready(1_500));
        assert!(entry.is_ready(10_000));
    }

    #[test]
    fn take_ready_on_empty_set_is_noop() {
        let mut set = StagedDeletionSet::new();
        let ready = set.take_ready_for_deletion(1_000_000);
        assert!(ready.is_empty());
    }
}
