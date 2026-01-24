//! Composite storage combining all three storage layers
//!
//! This module provides [`CompositeStorage`], which unifies:
//! - Append-only event logs
//! - Structured storage (redb)
//! - Content-addressed blobs
//!
//! ## Storage Flow
//!
//! ```text
//! Event arrives → CompositeStorage.append_event()
//!   ├─ If payload > 4KB: store in BlobStore, get ContentRef
//!   ├─ Append to EventLog (with ContentRef if applicable)
//!   └─ Update redb indices (event_index, pending_delivery)
//!
//! Sync with peer → CompositeStorage.events_since()
//!   ├─ Query redb for event IDs since their heads
//!   ├─ Read events from EventLog
//!   └─ Resolve ContentRefs from BlobStore if needed
//!
//! Bootstrap new peer → CompositeStorage.load_interface()
//!   ├─ Load latest snapshot from BlobStore
//!   ├─ Replay recent events from EventLog since snapshot
//!   └─ Build in-memory state
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use tracing::{debug, info, instrument};

use indras_core::{EventId, InterfaceId, PeerIdentity};

use crate::append_log::{EventLog, EventLogConfig, EventLogEntry};
use crate::blobs::{BlobStore, BlobStoreConfig, ContentRef};
use crate::error::StorageError;
use crate::structured::{
    InterfaceRecord, InterfaceStore, MembershipRecord, PeerRecord, PeerRegistry, RedbStorage,
    RedbStorageConfig, SyncStateStore,
};

/// Configuration for composite storage
#[derive(Debug, Clone)]
pub struct CompositeStorageConfig {
    /// Base directory for all storage
    pub base_dir: PathBuf,
    /// Event log configuration
    pub event_log: EventLogConfig,
    /// redb configuration
    pub redb: RedbStorageConfig,
    /// Blob store configuration
    pub blobs: BlobStoreConfig,
    /// Threshold for storing payloads in blobs
    pub blob_threshold: usize,
}

impl Default for CompositeStorageConfig {
    fn default() -> Self {
        let base_dir = PathBuf::from("./data");
        Self {
            base_dir: base_dir.clone(),
            event_log: EventLogConfig {
                base_dir: base_dir.join("logs"),
                ..Default::default()
            },
            redb: RedbStorageConfig {
                db_path: base_dir.join("indras.redb"),
                ..Default::default()
            },
            blobs: BlobStoreConfig {
                base_dir: base_dir.join("blobs"),
                ..Default::default()
            },
            blob_threshold: 4096, // 4KB
        }
    }
}

impl CompositeStorageConfig {
    /// Create a configuration with a custom base directory
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        Self {
            base_dir: base_dir.clone(),
            event_log: EventLogConfig {
                base_dir: base_dir.join("logs"),
                ..Default::default()
            },
            redb: RedbStorageConfig {
                db_path: base_dir.join("indras.redb"),
                ..Default::default()
            },
            blobs: BlobStoreConfig {
                base_dir: base_dir.join("blobs"),
                ..Default::default()
            },
            blob_threshold: 4096,
        }
    }
}

/// Composite storage unifying all three storage layers
pub struct CompositeStorage<I: PeerIdentity> {
    /// Per-interface event logs
    event_logs: DashMap<InterfaceId, Arc<EventLog<I>>>,
    /// Structured storage
    redb: Arc<RedbStorage>,
    /// Peer registry
    peer_registry: PeerRegistry,
    /// Interface store
    interface_store: InterfaceStore,
    /// Sync state store
    sync_state: SyncStateStore,
    /// Blob storage
    blobs: Arc<BlobStore>,
    /// Configuration
    config: CompositeStorageConfig,
}

impl<I: PeerIdentity> CompositeStorage<I> {
    /// Create a new composite storage
    #[instrument(skip(config), fields(base_dir = %config.base_dir.display()))]
    pub async fn new(config: CompositeStorageConfig) -> Result<Self, StorageError> {
        // Ensure base directory exists
        tokio::fs::create_dir_all(&config.base_dir)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Open redb
        let redb = Arc::new(RedbStorage::open(config.redb.clone())?);

        // Create stores
        let peer_registry = PeerRegistry::new(redb.clone());
        let interface_store = InterfaceStore::new(redb.clone());
        let sync_state = SyncStateStore::new(redb.clone());

        // Open blob store
        let blobs = Arc::new(BlobStore::new(config.blobs.clone()).await?);

        info!("Composite storage initialized");

        Ok(Self {
            event_logs: DashMap::new(),
            redb,
            peer_registry,
            interface_store,
            sync_state,
            blobs,
            config,
        })
    }

