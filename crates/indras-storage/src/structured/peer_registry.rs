//! Peer registry storage
//!
//! Stores metadata about known peers.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::debug;

use indras_core::PeerIdentity;

use crate::error::StorageError;
use super::tables::{RedbStorage, PEER_REGISTRY};

/// Metadata about a peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    /// Peer identity bytes
    pub peer_id: Vec<u8>,
    /// Human-readable display name
    pub display_name: Option<String>,
    /// Last time we saw this peer (Unix millis)
    pub last_seen_millis: i64,
    /// First time we saw this peer (Unix millis)
    pub first_seen_millis: i64,
    /// Number of messages exchanged
    pub message_count: u64,
    /// Whether this peer is trusted
    pub trusted: bool,
    /// Custom metadata (application-specific)
    pub metadata: Vec<u8>,
}

impl PeerRecord {
    /// Create a new peer record
    pub fn new(peer_id: Vec<u8>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            peer_id,
            display_name: None,
            last_seen_millis: now,
            first_seen_millis: now,
            message_count: 0,
            trusted: false,
            metadata: Vec::new(),
        }
    }

    /// Set the display name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Mark as trusted
    pub fn with_trusted(mut self, trusted: bool) -> Self {
        self.trusted = trusted;
        self
    }
}

/// Peer registry for managing peer metadata
pub struct PeerRegistry {
    storage: Arc<RedbStorage>,
}

impl PeerRegistry {
    /// Create a new peer registry
    pub fn new(storage: Arc<RedbStorage>) -> Self {
        Self { storage }
    }

    /// Register or update a peer
    pub fn upsert<I: PeerIdentity>(&self, peer: &I, record: &PeerRecord) -> Result<(), StorageError> {
        let key = peer.as_bytes();
        let value = postcard::to_allocvec(record)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        self.storage.put(PEER_REGISTRY, &key, &value)?;
        debug!(peer_id = %peer.short_id(), "Updated peer record");
        Ok(())
    }

    /// Get a peer record
    pub fn get<I: PeerIdentity>(&self, peer: &I) -> Result<Option<PeerRecord>, StorageError> {
        let key = peer.as_bytes();
        match self.storage.get(PEER_REGISTRY, &key)? {
            Some(value) => {
                let record: PeerRecord = postcard::from_bytes(&value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// Update last seen time
    pub fn touch<I: PeerIdentity>(&self, peer: &I) -> Result<(), StorageError> {
        let key = peer.as_bytes();
        match self.storage.get(PEER_REGISTRY, &key)? {
            Some(value) => {
                let mut record: PeerRecord = postcard::from_bytes(&value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                record.last_seen_millis = chrono::Utc::now().timestamp_millis();

                let new_value = postcard::to_allocvec(&record)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                self.storage.put(PEER_REGISTRY, &key, &new_value)?;
            }
            None => {
                // Create new record
                let record = PeerRecord::new(key.clone());
                let value = postcard::to_allocvec(&record)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                self.storage.put(PEER_REGISTRY, &key, &value)?;
            }
        }
        Ok(())
    }

    /// Increment message count
    pub fn increment_messages<I: PeerIdentity>(&self, peer: &I) -> Result<u64, StorageError> {
        let key = peer.as_bytes();
        let mut record = self.get(peer)?.unwrap_or_else(|| PeerRecord::new(key.clone()));
        record.message_count += 1;
        record.last_seen_millis = chrono::Utc::now().timestamp_millis();

        let value = postcard::to_allocvec(&record)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        self.storage.put(PEER_REGISTRY, &key, &value)?;

        Ok(record.message_count)
    }

    /// Delete a peer record
    pub fn delete<I: PeerIdentity>(&self, peer: &I) -> Result<bool, StorageError> {
        let key = peer.as_bytes();
        self.storage.delete(PEER_REGISTRY, &key)
    }

    /// Get all peer records
    pub fn all(&self) -> Result<Vec<PeerRecord>, StorageError> {
        let entries = self.storage.scan_prefix(PEER_REGISTRY, &[])?;
        let mut records = Vec::with_capacity(entries.len());

        for (_key, value) in entries {
            let record: PeerRecord = postcard::from_bytes(&value)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?;
            records.push(record);
        }

        Ok(records)
    }

    /// Count all peers
    pub fn count(&self) -> Result<usize, StorageError> {
        self.storage.count_prefix(PEER_REGISTRY, &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;
    use tempfile::TempDir;

    use crate::structured::tables::RedbStorageConfig;

    fn create_test_registry() -> (PeerRegistry, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = RedbStorageConfig {
            db_path: temp_dir.path().join("test.redb"),
            ..Default::default()
        };
        let storage = Arc::new(RedbStorage::open(config).unwrap());
        (PeerRegistry::new(storage), temp_dir)
    }

    #[test]
    fn test_upsert_and_get() {
        let (registry, _temp) = create_test_registry();
        let peer = SimulationIdentity::new('A').unwrap();

        let record = PeerRecord::new(peer.as_bytes())
            .with_name("Alice")
            .with_trusted(true);

        registry.upsert(&peer, &record).unwrap();

        let retrieved = registry.get(&peer).unwrap().unwrap();
        assert_eq!(retrieved.display_name, Some("Alice".to_string()));
        assert!(retrieved.trusted);
    }

    #[test]
    fn test_touch() {
        let (registry, _temp) = create_test_registry();
        let peer = SimulationIdentity::new('B').unwrap();

        // Touch creates new record if not exists
        registry.touch(&peer).unwrap();

        let record = registry.get(&peer).unwrap().unwrap();
        assert!(record.last_seen_millis > 0);
    }

    #[test]
    fn test_increment_messages() {
        let (registry, _temp) = create_test_registry();
        let peer = SimulationIdentity::new('C').unwrap();

        for i in 1..=5 {
            let count = registry.increment_messages(&peer).unwrap();
            assert_eq!(count, i);
        }

        let record = registry.get(&peer).unwrap().unwrap();
        assert_eq!(record.message_count, 5);
    }

    #[test]
    fn test_all_peers() {
        let (registry, _temp) = create_test_registry();

        for c in ['A', 'B', 'C'] {
            let peer = SimulationIdentity::new(c).unwrap();
            registry.touch(&peer).unwrap();
        }

        let all = registry.all().unwrap();
        assert_eq!(all.len(), 3);
    }
}
