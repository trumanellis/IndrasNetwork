//! Interface storage
//!
//! Stores interface metadata and membership.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::debug;

use indras_core::{InterfaceId, PeerIdentity};

use super::tables::{INTERFACE_MEMBERS, INTERFACES, RedbStorage};
use crate::error::StorageError;

/// Metadata about an interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceRecord {
    /// Interface ID bytes
    pub interface_id: [u8; 32],
    /// Human-readable name
    pub name: Option<String>,
    /// Description
    pub description: Option<String>,
    /// When the interface was created (Unix millis)
    pub created_at_millis: i64,
    /// When we last saw activity (Unix millis)
    pub last_activity_millis: i64,
    /// Number of events in the interface
    pub event_count: u64,
    /// Number of members
    pub member_count: u32,
    /// Whether this is an encrypted interface
    pub encrypted: bool,
    /// Interface key (encrypted, if applicable)
    pub encrypted_key: Option<Vec<u8>>,
}

impl InterfaceRecord {
    /// Create a new interface record
    pub fn new(interface_id: InterfaceId) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            interface_id: *interface_id.as_bytes(),
            name: None,
            description: None,
            created_at_millis: now,
            last_activity_millis: now,
            event_count: 0,
            member_count: 0,
            encrypted: false,
            encrypted_key: None,
        }
    }

    /// Set the name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark as encrypted
    pub fn with_encrypted(mut self, encrypted: bool) -> Self {
        self.encrypted = encrypted;
        self
    }
}

/// Membership record for an interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipRecord {
    /// Peer ID bytes
    pub peer_id: Vec<u8>,
    /// When the peer joined (Unix millis)
    pub joined_at_millis: i64,
    /// Role in the interface (e.g., "admin", "member")
    pub role: String,
    /// Whether the member is currently active
    pub active: bool,
    /// Last activity timestamp
    pub last_activity_millis: i64,
}

impl MembershipRecord {
    /// Create a new membership record
    pub fn new(peer_id: Vec<u8>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            peer_id,
            joined_at_millis: now,
            role: "member".to_string(),
            active: true,
            last_activity_millis: now,
        }
    }

    /// Set the role
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = role.into();
        self
    }
}

/// Interface storage manager
pub struct InterfaceStore {
    storage: Arc<RedbStorage>,
}

impl InterfaceStore {
    /// Create a new interface store
    pub fn new(storage: Arc<RedbStorage>) -> Self {
        Self { storage }
    }

    /// Create or update an interface record
    pub fn upsert(&self, record: &InterfaceRecord) -> Result<(), StorageError> {
        let key = record.interface_id;
        let value = postcard::to_allocvec(record)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        self.storage.put(INTERFACES, &key, &value)?;
        debug!(interface = %hex::encode(key), "Updated interface record");
        Ok(())
    }

