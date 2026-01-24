//! In-memory storage implementations
//!
//! This module provides in-memory implementations of storage traits,
//! suitable for testing and simulation environments.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use dashmap::DashMap;
use indras_core::StorageError as CoreStorageError;
use indras_core::{EventId, Packet, PacketId, PacketStore, PeerIdentity};
use tracing::{debug, trace};

use crate::PendingStore;
use crate::error::StorageError;
use crate::quota::QuotaManager;

/// In-memory implementation of PendingStore
///
/// Uses `DashMap` for concurrent access to pending events per peer.
/// This implementation is suitable for testing and simulation.
#[derive(Debug)]
pub struct InMemoryPendingStore {
    /// Map from peer bytes to their pending event IDs
    pending: DashMap<Vec<u8>, BTreeSet<EventId>>,
    /// Quota manager for capacity limits
    quota: QuotaManager,
    /// Total count of pending events (across all peers)
    total_count: AtomicUsize,
}

impl Default for InMemoryPendingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryPendingStore {
    /// Create a new in-memory pending store
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
            quota: QuotaManager::default(),
            total_count: AtomicUsize::new(0),
        }
    }

    /// Create with a custom quota manager
    pub fn with_quota(quota: QuotaManager) -> Self {
        Self {
            pending: DashMap::new(),
            quota,
            total_count: AtomicUsize::new(0),
        }
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
impl<I: PeerIdentity> PendingStore<I> for InMemoryPendingStore {
    async fn mark_pending(&self, peer: &I, event_id: EventId) -> Result<(), StorageError> {
        let key = peer.as_bytes();
        trace!(peer = %peer, event = %event_id, "Marking event as pending");

        // Check total quota
        let current_total = self.total_count.load(Ordering::SeqCst);
        if self.quota.would_exceed_total_quota(current_total) {
            return Err(StorageError::CapacityExceeded);
        }

        let mut entry = self.pending.entry(key).or_default();
        let events = entry.value_mut();

        // Check peer quota
        if self.quota.would_exceed_peer_quota(events.len()) {
            // Apply eviction policy
            let to_evict = self.quota.events_to_evict_for_peer(events.len(), 1);
            let evict_ids = self.quota.select_for_eviction(events, to_evict);
            for id in evict_ids {
                events.remove(&id);
                self.total_count.fetch_sub(1, Ordering::SeqCst);
                debug!(peer = %peer, event = %id, "Evicted pending event due to peer quota");
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
        let key = peer.as_bytes();
        trace!(peer = %peer, event = %event_id, "Marking event as delivered");

        if let Some(mut entry) = self.pending.get_mut(&key)
            && entry.remove(&event_id)
        {
            self.total_count.fetch_sub(1, Ordering::SeqCst);
        }

        Ok(())
    }

    async fn mark_delivered_up_to(&self, peer: &I, up_to: EventId) -> Result<(), StorageError> {
        let key = peer.as_bytes();
        trace!(peer = %peer, up_to = %up_to, "Marking events as delivered up to");

        if let Some(mut entry) = self.pending.get_mut(&key) {
            let events = entry.value_mut();
            // Remove all events with ID <= up_to
            // Since EventId compares by (sender_hash, sequence), we need to check both
            // For simplicity, we mark all events from the same sender with sequence <= up_to.sequence
            let to_remove: Vec<_> = events
                .iter()
                .filter(|id| id.sender_hash == up_to.sender_hash && id.sequence <= up_to.sequence)
                .copied()
                .collect();

            let removed_count = to_remove.len();
            for id in to_remove {
                events.remove(&id);
            }

            if removed_count > 0 {
                self.total_count.fetch_sub(removed_count, Ordering::SeqCst);
            }
        }

        Ok(())
    }

    async fn clear_pending(&self, peer: &I) -> Result<(), StorageError> {
        let key = peer.as_bytes();
        trace!(peer = %peer, "Clearing all pending events");

        if let Some((_, events)) = self.pending.remove(&key) {
            self.total_count.fetch_sub(events.len(), Ordering::SeqCst);
        }

        Ok(())
    }
}

/// In-memory implementation of PacketStore
///
/// Uses `DashMap` for concurrent access to stored packets.
/// This implementation is suitable for testing and simulation.
#[derive(Debug)]
pub struct InMemoryPacketStore<I: PeerIdentity> {
    /// Map from packet ID to packet
    packets: DashMap<PacketId, Packet<I>>,
    /// Index: destination -> packet IDs for that destination
    by_destination: DashMap<Vec<u8>, BTreeSet<PacketId>>,
}

impl<I: PeerIdentity> Default for InMemoryPacketStore<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: PeerIdentity> InMemoryPacketStore<I> {
    /// Create a new in-memory packet store
    pub fn new() -> Self {
        Self {
            packets: DashMap::new(),
            by_destination: DashMap::new(),
        }
    }
}

#[async_trait]
impl<I: PeerIdentity> PacketStore<I> for InMemoryPacketStore<I> {
    async fn store(&self, packet: Packet<I>) -> Result<(), CoreStorageError> {
        let dest_key = packet.destination.as_bytes();
        let packet_id = packet.id;

        trace!(packet_id = %packet_id, destination = %packet.destination, "Storing packet");

        // Update destination index
        self.by_destination
            .entry(dest_key)
            .or_default()
            .insert(packet_id);

        // Store the packet
        self.packets.insert(packet_id, packet);

        Ok(())
    }

    async fn retrieve(&self, id: &PacketId) -> Result<Option<Packet<I>>, CoreStorageError> {
        Ok(self.packets.get(id).map(|p| p.clone()))
    }

    async fn pending_for(&self, destination: &I) -> Result<Vec<Packet<I>>, CoreStorageError> {
        let dest_key = destination.as_bytes();

        match self.by_destination.get(&dest_key) {
            Some(packet_ids) => {
                let packets: Vec<_> = packet_ids
                    .iter()
                    .filter_map(|id| self.packets.get(id).map(|p| p.clone()))
                    .collect();
                Ok(packets)
            }
            None => Ok(Vec::new()),
        }
    }

    async fn delete(&self, id: &PacketId) -> Result<(), CoreStorageError> {
        if let Some((_, packet)) = self.packets.remove(id) {
            // Also remove from destination index
            let dest_key = packet.destination.as_bytes();
            if let Some(mut ids) = self.by_destination.get_mut(&dest_key) {
                ids.remove(id);
            }
            trace!(packet_id = %id, "Deleted packet");
        }
        Ok(())
    }

    async fn all_packets(&self) -> Result<Vec<Packet<I>>, CoreStorageError> {
        Ok(self.packets.iter().map(|p| p.clone()).collect())
    }

    async fn count(&self) -> Result<usize, CoreStorageError> {
        Ok(self.packets.len())
    }

    async fn clear(&self) -> Result<(), CoreStorageError> {
        self.packets.clear();
        self.by_destination.clear();
        debug!("Cleared all packets from store");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::{EncryptedPayload, SimulationIdentity};

    // ========================================================================
    // InMemoryPendingStore Tests
    // ========================================================================

    #[tokio::test]
    async fn test_pending_store_basic_operations() {
        let store = InMemoryPendingStore::new();
        let peer = SimulationIdentity::new('A').unwrap();
        let event_id = EventId::new(1, 1);

        // Initially no pending events
        let pending = store.pending_for(&peer).await.unwrap();
        assert!(pending.is_empty());

        // Mark event as pending
        store.mark_pending(&peer, event_id).await.unwrap();
        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending.contains(&event_id));

        // Mark as delivered
        store.mark_delivered(&peer, event_id).await.unwrap();
        let pending = store.pending_for(&peer).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_pending_store_multiple_events() {
        let store = InMemoryPendingStore::new();
        let peer = SimulationIdentity::new('A').unwrap();

        let events: Vec<_> = (1..=5).map(|i| EventId::new(1, i)).collect();

        for event in &events {
            store.mark_pending(&peer, *event).await.unwrap();
        }

        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 5);
        assert_eq!(store.total_pending(), 5);
    }

    #[tokio::test]
    async fn test_pending_store_multiple_peers() {
        let store = InMemoryPendingStore::new();
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();

        store
            .mark_pending(&peer_a, EventId::new(1, 1))
            .await
            .unwrap();
        store
            .mark_pending(&peer_a, EventId::new(1, 2))
            .await
            .unwrap();
        store
            .mark_pending(&peer_b, EventId::new(2, 1))
            .await
            .unwrap();

        assert_eq!(store.pending_for(&peer_a).await.unwrap().len(), 2);
        assert_eq!(store.pending_for(&peer_b).await.unwrap().len(), 1);
        assert_eq!(store.total_pending(), 3);
        assert_eq!(store.peer_count(), 2);
    }

    #[tokio::test]
    async fn test_pending_store_mark_delivered_up_to() {
        let store = InMemoryPendingStore::new();
        let peer = SimulationIdentity::new('A').unwrap();

        // Add events 1..5 from sender_hash 1
        for i in 1..=5 {
            store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
        }

        // Mark delivered up to sequence 3
        store
            .mark_delivered_up_to(&peer, EventId::new(1, 3))
            .await
            .unwrap();

        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 2); // Only 4 and 5 remain
        assert!(pending.contains(&EventId::new(1, 4)));
        assert!(pending.contains(&EventId::new(1, 5)));
    }

    #[tokio::test]
    async fn test_pending_store_clear_pending() {
        let store = InMemoryPendingStore::new();
        let peer = SimulationIdentity::new('A').unwrap();

        for i in 1..=10 {
            store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
        }

        assert_eq!(store.total_pending(), 10);

        store.clear_pending(&peer).await.unwrap();

        assert_eq!(store.total_pending(), 0);
        assert!(store.pending_for(&peer).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_pending_store_quota_eviction() {
        let quota = QuotaManager::new(5, 100);
        let store = InMemoryPendingStore::with_quota(quota);
        let peer = SimulationIdentity::new('A').unwrap();

        // Add 5 events (at quota limit)
        for i in 1..=5 {
            store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
        }

        // Adding 6th should trigger eviction of oldest
        store.mark_pending(&peer, EventId::new(1, 6)).await.unwrap();

        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 5);
        // Event 1 should have been evicted
        assert!(!pending.contains(&EventId::new(1, 1)));
        // Event 6 should be present
        assert!(pending.contains(&EventId::new(1, 6)));
    }

    #[tokio::test]
    async fn test_pending_store_duplicate_event() {
        let store = InMemoryPendingStore::new();
        let peer = SimulationIdentity::new('A').unwrap();
        let event_id = EventId::new(1, 1);

        // Mark same event twice
        store.mark_pending(&peer, event_id).await.unwrap();
        store.mark_pending(&peer, event_id).await.unwrap();

        // Should only have one entry
        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(store.total_pending(), 1);
    }

    // ========================================================================
    // InMemoryPacketStore Tests
    // ========================================================================

    fn create_test_packet(
        source: SimulationIdentity,
        destination: SimulationIdentity,
        seq: u64,
    ) -> Packet<SimulationIdentity> {
        Packet::new(
            PacketId::new(source.as_char() as u64, seq),
            source,
            destination,
            EncryptedPayload::plaintext(b"test".to_vec()),
            vec![],
        )
    }

    #[tokio::test]
    async fn test_packet_store_basic_operations() {
        let store = InMemoryPacketStore::<SimulationIdentity>::new();
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('B').unwrap();

        let packet = create_test_packet(source, dest, 1);
        let packet_id = packet.id;

        // Store packet
        store.store(packet.clone()).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        // Retrieve packet
        let retrieved = store.retrieve(&packet_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, packet_id);

        // Delete packet
        store.delete(&packet_id).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
        assert!(store.retrieve(&packet_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_packet_store_pending_for_destination() {
        let store = InMemoryPacketStore::<SimulationIdentity>::new();
        let source = SimulationIdentity::new('A').unwrap();
        let dest_b = SimulationIdentity::new('B').unwrap();
        let dest_c = SimulationIdentity::new('C').unwrap();

        // Store packets for different destinations
        store
            .store(create_test_packet(source, dest_b, 1))
            .await
            .unwrap();
        store
            .store(create_test_packet(source, dest_b, 2))
            .await
            .unwrap();
        store
            .store(create_test_packet(source, dest_c, 3))
            .await
            .unwrap();

        // Check pending for B
        let pending_b = store.pending_for(&dest_b).await.unwrap();
        assert_eq!(pending_b.len(), 2);

        // Check pending for C
        let pending_c = store.pending_for(&dest_c).await.unwrap();
        assert_eq!(pending_c.len(), 1);
    }

    #[tokio::test]
    async fn test_packet_store_all_packets() {
        let store = InMemoryPacketStore::<SimulationIdentity>::new();
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('B').unwrap();

        for i in 1..=5 {
            store
                .store(create_test_packet(source, dest, i))
                .await
                .unwrap();
        }

        let all = store.all_packets().await.unwrap();
        assert_eq!(all.len(), 5);
    }

    #[tokio::test]
    async fn test_packet_store_clear() {
        let store = InMemoryPacketStore::<SimulationIdentity>::new();
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('B').unwrap();

        for i in 1..=5 {
            store
                .store(create_test_packet(source, dest, i))
                .await
                .unwrap();
        }

        store.clear().await.unwrap();

        assert_eq!(store.count().await.unwrap(), 0);
        assert!(store.all_packets().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_packet_store_delete_updates_destination_index() {
        let store = InMemoryPacketStore::<SimulationIdentity>::new();
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('B').unwrap();

        let packet = create_test_packet(source, dest, 1);
        let packet_id = packet.id;

        store.store(packet).await.unwrap();
        assert_eq!(store.pending_for(&dest).await.unwrap().len(), 1);

        store.delete(&packet_id).await.unwrap();
        assert!(store.pending_for(&dest).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_packet_store_retrieve_nonexistent() {
        let store = InMemoryPacketStore::<SimulationIdentity>::new();
        let packet_id = PacketId::new(999, 999);

        let result = store.retrieve(&packet_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_packet_store_delete_nonexistent() {
        let store = InMemoryPacketStore::<SimulationIdentity>::new();
        let packet_id = PacketId::new(999, 999);

        // Should not error
        store.delete(&packet_id).await.unwrap();
    }
}
