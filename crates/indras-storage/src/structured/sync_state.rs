//! Sync state storage
//!
//! Tracks synchronization state between peers for each interface.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::debug;

use indras_core::{EventId, InterfaceId, PeerIdentity};

use super::tables::{PENDING_DELIVERY, RedbStorage, SYNC_STATE};
use crate::error::StorageError;

/// Sync state for a (peer, interface) pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStateRecord {
    /// Peer ID bytes
    pub peer_id: Vec<u8>,
    /// Interface ID
    pub interface_id: [u8; 32],
    /// Their last known document heads (Automerge change hashes)
    pub their_heads: Vec<[u8; 32]>,
    /// Last event ID they've acknowledged
    pub last_acked_event: Option<EventId>,
    /// Last sync timestamp (Unix millis)
    pub last_sync_millis: i64,
    /// Number of sync rounds completed
    pub sync_count: u64,
    /// Whether sync is currently in progress
    pub sync_in_progress: bool,
}

impl SyncStateRecord {
    /// Create a new sync state record
    pub fn new(peer_id: Vec<u8>, interface_id: InterfaceId) -> Self {
        Self {
            peer_id,
            interface_id: *interface_id.as_bytes(),
            their_heads: Vec::new(),
            last_acked_event: None,
            last_sync_millis: 0,
            sync_count: 0,
            sync_in_progress: false,
        }
    }

    /// Update heads from sync
    pub fn with_heads(mut self, heads: Vec<[u8; 32]>) -> Self {
        self.their_heads = heads;
        self
    }

    /// Mark an event as acknowledged
    pub fn with_acked_event(mut self, event_id: EventId) -> Self {
        self.last_acked_event = Some(event_id);
        self
    }
}

/// Pending delivery record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDeliveryRecord {
    /// Event ID
    pub event_id: EventId,
    /// When it was queued for delivery (Unix millis)
    pub queued_at_millis: i64,
    /// Number of delivery attempts
    pub attempt_count: u32,
    /// Last attempt timestamp
    pub last_attempt_millis: Option<i64>,
}

impl PendingDeliveryRecord {
    /// Create a new pending delivery record
    pub fn new(event_id: EventId) -> Self {
        Self {
            event_id,
            queued_at_millis: chrono::Utc::now().timestamp_millis(),
            attempt_count: 0,
            last_attempt_millis: None,
        }
    }
}

/// Sync state storage manager
pub struct SyncStateStore {
    storage: Arc<RedbStorage>,
}

impl SyncStateStore {
    /// Create a new sync state store
    pub fn new(storage: Arc<RedbStorage>) -> Self {
        Self { storage }
    }

