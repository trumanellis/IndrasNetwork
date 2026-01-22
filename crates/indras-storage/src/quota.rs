//! Quota management for storage
//!
//! This module provides quota management and eviction policies
//! for controlling storage capacity limits.

use std::collections::BTreeSet;

use indras_core::EventId;

/// Eviction policy for when storage limits are reached
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EvictionPolicy {
    /// First-in-first-out: evict oldest items first (default)
    #[default]
    Fifo,
    /// Evict items with the oldest EventId (by sender_hash then sequence)
    OldestFirst,
}

/// Manages storage quotas and eviction
#[derive(Debug, Clone)]
pub struct QuotaManager {
    /// Maximum number of pending events per peer
    max_pending_per_peer: usize,
    /// Maximum total pending events across all peers
    max_total_pending: usize,
    /// Eviction policy when limits are reached
    eviction_policy: EvictionPolicy,
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self {
            max_pending_per_peer: 1000,
            max_total_pending: 100_000,
            eviction_policy: EvictionPolicy::default(),
        }
    }
}

impl QuotaManager {
    /// Create a new QuotaManager with specified limits
    pub fn new(max_pending_per_peer: usize, max_total_pending: usize) -> Self {
        Self {
            max_pending_per_peer,
            max_total_pending,
            eviction_policy: EvictionPolicy::default(),
        }
    }

    /// Create a QuotaManager with custom eviction policy
    pub fn with_eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.eviction_policy = policy;
        self
    }

    /// Get the maximum pending events per peer
    pub fn max_pending_per_peer(&self) -> usize {
        self.max_pending_per_peer
    }

    /// Get the maximum total pending events
    pub fn max_total_pending(&self) -> usize {
        self.max_total_pending
    }

    /// Get the eviction policy
    pub fn eviction_policy(&self) -> EvictionPolicy {
        self.eviction_policy
    }

    /// Check if adding an event would exceed peer quota
    pub fn would_exceed_peer_quota(&self, current_count: usize) -> bool {
        current_count >= self.max_pending_per_peer
    }

    /// Check if adding an event would exceed total quota
    pub fn would_exceed_total_quota(&self, current_total: usize) -> bool {
        current_total >= self.max_total_pending
    }

    /// Apply eviction policy to a set of events, returning items to evict
    ///
    /// Returns the EventIds that should be evicted to make room for new items.
    pub fn select_for_eviction(&self, events: &BTreeSet<EventId>, count: usize) -> Vec<EventId> {
        if count == 0 || events.is_empty() {
            return Vec::new();
        }

        let to_evict = count.min(events.len());

        match self.eviction_policy {
            EvictionPolicy::Fifo | EvictionPolicy::OldestFirst => {
                // BTreeSet is already ordered, so take from the beginning
                // EventId orders by (sender_hash, sequence) which approximates age
                events.iter().take(to_evict).copied().collect()
            }
        }
    }

    /// Calculate how many events need to be evicted to fit within peer quota
    pub fn events_to_evict_for_peer(&self, current_count: usize, to_add: usize) -> usize {
        let new_total = current_count.saturating_add(to_add);
        new_total.saturating_sub(self.max_pending_per_peer)
    }

    /// Calculate how many events need to be evicted to fit within total quota
    pub fn events_to_evict_for_total(&self, current_total: usize, to_add: usize) -> usize {
        let new_total = current_total.saturating_add(to_add);
        new_total.saturating_sub(self.max_total_pending)
    }
}

/// Builder for QuotaManager
#[derive(Debug, Default)]
pub struct QuotaManagerBuilder {
    max_pending_per_peer: Option<usize>,
    max_total_pending: Option<usize>,
    eviction_policy: Option<EvictionPolicy>,
}