    /// Get an interface record
    pub fn get(&self, interface_id: &InterfaceId) -> Result<Option<InterfaceRecord>, StorageError> {
        let key = interface_id.as_bytes();
        match self.storage.get(INTERFACES, key)? {
            Some(value) => {
                let record: InterfaceRecord = postcard::from_bytes(&value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// Increment event count
    pub fn increment_events(&self, interface_id: &InterfaceId) -> Result<u64, StorageError> {
        match self.get(interface_id)? {
            Some(mut record) => {
                record.event_count += 1;
                record.last_activity_millis = chrono::Utc::now().timestamp_millis();
                self.upsert(&record)?;
                Ok(record.event_count)
            }
            None => Err(StorageError::PacketNotFound(hex::encode(
                interface_id.as_bytes(),
            ))),
        }
    }

    /// Delete an interface
    pub fn delete(&self, interface_id: &InterfaceId) -> Result<bool, StorageError> {
        let key = interface_id.as_bytes();

        // Delete all members first
        let prefix = self.make_member_prefix(interface_id);
        let members = self.storage.scan_prefix(INTERFACE_MEMBERS, &prefix)?;
        for (key, _) in members {
            self.storage.delete(INTERFACE_MEMBERS, &key)?;
        }

        // Delete the interface record
        self.storage.delete(INTERFACES, key)
    }

    /// Add a member to an interface
    pub fn add_member<I: PeerIdentity>(
        &self,
        interface_id: &InterfaceId,
        peer: &I,
        record: &MembershipRecord,
    ) -> Result<(), StorageError> {
        let key = self.make_member_key(interface_id, peer);
        let value = postcard::to_allocvec(record)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        self.storage.put(INTERFACE_MEMBERS, &key, &value)?;

        // Update member count
        if let Some(mut iface_record) = self.get(interface_id)? {
            iface_record.member_count += 1;
            self.upsert(&iface_record)?;
        }

        debug!(
            interface = %hex::encode(interface_id.as_bytes()),
            peer = %peer.short_id(),
            "Added member to interface"
        );
        Ok(())
    }

    /// Remove a member from an interface
    pub fn remove_member<I: PeerIdentity>(
        &self,
        interface_id: &InterfaceId,
        peer: &I,
    ) -> Result<bool, StorageError> {
        let key = self.make_member_key(interface_id, peer);
        let removed = self.storage.delete(INTERFACE_MEMBERS, &key)?;

        if removed && let Some(mut iface_record) = self.get(interface_id)? {
            iface_record.member_count = iface_record.member_count.saturating_sub(1);
            self.upsert(&iface_record)?;
        }

        Ok(removed)
    }

    /// Get membership record
    pub fn get_member<I: PeerIdentity>(
        &self,
        interface_id: &InterfaceId,
        peer: &I,
    ) -> Result<Option<MembershipRecord>, StorageError> {
        let key = self.make_member_key(interface_id, peer);
        match self.storage.get(INTERFACE_MEMBERS, &key)? {
            Some(value) => {
                let record: MembershipRecord = postcard::from_bytes(&value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// Check if a peer is a member
    pub fn is_member<I: PeerIdentity>(
        &self,
        interface_id: &InterfaceId,
        peer: &I,
    ) -> Result<bool, StorageError> {
        Ok(self.get_member(interface_id, peer)?.is_some())
    }

    /// Get all members of an interface
    pub fn get_members(
        &self,
        interface_id: &InterfaceId,
    ) -> Result<Vec<MembershipRecord>, StorageError> {
        let prefix = self.make_member_prefix(interface_id);
        let entries = self.storage.scan_prefix(INTERFACE_MEMBERS, &prefix)?;

        let mut members = Vec::with_capacity(entries.len());
        for (_key, value) in entries {
            let record: MembershipRecord = postcard::from_bytes(&value)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?;
            members.push(record);
        }

        Ok(members)
    }

    /// Get all interfaces
    pub fn all(&self) -> Result<Vec<InterfaceRecord>, StorageError> {
        let entries = self.storage.scan_prefix(INTERFACES, &[])?;
        let mut records = Vec::with_capacity(entries.len());

        for (_key, value) in entries {
            let record: InterfaceRecord = postcard::from_bytes(&value)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?;
            records.push(record);
        }

        Ok(records)
    }

    /// Make the key for a member entry
    fn make_member_key<I: PeerIdentity>(&self, interface_id: &InterfaceId, peer: &I) -> Vec<u8> {
        let mut key = Vec::with_capacity(32 + peer.as_bytes().len());
        key.extend_from_slice(interface_id.as_bytes());
        key.extend_from_slice(&peer.as_bytes());
        key
    }

    /// Make the prefix for scanning members of an interface
    fn make_member_prefix(&self, interface_id: &InterfaceId) -> Vec<u8> {
        interface_id.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;
    use tempfile::TempDir;

    use crate::structured::tables::RedbStorageConfig;

    fn create_test_store() -> (InterfaceStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = RedbStorageConfig {
            db_path: temp_dir.path().join("test.redb"),
            ..Default::default()
        };
        let storage = Arc::new(RedbStorage::open(config).unwrap());
        (InterfaceStore::new(storage), temp_dir)
    }

    #[test]
    fn test_interface_crud() {
        let (store, _temp) = create_test_store();
        let interface_id = InterfaceId::new([0x42; 32]);

        let record = InterfaceRecord::new(interface_id)
            .with_name("Test Interface")
            .with_description("A test interface");

        store.upsert(&record).unwrap();

        let retrieved = store.get(&interface_id).unwrap().unwrap();
        assert_eq!(retrieved.name, Some("Test Interface".to_string()));

        let deleted = store.delete(&interface_id).unwrap();
        assert!(deleted);

        assert!(store.get(&interface_id).unwrap().is_none());
    }

    #[test]
    fn test_membership() {
        let (store, _temp) = create_test_store();
        let interface_id = InterfaceId::new([0xAB; 32]);

        // Create interface
        let record = InterfaceRecord::new(interface_id);
        store.upsert(&record).unwrap();

        // Add members
        for c in ['A', 'B', 'C'] {
            let peer = SimulationIdentity::new(c).unwrap();
            let membership = MembershipRecord::new(peer.as_bytes());
            store.add_member(&interface_id, &peer, &membership).unwrap();
        }

        // Check member count
        let iface = store.get(&interface_id).unwrap().unwrap();
        assert_eq!(iface.member_count, 3);

        // Get all members
        let members = store.get_members(&interface_id).unwrap();
        assert_eq!(members.len(), 3);

        // Check is_member
        let peer_a = SimulationIdentity::new('A').unwrap();
        assert!(store.is_member(&interface_id, &peer_a).unwrap());

        let peer_z = SimulationIdentity::new('Z').unwrap();
        assert!(!store.is_member(&interface_id, &peer_z).unwrap());
    }
}