    /// Get or create an event log for an interface
    pub async fn event_log(
        &self,
        interface_id: InterfaceId,
    ) -> Result<Arc<EventLog<I>>, StorageError> {
        if let Some(log) = self.event_logs.get(&interface_id) {
            return Ok(log.clone());
        }

        let log = EventLog::new(interface_id, self.config.event_log.clone()).await?;
        let log = Arc::new(log);
        self.event_logs.insert(interface_id, log.clone());
        Ok(log)
    }

    /// Append an event to an interface's log
    ///
    /// Large payloads are automatically stored in the blob store.
    #[instrument(skip(self, payload), fields(interface_id = %hex::encode(interface_id.as_bytes())))]
    pub async fn append_event(
        &self,
        interface_id: &InterfaceId,
        event_id: EventId,
        payload: Bytes,
    ) -> Result<u64, StorageError> {
        let log = self.event_log(*interface_id).await?;

        // Check if payload should be stored as blob
        let sequence = if payload.len() >= self.config.blob_threshold {
            let content_ref = self.blobs.store(&payload).await?;
            let blob_ref =
                crate::append_log::event_log::BlobRef::new(content_ref.hash, content_ref.size);
            log.append_with_blob(event_id, blob_ref).await?
        } else {
            log.append(event_id, payload).await?
        };

        // Update interface event count
        let _ = self.interface_store.increment_events(interface_id);

        debug!(sequence = sequence, "Appended event");
        Ok(sequence)
    }

    /// Get an event by ID
    pub async fn get_event(
        &self,
        interface_id: &InterfaceId,
        event_id: EventId,
    ) -> Result<Option<EventLogEntry<I>>, StorageError> {
        let log = self.event_log(*interface_id).await?;
        log.read_event(event_id).await
    }

    /// Get events since a sequence number
    pub async fn events_since(
        &self,
        interface_id: &InterfaceId,
        since: u64,
    ) -> Result<Vec<EventLogEntry<I>>, StorageError> {
        let log = self.event_log(*interface_id).await?;
        log.read_since(since).await
    }

    /// Resolve a blob reference to its content
    pub async fn resolve_blob(&self, content_ref: &ContentRef) -> Result<Bytes, StorageError> {
        self.blobs.load(content_ref).await
    }

    /// Store content in the blob store
    pub async fn store_blob(&self, data: &[u8]) -> Result<ContentRef, StorageError> {
        self.blobs.store(data).await
    }

    /// Get the peer registry
    pub fn peer_registry(&self) -> &PeerRegistry {
        &self.peer_registry
    }

    /// Get the interface store
    pub fn interface_store(&self) -> &InterfaceStore {
        &self.interface_store
    }

    /// Get the sync state store
    pub fn sync_state(&self) -> &SyncStateStore {
        &self.sync_state
    }

    /// Get the blob store
    pub fn blob_store(&self) -> &BlobStore {
        &self.blobs
    }

    /// Create a new interface
    pub fn create_interface(
        &self,
        interface_id: InterfaceId,
        name: Option<String>,
    ) -> Result<(), StorageError> {
        let mut record = InterfaceRecord::new(interface_id);
        if let Some(n) = name {
            record = record.with_name(n);
        }
        self.interface_store.upsert(&record)
    }

    /// Register a peer
    pub fn register_peer(&self, peer: &I, name: Option<String>) -> Result<(), StorageError> {
        let mut record = PeerRecord::new(peer.as_bytes());
        if let Some(n) = name {
            record = record.with_name(n);
        }
        self.peer_registry.upsert(peer, &record)
    }

    /// Add a member to an interface
    pub fn add_member(&self, interface_id: &InterfaceId, peer: &I) -> Result<(), StorageError> {
        let membership = MembershipRecord::new(peer.as_bytes());
        self.interface_store
            .add_member(interface_id, peer, &membership)
    }

    /// Update sync state for a peer/interface
    pub fn update_sync_state(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        heads: Vec<[u8; 32]>,
    ) -> Result<(), StorageError> {
        self.sync_state.update_heads(peer, interface_id, heads)
    }

