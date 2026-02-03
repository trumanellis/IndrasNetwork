//! Store-and-forward event storage
//!
//! The EventStore manages the append-only event log and tracks which events
//! have been delivered to which peers. Events are held for offline peers
//! until they come online and confirm receipt.

use std::collections::{HashMap, HashSet};

use indras_core::{EventId, InterfaceEvent, PeerIdentity};

/// Store-and-forward event storage
///
/// Manages:
/// - The append-only event log
/// - Pending events for each peer (events not yet confirmed delivered)
/// - Delivery tracking
pub struct EventStore<I: PeerIdentity> {
    /// All events in append order
    events: Vec<InterfaceEvent<I>>,
    /// Global sequence counter
    sequence: u64,
    /// Indices of events pending for each peer
    pending: HashMap<I, Vec<usize>>,
    /// Last delivered EventId per peer
    delivered: HashMap<I, EventId>,
    /// Current members (used to track new events)
    members: HashSet<I>,
    /// Hash of local peer identity (used in EventId to prevent collisions)
    local_peer_hash: u64,
}

impl<I: PeerIdentity> EventStore<I> {
    /// Create a new empty event store
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            sequence: 0,
            pending: HashMap::new(),
            delivered: HashMap::new(),
            members: HashSet::new(),
            local_peer_hash: 0,
        }
    }

    /// Create a new event store with a local peer hash for unique EventIds.
    pub fn with_peer_hash(peer_hash: u64) -> Self {
        Self {
            events: Vec::new(),
            sequence: 0,
            pending: HashMap::new(),
            delivered: HashMap::new(),
            members: HashSet::new(),
            local_peer_hash: peer_hash,
        }
    }

    /// Create an event store with initial members
    pub fn with_members(members: HashSet<I>) -> Self {
        let pending = members.iter().map(|p| (p.clone(), Vec::new())).collect();
        Self {
            events: Vec::new(),
            sequence: 0,
            pending,
            delivered: HashMap::new(),
            members,
            local_peer_hash: 0,
        }
    }

    /// Create an event store with initial members and a local peer hash.
    pub fn with_members_and_hash(members: HashSet<I>, peer_hash: u64) -> Self {
        let pending = members.iter().map(|p| (p.clone(), Vec::new())).collect();
        Self {
            events: Vec::new(),
            sequence: 0,
            pending,
            delivered: HashMap::new(),
            members,
            local_peer_hash: peer_hash,
        }
    }

    /// Update the member set
    pub fn set_members(&mut self, members: HashSet<I>) {
        // Add pending tracking for new members
        for member in &members {
            if !self.pending.contains_key(member) {
                // New member - they need ALL existing events
                let indices: Vec<usize> = (0..self.events.len()).collect();
                self.pending.insert(member.clone(), indices);
            }
        }

        // Remove tracking for removed members
        self.pending.retain(|k, _| members.contains(k));
        self.delivered.retain(|k, _| members.contains(k));

        self.members = members;
    }

    /// Add a member
    pub fn add_member(&mut self, peer: I) {
        if !self.members.contains(&peer) {
            // New member needs all existing events
            let indices: Vec<usize> = (0..self.events.len()).collect();
            self.pending.insert(peer.clone(), indices);
            self.members.insert(peer);
        }
    }

    /// Remove a member
    pub fn remove_member(&mut self, peer: &I) {
        self.members.remove(peer);
        self.pending.remove(peer);
        self.delivered.remove(peer);
    }

    /// Append an event and track for all members except the sender
    ///
    /// Returns the EventId of the appended event.
    pub fn append(&mut self, event: InterfaceEvent<I>) -> EventId {
        let sender = event.sender().cloned();
        let idx = self.events.len();

        // Get or create event ID
        let event_id = event.event_id().unwrap_or_else(|| {
            self.sequence += 1;
            EventId::new(self.local_peer_hash, self.sequence)
        });

        self.events.push(event);

        // Track this event as pending for all members except the sender
        for member in &self.members {
            // Skip the sender - they already have it
            if sender.as_ref() == Some(member) {
                continue;
            }

            self.pending.entry(member.clone()).or_default().push(idx);
        }

        event_id
    }

    /// Get pending events for a peer
    ///
    /// Returns events that haven't been confirmed delivered to this peer.
    pub fn pending_for(&self, peer: &I) -> Vec<&InterfaceEvent<I>> {
        self.pending
            .get(peer)
            .map(|indices| indices.iter().filter_map(|&i| self.events.get(i)).collect())
            .unwrap_or_default()
    }

    /// Get pending events as owned copies
    pub fn pending_for_owned(&self, peer: &I) -> Vec<InterfaceEvent<I>> {
        self.pending
            .get(peer)
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|&i| self.events.get(i).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get the count of pending events for a peer
    pub fn pending_count(&self, peer: &I) -> usize {
        self.pending.get(peer).map(|v| v.len()).unwrap_or(0)
    }

    /// Check if there are any pending events for any peer
    pub fn has_pending(&self) -> bool {
        self.pending.values().any(|v| !v.is_empty())
    }

    /// Mark events as delivered to a peer up to (and including) the given EventId
    ///
    /// Removes these events from the pending list for this peer.
    pub fn mark_delivered(&mut self, peer: &I, up_to: EventId) {
        if let Some(pending) = self.pending.get_mut(peer) {
            // Remove events that have been delivered
            pending.retain(|&idx| {
                if let Some(event) = self.events.get(idx)
                    && let Some(event_id) = event.event_id()
                {
                    // Keep events with sequence > up_to.sequence
                    // (same sender hash assumed for simplicity)
                    return event_id.sequence > up_to.sequence;
                }
                true // Keep events without IDs
            });
        }

        // Update delivered tracking
        self.delivered.insert(peer.clone(), up_to);
    }

    /// Mark all pending events as delivered to a peer
    pub fn mark_all_delivered(&mut self, peer: &I) {
        if let Some(pending) = self.pending.get_mut(peer) {
            pending.clear();
        }

        // Find the highest sequence in our events
        if let Some(max_seq) = self
            .events
            .iter()
            .filter_map(|e| e.event_id())
            .map(|id| id.sequence)
            .max()
        {
            self.delivered
                .insert(peer.clone(), EventId::new(0, max_seq));
        }
    }

    /// Remove events that have been delivered to all current members.
    ///
    /// This frees memory by pruning events that no peer still needs.
    /// Returns the number of pruned events.
    pub fn prune_delivered(&mut self) -> usize {
        if self.members.is_empty() || self.events.is_empty() {
            return 0;
        }

        // Find the minimum delivered sequence across all members
        let min_delivered = self.members.iter()
            .filter_map(|m| self.delivered.get(m))
            .map(|eid| eid.sequence)
            .min();

        let min_seq = match min_delivered {
            Some(seq) => seq,
            None => return 0, // No delivery tracking yet
        };

        // Count events to prune
        let before = self.events.len();

        // Find the split point: events with sequence <= min_seq can be removed
        let prune_count = self.events.iter()
            .take_while(|e| {
                e.event_id().map(|id| id.sequence <= min_seq).unwrap_or(false)
            })
            .count();

        if prune_count == 0 {
            return 0;
        }

        // Remove pruned events
        self.events.drain(..prune_count);

        // Reindex all pending lists (shift indices down by prune_count)
        for indices in self.pending.values_mut() {
            indices.retain(|&idx| idx >= prune_count);
            for idx in indices.iter_mut() {
                *idx -= prune_count;
            }
        }

        before - self.events.len()
    }

    /// Get all events since a global sequence number
    pub fn since(&self, seq: u64) -> Vec<&InterfaceEvent<I>> {
        self.events
            .iter()
            .filter(|e| e.event_id().map(|id| id.sequence > seq).unwrap_or(true))
            .collect()
    }

    /// Get all events since a global sequence number (owned)
    pub fn since_owned(&self, seq: u64) -> Vec<InterfaceEvent<I>> {
        self.events
            .iter()
            .filter(|e| e.event_id().map(|id| id.sequence > seq).unwrap_or(true))
            .cloned()
            .collect()
    }

    /// Get all events
    pub fn all(&self) -> &[InterfaceEvent<I>] {
        &self.events
    }

    /// Get the total number of events
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get the last delivered EventId for a peer
    pub fn last_delivered(&self, peer: &I) -> Option<EventId> {
        self.delivered.get(peer).copied()
    }

    /// Get the current global sequence
    pub fn current_sequence(&self) -> u64 {
        self.sequence
    }

    /// Set the local peer hash (used for EventId generation).
    pub fn set_peer_hash(&mut self, hash: u64) {
        self.local_peer_hash = hash;
    }

    /// Clear all events (for testing)
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.events.clear();
        self.pending.clear();
        self.delivered.clear();
        self.sequence = 0;
    }
}