impl QuotaManagerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum pending events per peer
    pub fn max_pending_per_peer(mut self, max: usize) -> Self {
        self.max_pending_per_peer = Some(max);
        self
    }

    /// Set maximum total pending events
    pub fn max_total_pending(mut self, max: usize) -> Self {
        self.max_total_pending = Some(max);
        self
    }

    /// Set eviction policy
    pub fn eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.eviction_policy = Some(policy);
        self
    }

    /// Build the QuotaManager
    pub fn build(self) -> QuotaManager {
        let mut manager = QuotaManager::default();

        if let Some(max) = self.max_pending_per_peer {
            manager.max_pending_per_peer = max;
        }
        if let Some(max) = self.max_total_pending {
            manager.max_total_pending = max;
        }
        if let Some(policy) = self.eviction_policy {
            manager.eviction_policy = policy;
        }

        manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_quota_manager() {
        let manager = QuotaManager::default();
        assert_eq!(manager.max_pending_per_peer(), 1000);
        assert_eq!(manager.max_total_pending(), 100_000);
        assert_eq!(manager.eviction_policy(), EvictionPolicy::Fifo);
    }

    #[test]
    fn test_custom_quota_manager() {
        let manager = QuotaManager::new(500, 50_000)
            .with_eviction_policy(EvictionPolicy::OldestFirst);

        assert_eq!(manager.max_pending_per_peer(), 500);
        assert_eq!(manager.max_total_pending(), 50_000);
        assert_eq!(manager.eviction_policy(), EvictionPolicy::OldestFirst);
    }

    #[test]
    fn test_would_exceed_peer_quota() {
        let manager = QuotaManager::new(10, 100);

        assert!(!manager.would_exceed_peer_quota(5));
        assert!(!manager.would_exceed_peer_quota(9));
        assert!(manager.would_exceed_peer_quota(10));
        assert!(manager.would_exceed_peer_quota(15));
    }

    #[test]
    fn test_would_exceed_total_quota() {
        let manager = QuotaManager::new(10, 100);

        assert!(!manager.would_exceed_total_quota(50));
        assert!(!manager.would_exceed_total_quota(99));
        assert!(manager.would_exceed_total_quota(100));
        assert!(manager.would_exceed_total_quota(150));
    }

    #[test]
    fn test_select_for_eviction() {
        let manager = QuotaManager::default();

        let mut events = BTreeSet::new();
        events.insert(EventId::new(1, 1));
        events.insert(EventId::new(1, 2));
        events.insert(EventId::new(1, 3));
        events.insert(EventId::new(2, 1));
        events.insert(EventId::new(2, 2));

        // Evict 2 items
        let to_evict = manager.select_for_eviction(&events, 2);
        assert_eq!(to_evict.len(), 2);
        // Should evict the "oldest" by BTreeSet ordering
        assert!(to_evict.contains(&EventId::new(1, 1)));
        assert!(to_evict.contains(&EventId::new(1, 2)));
    }

    #[test]
    fn test_select_for_eviction_empty() {
        let manager = QuotaManager::default();
        let events = BTreeSet::new();

        let to_evict = manager.select_for_eviction(&events, 5);
        assert!(to_evict.is_empty());
    }

    #[test]
    fn test_select_for_eviction_more_than_available() {
        let manager = QuotaManager::default();

        let mut events = BTreeSet::new();
        events.insert(EventId::new(1, 1));
        events.insert(EventId::new(1, 2));

        let to_evict = manager.select_for_eviction(&events, 10);
        assert_eq!(to_evict.len(), 2); // Only evict what's available
    }

    #[test]
    fn test_events_to_evict_for_peer() {
        let manager = QuotaManager::new(10, 100);

        // Current 8, adding 1 -> 9, no eviction needed
        assert_eq!(manager.events_to_evict_for_peer(8, 1), 0);

        // Current 8, adding 2 -> 10, no eviction needed
        assert_eq!(manager.events_to_evict_for_peer(8, 2), 0);

        // Current 8, adding 3 -> 11, evict 1
        assert_eq!(manager.events_to_evict_for_peer(8, 3), 1);

        // Current 10, adding 5 -> 15, evict 5
        assert_eq!(manager.events_to_evict_for_peer(10, 5), 5);
    }

    #[test]
    fn test_events_to_evict_for_total() {
        let manager = QuotaManager::new(10, 100);

        // Current 95, adding 5 -> 100, no eviction needed
        assert_eq!(manager.events_to_evict_for_total(95, 5), 0);

        // Current 95, adding 10 -> 105, evict 5
        assert_eq!(manager.events_to_evict_for_total(95, 10), 5);
    }

    #[test]
    fn test_builder() {
        let manager = QuotaManagerBuilder::new()
            .max_pending_per_peer(200)
            .max_total_pending(5000)
            .eviction_policy(EvictionPolicy::OldestFirst)
            .build();

        assert_eq!(manager.max_pending_per_peer(), 200);
        assert_eq!(manager.max_total_pending(), 5000);
        assert_eq!(manager.eviction_policy(), EvictionPolicy::OldestFirst);
    }

    #[test]
    fn test_builder_partial() {
        // Only set some values, others should use defaults
        let manager = QuotaManagerBuilder::new()
            .max_pending_per_peer(500)
            .build();

        assert_eq!(manager.max_pending_per_peer(), 500);
        assert_eq!(manager.max_total_pending(), 100_000); // Default
        assert_eq!(manager.eviction_policy(), EvictionPolicy::Fifo); // Default
    }
}