    /// Acknowledge events from a peer
    pub fn acknowledge_events(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        up_to: EventId,
    ) -> Result<(), StorageError> {
        self.sync_state
            .acknowledge_events(peer, interface_id, up_to)
    }

    /// Queue an event for delivery to a peer
    pub fn queue_for_delivery(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        event_id: EventId,
    ) -> Result<(), StorageError> {
        self.sync_state.add_pending(peer, interface_id, event_id)
    }

    /// Get pending events for a peer
    pub fn pending_for(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
    ) -> Result<Vec<EventId>, StorageError> {
        let pending = self.sync_state.get_pending(peer, interface_id)?;
        Ok(pending.into_iter().map(|p| p.event_id).collect())
    }

    /// Compact the database
    pub fn compact(&self) -> Result<(), StorageError> {
        self.redb.compact()
    }

    /// Close all storage
    pub async fn close(&self) -> Result<(), StorageError> {
        // Close all event logs
        for entry in self.event_logs.iter() {
            entry.close().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;
    use tempfile::TempDir;

    async fn create_test_storage() -> (CompositeStorage<SimulationIdentity>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = CompositeStorageConfig::with_base_dir(temp_dir.path());
        let storage = CompositeStorage::new(config).await.unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_append_and_read_events() {
        let (storage, _temp) = create_test_storage().await;
        let interface_id = InterfaceId::new([0x42; 32]);

        // Create interface
        storage
            .create_interface(interface_id, Some("Test".to_string()))
            .unwrap();

        // Append events
        for i in 1..=5 {
            let event_id = EventId::new(1, i);
            storage
                .append_event(&interface_id, event_id, Bytes::from(format!("data {}", i)))
                .await
                .unwrap();
        }

        // Read events
        let events = storage.events_since(&interface_id, 0).await.unwrap();
        assert_eq!(events.len(), 5);
    }

    #[tokio::test]
    async fn test_large_payload_stored_as_blob() {
        let (storage, _temp) = create_test_storage().await;
        let interface_id = InterfaceId::new([0xAB; 32]);

        storage.create_interface(interface_id, None).unwrap();

        // Create a large payload (> 4KB)
        let large_data = vec![0xAB; 10000];
        let event_id = EventId::new(1, 1);

        storage
            .append_event(&interface_id, event_id, Bytes::from(large_data.clone()))
            .await
            .unwrap();

        // Read the event
        let event = storage
            .get_event(&interface_id, event_id)
            .await
            .unwrap()
            .unwrap();

        // Should have a blob reference
        assert!(event.blob_ref.is_some());
        assert!(event.payload.is_empty()); // Payload moved to blob

        // Resolve the blob
        let blob_ref = event.blob_ref.unwrap();
        let content_ref = ContentRef::new(blob_ref.hash, blob_ref.size);
        let resolved = storage.resolve_blob(&content_ref).await.unwrap();
        assert_eq!(resolved.len(), large_data.len());
    }

    #[tokio::test]
    async fn test_peer_and_sync_state() {
        let (storage, _temp) = create_test_storage().await;

        let peer = SimulationIdentity::new('A').unwrap();
        let interface_id = InterfaceId::new([0xCD; 32]);

        // Register peer
        storage
            .register_peer(&peer, Some("Alice".to_string()))
            .unwrap();

        // Create interface and add member
        storage
            .create_interface(interface_id, Some("Chat".to_string()))
            .unwrap();
        storage.add_member(&interface_id, &peer).unwrap();

        // Update sync state
        let heads = vec![[0x01; 32], [0x02; 32]];
        storage
            .update_sync_state(&peer, &interface_id, heads.clone())
            .unwrap();

        // Queue events for delivery
        for i in 1..=3 {
            storage
                .queue_for_delivery(&peer, &interface_id, EventId::new(1, i))
                .unwrap();
        }

        // Check pending
        let pending = storage.pending_for(&peer, &interface_id).unwrap();
        assert_eq!(pending.len(), 3);

        // Acknowledge events
        storage
            .acknowledge_events(&peer, &interface_id, EventId::new(1, 2))
            .unwrap();

        // Check pending again
        let pending = storage.pending_for(&peer, &interface_id).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sequence, 3);
    }
}