impl<I: PeerIdentity> Default for EventStore<I> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    fn create_peers() -> (SimulationIdentity, SimulationIdentity, SimulationIdentity) {
        (
            SimulationIdentity::new('A').unwrap(),
            SimulationIdentity::new('B').unwrap(),
            SimulationIdentity::new('C').unwrap(),
        )
    }

    #[test]
    fn test_event_store_creation() {
        let store: EventStore<SimulationIdentity> = EventStore::new();
        assert!(store.is_empty());
        assert_eq!(store.current_sequence(), 0);
    }

    #[test]
    fn test_append_and_pending() {
        let (a, b, c) = create_peers();
        let mut members = HashSet::new();
        members.insert(a);
        members.insert(b);
        members.insert(c);

        let mut store = EventStore::with_members(members);

        // A sends a message
        let event = InterfaceEvent::message(a, 1, b"Hello from A".to_vec());
        store.append(event);

        // A shouldn't have it pending (they sent it)
        assert_eq!(store.pending_count(&a), 0);

        // B and C should have it pending
        assert_eq!(store.pending_count(&b), 1);
        assert_eq!(store.pending_count(&c), 1);

        // Verify the pending event
        let pending_b = store.pending_for(&b);
        assert_eq!(pending_b.len(), 1);
        match &pending_b[0] {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Hello from A");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_mark_delivered() {
        let (a, b, _c) = create_peers();
        let mut members = HashSet::new();
        members.insert(a);
        members.insert(b);

        let mut store = EventStore::with_members(members);

        // A sends multiple messages
        let event1 = InterfaceEvent::message(a, 1, b"Message 1".to_vec());
        let event2 = InterfaceEvent::message(a, 2, b"Message 2".to_vec());
        let event3 = InterfaceEvent::message(a, 3, b"Message 3".to_vec());

        let _id1 = store.append(event1);
        let id2 = store.append(event2);
        let _id3 = store.append(event3);

        assert_eq!(store.pending_count(&b), 3);

        // B confirms receipt up to message 2
        store.mark_delivered(&b, id2);

        // B should only have message 3 pending
        assert_eq!(store.pending_count(&b), 1);
        let pending = store.pending_for(&b);
        match &pending[0] {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Message 3");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_new_member_gets_all_events() {
        let (a, b, c) = create_peers();
        let mut members = HashSet::new();
        members.insert(a);
        members.insert(b);

        let mut store = EventStore::with_members(members);

        // A sends messages before C joins
        store.append(InterfaceEvent::message(a, 1, b"Before C".to_vec()));
        store.append(InterfaceEvent::message(b, 1, b"Also before C".to_vec()));

        // C joins
        store.add_member(c);

        // C should have all existing events pending
        assert_eq!(store.pending_count(&c), 2);
    }

    #[test]
    fn test_has_pending() {
        let (a, b, _c) = create_peers();
        let mut members = HashSet::new();
        members.insert(a);
        members.insert(b);

        let mut store = EventStore::with_members(members);

        assert!(!store.has_pending());

        store.append(InterfaceEvent::message(a, 1, b"Test".to_vec()));

        assert!(store.has_pending());

        store.mark_all_delivered(&b);

        assert!(!store.has_pending());
    }

    #[test]
    fn test_since() {
        let (a, _b, _c) = create_peers();
        let mut store: EventStore<SimulationIdentity> = EventStore::new();

        store.append(InterfaceEvent::message(a, 1, b"First".to_vec()));
        store.append(InterfaceEvent::message(a, 2, b"Second".to_vec()));
        store.append(InterfaceEvent::message(a, 3, b"Third".to_vec()));

        // Get events since sequence 1
        let events = store.since(1);
        assert_eq!(events.len(), 2); // Should get events 2 and 3
    }

    #[test]
    fn test_event_id_uses_peer_hash() {
        use indras_core::PresenceStatus;

        let (a, b, _) = create_peers();
        let mut members = HashSet::new();
        members.insert(a);
        members.insert(b);

        let mut store = EventStore::with_members_and_hash(members, 42);

        // Create a Presence event (which has no EventId, so store generates one)
        let event = InterfaceEvent::Presence {
            peer: a,
            status: PresenceStatus::Online,
            timestamp: chrono::Utc::now(),
        };
        let id = store.append(event);

        assert_eq!(id.sender_hash, 42);
        assert_eq!(id.sequence, 1);
    }

    #[test]
    fn test_prune_delivered() {
        let (a, b, _) = create_peers();
        let mut members = HashSet::new();
        members.insert(a);
        members.insert(b);

        let mut store = EventStore::with_members(members);

        let e1 = InterfaceEvent::message(a, 1, b"First".to_vec());
        let e2 = InterfaceEvent::message(a, 2, b"Second".to_vec());
        let e3 = InterfaceEvent::message(a, 3, b"Third".to_vec());

        let _id1 = store.append(e1);
        let id2 = store.append(e2);
        let _id3 = store.append(e3);

        assert_eq!(store.len(), 3);

        // Mark B delivered up to event 2
        store.mark_delivered(&b, id2);

        // Prune: B has received up to 2, A sent them all so only B matters
        // min_delivered across all members = 2 (only B has delivery tracking)
        // But A has no entry in delivered, so min won't include A
        let pruned = store.prune_delivered();
        // Since A has no delivered tracking, min will be just B's = 2
        // Events with sequence <= 2 are pruned (event 1 and 2)
        assert_eq!(pruned, 2);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_prune_delivered_no_tracking() {
        let (a, _b, _) = create_peers();
        let mut store: EventStore<SimulationIdentity> = EventStore::new();
        store.append(InterfaceEvent::message(a, 1, b"Test".to_vec()));

        // No delivery tracking, nothing to prune
        let pruned = store.prune_delivered();
        assert_eq!(pruned, 0);
    }

    #[test]
    fn test_with_peer_hash() {
        let store: EventStore<SimulationIdentity> = EventStore::with_peer_hash(99);
        assert_eq!(store.current_sequence(), 0);
        assert!(store.is_empty());
    }
}