    /// Get or create sync state for a peer/interface pair
    pub fn get_or_create<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
    ) -> Result<SyncStateRecord, StorageError> {
        let key = self.make_sync_key(peer, interface_id);

        match self.storage.get(SYNC_STATE, &key)? {
            Some(value) => postcard::from_bytes(&value)
                .map_err(|e| StorageError::Deserialization(e.to_string())),
            None => {
                let record = SyncStateRecord::new(peer.as_bytes(), *interface_id);
                self.update(peer, interface_id, &record)?;
                Ok(record)
            }
        }
    }

    /// Update sync state
    pub fn update<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        record: &SyncStateRecord,
    ) -> Result<(), StorageError> {
        let key = self.make_sync_key(peer, interface_id);
        let value = postcard::to_allocvec(record)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        self.storage.put(SYNC_STATE, &key, &value)?;
        Ok(())
    }

    /// Update heads after sync
    pub fn update_heads<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        heads: Vec<[u8; 32]>,
    ) -> Result<(), StorageError> {
        let mut record = self.get_or_create(peer, interface_id)?;
        record.their_heads = heads;
        record.last_sync_millis = chrono::Utc::now().timestamp_millis();
        record.sync_count += 1;
        self.update(peer, interface_id, &record)
    }

    /// Acknowledge events
    pub fn acknowledge_events<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        up_to: EventId,
    ) -> Result<(), StorageError> {
        let mut record = self.get_or_create(peer, interface_id)?;
        record.last_acked_event = Some(up_to);
        self.update(peer, interface_id, &record)?;

        // Clear pending deliveries up to this event
        self.clear_pending_up_to(peer, interface_id, up_to)?;

        debug!(
            peer = %peer.short_id(),
            event_id = ?up_to,
            "Acknowledged events"
        );
        Ok(())
    }

    /// Add a pending delivery
    pub fn add_pending<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        event_id: EventId,
    ) -> Result<(), StorageError> {
        let key = self.make_pending_key(peer, interface_id, event_id);
        let record = PendingDeliveryRecord::new(event_id);
        let value = postcard::to_allocvec(&record)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        self.storage.put(PENDING_DELIVERY, &key, &value)
    }

    /// Get all pending deliveries for a peer/interface
    pub fn get_pending<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
    ) -> Result<Vec<PendingDeliveryRecord>, StorageError> {
        let prefix = self.make_pending_prefix(peer, interface_id);
        let entries = self.storage.scan_prefix(PENDING_DELIVERY, &prefix)?;

        let mut records = Vec::with_capacity(entries.len());
        for (_key, value) in entries {
            let record: PendingDeliveryRecord = postcard::from_bytes(&value)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?;
            records.push(record);
        }

        // Sort by event ID
        records.sort_by_key(|r| r.event_id);
        Ok(records)
    }

    /// Clear pending deliveries up to an event
    fn clear_pending_up_to<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        up_to: EventId,
    ) -> Result<usize, StorageError> {
        let pending = self.get_pending(peer, interface_id)?;
        let mut cleared = 0;

        for record in pending {
            if record.event_id <= up_to {
                let key = self.make_pending_key(peer, interface_id, record.event_id);
                self.storage.delete(PENDING_DELIVERY, &key)?;
                cleared += 1;
            }
        }

        Ok(cleared)
    }

    /// Mark a delivery attempt
    pub fn record_attempt<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        event_id: EventId,
    ) -> Result<(), StorageError> {
        let key = self.make_pending_key(peer, interface_id, event_id);

        if let Some(value) = self.storage.get(PENDING_DELIVERY, &key)? {
            let mut record: PendingDeliveryRecord = postcard::from_bytes(&value)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?;

            record.attempt_count += 1;
            record.last_attempt_millis = Some(chrono::Utc::now().timestamp_millis());

            let new_value = postcard::to_allocvec(&record)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            self.storage.put(PENDING_DELIVERY, &key, &new_value)?;
        }

        Ok(())
    }

    /// Get all sync states for an interface
    pub fn get_all_for_interface(
        &self,
        interface_id: &InterfaceId,
    ) -> Result<Vec<SyncStateRecord>, StorageError> {
        // We need to scan all sync states and filter by interface
        // This is not efficient, but works for now
        let entries = self.storage.scan_prefix(SYNC_STATE, &[])?;

        let mut records = Vec::new();
        for (_key, value) in entries {
            let record: SyncStateRecord = postcard::from_bytes(&value)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?;

            if record.interface_id == *interface_id.as_bytes() {
                records.push(record);
            }
        }

        Ok(records)
    }

    /// Make the key for sync state
    fn make_sync_key<I: PeerIdentity>(&self, peer: &I, interface_id: &InterfaceId) -> Vec<u8> {
        let peer_bytes = peer.as_bytes();
        let mut key = Vec::with_capacity(peer_bytes.len() + 32);
        key.extend_from_slice(&peer_bytes);
        key.extend_from_slice(interface_id.as_bytes());
        key
    }

    /// Make the key for pending delivery
    fn make_pending_key<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
        event_id: EventId,
    ) -> Vec<u8> {
        let peer_bytes = peer.as_bytes();
        let event_bytes = event_id.to_bytes();

        let mut key = Vec::with_capacity(peer_bytes.len() + 32 + event_bytes.len());
        key.extend_from_slice(&peer_bytes);
        key.extend_from_slice(interface_id.as_bytes());
        key.extend_from_slice(&event_bytes);
        key
    }

    /// Make the prefix for pending deliveries
    fn make_pending_prefix<I: PeerIdentity>(
        &self,
        peer: &I,
        interface_id: &InterfaceId,
    ) -> Vec<u8> {
        let peer_bytes = peer.as_bytes();
        let mut prefix = Vec::with_capacity(peer_bytes.len() + 32);
        prefix.extend_from_slice(&peer_bytes);
        prefix.extend_from_slice(interface_id.as_bytes());
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;
    use tempfile::TempDir;

    use crate::structured::tables::RedbStorageConfig;

    fn create_test_store() -> (SyncStateStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = RedbStorageConfig {
            db_path: temp_dir.path().join("test.redb"),
            ..Default::default()
        };
        let storage = Arc::new(RedbStorage::open(config).unwrap());
        (SyncStateStore::new(storage), temp_dir)
    }

    #[test]
    fn test_sync_state_crud() {
        let (store, _temp) = create_test_store();
        let peer = SimulationIdentity::new('A').unwrap();
        let interface_id = InterfaceId::new([0x42; 32]);

        // Get or create
        let record = store.get_or_create(&peer, &interface_id).unwrap();
        assert!(record.their_heads.is_empty());
        assert!(record.last_acked_event.is_none());

        // Update heads
        let heads = vec![[0x01; 32], [0x02; 32]];
        store
            .update_heads(&peer, &interface_id, heads.clone())
            .unwrap();

        let record = store.get_or_create(&peer, &interface_id).unwrap();
        assert_eq!(record.their_heads.len(), 2);
        assert_eq!(record.sync_count, 1);
    }

    #[test]
    fn test_pending_delivery() {
        let (store, _temp) = create_test_store();
        let peer = SimulationIdentity::new('B').unwrap();
        let interface_id = InterfaceId::new([0xAB; 32]);

        // Add pending events
        for i in 1..=5 {
            let event_id = EventId::new(1, i);
            store.add_pending(&peer, &interface_id, event_id).unwrap();
        }

        // Get pending
        let pending = store.get_pending(&peer, &interface_id).unwrap();
        assert_eq!(pending.len(), 5);

        // Acknowledge up to event 3
        store
            .acknowledge_events(&peer, &interface_id, EventId::new(1, 3))
            .unwrap();

        // Should only have events 4 and 5 pending
        let pending = store.get_pending(&peer, &interface_id).unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].event_id.sequence, 4);
        assert_eq!(pending[1].event_id.sequence, 5);
    }

    #[test]
    fn test_record_attempt() {
        let (store, _temp) = create_test_store();
        let peer = SimulationIdentity::new('C').unwrap();
        let interface_id = InterfaceId::new([0xCD; 32]);
        let event_id = EventId::new(1, 1);

        store.add_pending(&peer, &interface_id, event_id).unwrap();

        // Record attempts
        for _ in 0..3 {
            store
                .record_attempt(&peer, &interface_id, event_id)
                .unwrap();
        }

        let pending = store.get_pending(&peer, &interface_id).unwrap();
        assert_eq!(pending[0].attempt_count, 3);
        assert!(pending[0].last_attempt_millis.is_some());
    }
}
