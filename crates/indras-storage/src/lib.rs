//! # Indras Storage
//!
//! Storage abstractions for Indras Network.
//!
//! This crate provides a tri-layer storage architecture:
//!
//! 1. **Append-only logs** - Immutable event history with ordering guarantees
//! 2. **Structured storage (redb)** - Queryable metadata, indices, sync state
//! 3. **Content-addressed blobs** - Large payloads, document snapshots
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    CompositeStorage                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │  EventLog (append-only)  │  redb (structured)  │  BlobStore │
//! │  - Event history         │  - Peer registry    │  - Blobs   │
//! │  - Ordering guarantees   │  - Membership index │  - Docs    │
//! │  - Replay/audit          │  - Sync state       │  - Large   │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Features
//!
//! - **PendingStore trait**: Abstraction for tracking pending event delivery
//! - **EventLog**: Append-only per-interface event logs
//! - **RedbStorage**: Fast key-value storage with range queries
//! - **BlobStore**: Content-addressed storage for large payloads
//! - **CompositeStorage**: Unified interface for all three layers
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_storage::{CompositeStorage, CompositeStorageConfig};
//! use indras_core::{SimulationIdentity, EventId, InterfaceId};
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = CompositeStorageConfig::with_base_dir("./data");
//!     let storage = CompositeStorage::<SimulationIdentity>::new(config).await.unwrap();
//!
//!     let interface_id = InterfaceId::generate();
//!     storage.create_interface(interface_id, Some("My Interface".to_string())).unwrap();
//!
//!     let event_id = EventId::new(1, 1);
//!     storage.append_event(&interface_id, event_id, Bytes::from("Hello!")).await.unwrap();
//! }
//! ```

pub mod error;
pub mod memory;
pub mod persistent;
pub mod quota;

// New tri-layer storage
pub mod append_log;
pub mod blobs;
pub mod composite;
pub mod structured;

// Re-exports
pub use error::StorageError;
pub use memory::{InMemoryPacketStore, InMemoryPendingStore};
pub use persistent::PersistentPendingStore;
pub use quota::{EvictionPolicy, QuotaManager, QuotaManagerBuilder};

// Tri-layer storage re-exports
pub use append_log::{CompactionConfig, EventLog, EventLogConfig, EventLogEntry};
pub use blobs::{BlobStore, BlobStoreConfig, ContentRef};
pub use composite::{CompositeStorage, CompositeStorageConfig};
pub use structured::{
    InterfaceRecord, InterfaceStore, PeerRecord, PeerRegistry, RedbStorage, RedbStorageConfig,
    SyncStateRecord, SyncStateStore,
};

// Re-export PacketStore trait from indras-core for convenience
pub use indras_core::PacketStore;

use async_trait::async_trait;
use indras_core::{EventId, PeerIdentity};

/// Trait for tracking pending event delivery in store-and-forward messaging
///
/// Implementations track which events are pending delivery to which peers.
/// This is used by the store-and-forward system to ensure messages are
/// eventually delivered to offline peers when they come back online.
///
/// The trait is generic over the peer identity type, allowing it to work
/// with both simulation identities and real cryptographic identities.
#[async_trait]
pub trait PendingStore<I: PeerIdentity>: Send + Sync {
    /// Track an event as pending for a peer (for store-and-forward)
    ///
    /// The event will be considered pending until explicitly marked as
    /// delivered or cleared.
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer to track the event for
    /// * `event_id` - The event ID to mark as pending
    ///
    /// # Errors
    ///
    /// Returns an error if storage capacity is exceeded or an I/O error occurs.
    async fn mark_pending(&self, peer: &I, event_id: EventId) -> Result<(), StorageError>;

    /// Get all pending event IDs for a peer
    ///
    /// Returns the list of event IDs that are pending delivery to the specified peer.
    /// The events are returned in order (by EventId's natural ordering).
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer to get pending events for
    ///
    /// # Returns
    ///
    /// A vector of pending event IDs, or an empty vector if none are pending.
    async fn pending_for(&self, peer: &I) -> Result<Vec<EventId>, StorageError>;

    /// Mark an event as delivered to a peer
    ///
    /// Removes the event from the pending set for the specified peer.
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer the event was delivered to
    /// * `event_id` - The event ID that was delivered
    async fn mark_delivered(&self, peer: &I, event_id: EventId) -> Result<(), StorageError>;

    /// Mark all events up to a given ID as delivered
    ///
    /// This is an optimization for acknowledging multiple events at once.
    /// All events from the same sender with sequence numbers <= the given
    /// event's sequence are marked as delivered.
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer the events were delivered to
    /// * `up_to` - The event ID to deliver up to (inclusive)
    async fn mark_delivered_up_to(&self, peer: &I, up_to: EventId) -> Result<(), StorageError>;

    /// Clear all pending events for a peer
    ///
    /// This is typically called when a peer leaves an interface or
    /// when all pending events should be discarded.
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer to clear pending events for
    async fn clear_pending(&self, peer: &I) -> Result<(), StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    /// Test that the PendingStore trait is object-safe
    fn _assert_object_safe(_: &dyn PendingStore<SimulationIdentity>) {}

    #[tokio::test]
    async fn test_in_memory_pending_store() {
        let store = InMemoryPendingStore::new();
        let peer = SimulationIdentity::new('A').unwrap();

        // Initially empty
        let pending = store.pending_for(&peer).await.unwrap();
        assert!(pending.is_empty());

        // Add some events
        for i in 1..=5 {
            store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
        }

        // Check pending
        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 5);

        // Mark one as delivered
        store
            .mark_delivered(&peer, EventId::new(1, 3))
            .await
            .unwrap();
        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 4);

        // Clear all
        store.clear_pending(&peer).await.unwrap();
        let pending = store.pending_for(&peer).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_quota_manager_integration() {
        let quota = QuotaManager::new(5, 100);
        let store = InMemoryPendingStore::with_quota(quota);
        let peer = SimulationIdentity::new('A').unwrap();

        // Add 6 events (exceeds per-peer quota of 5)
        for i in 1..=6 {
            store.mark_pending(&peer, EventId::new(1, i)).await.unwrap();
        }

        // Should only have 5 due to eviction
        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 5);

        // The oldest event (1) should have been evicted
        assert!(!pending.contains(&EventId::new(1, 1)));
        // The newest event (6) should be present
        assert!(pending.contains(&EventId::new(1, 6)));
    }
}
