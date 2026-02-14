//! Yrs document backing an N-peer interface
//!
//! The InterfaceDocument stores:
//! - Member list (who is in the interface)
//! - Interface metadata (name, description, settings)
//! - Event log (serialized events as byte buffers in a Yrs Array)

use std::collections::HashSet;

use indras_core::{InterfaceEvent, InterfaceMetadata, PeerIdentity};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Any, Array, Doc, Map, Out, ReadTxn, StateVector, Transact, Update};

use crate::error::SyncError;

/// Keys used in the Yrs document structure
mod keys {
    pub const MEMBERS: &str = "members";
    pub const METADATA: &str = "metadata";
    pub const EVENTS: &str = "events";
    pub const NAME: &str = "name";
    pub const DESCRIPTION: &str = "description";
    pub const CREATED_AT: &str = "created_at";
    pub const CREATOR: &str = "creator";
}

/// Yrs document backing an NInterface
///
/// Document structure:
/// ```json
/// {
///   "members": { "peer_id_hex": true, ... },
///   "metadata": {
///     "name": "...",
///     "description": "...",
///     "created_at": timestamp,
///     "creator": "peer_id_hex"
///   },
///   "events": [
///     <serialized event bytes as Buffer>,
///     ...
///   ]
/// }
/// ```
pub struct InterfaceDocument {
    doc: Doc,
}

impl InterfaceDocument {
    /// Create a new empty document with pre-initialized shared types
    pub fn new() -> Self {
        let doc = Doc::new();

        // Pre-initialize the shared types so they exist in the document.
        // These calls register the root-level map/array names; the actual
        // MapRef/ArrayRef handles are cheap to re-obtain later.
        {
            let _members = doc.get_or_insert_map(keys::MEMBERS);
            let _metadata = doc.get_or_insert_map(keys::METADATA);
            let _events = doc.get_or_insert_array(keys::EVENTS);

            // Open a write transaction to commit the initial structure
            let _txn = doc.transact_mut();
            // Transaction auto-commits on drop
        }

        Self { doc }
    }

    /// Create from existing Yrs update bytes
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError> {
        let doc = Doc::new();

        // Pre-register shared type names so they're accessible
        let _members = doc.get_or_insert_map(keys::MEMBERS);
        let _metadata = doc.get_or_insert_map(keys::METADATA);
        let _events = doc.get_or_insert_array(keys::EVENTS);

        // Apply the saved state
        let update =
            Update::decode_v1(bytes).map_err(|e| SyncError::DocumentLoad(e.to_string()))?;
        {
            let mut txn = doc.transact_mut();
            txn.apply_update(update)
                .map_err(|e| SyncError::DocumentLoad(e.to_string()))?;
        }

        Ok(Self { doc })
    }

    /// Export the full document state as bytes
    pub fn save(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    }

    /// Get the encoded state vector (replaces heads/heads_as_bytes)
    pub fn state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    /// Generate a sync message (update) for a peer given their encoded state vector.
    ///
    /// Returns the minimal update bytes that the peer needs to catch up.
    pub fn generate_sync_message(&self, their_sv_bytes: &[u8]) -> Result<Vec<u8>, SyncError> {
        let their_sv = if their_sv_bytes.is_empty() {
            // Empty bytes means the peer has no state — send everything
            StateVector::default()
        } else {
            StateVector::decode_v1(their_sv_bytes)
                .map_err(|e| SyncError::SyncMerge(e.to_string()))?
        };
        let txn = self.doc.transact();
        Ok(txn.encode_state_as_update_v1(&their_sv))
    }

    /// Apply a sync message (update bytes) from a peer
    pub fn apply_sync_message(&self, update_bytes: &[u8]) -> Result<(), SyncError> {
        let update =
            Update::decode_v1(update_bytes).map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        Ok(())
    }

    /// Add a member to the interface
    pub fn add_member<I: PeerIdentity>(&self, peer: &I) {
        let members = self.doc.get_or_insert_map(keys::MEMBERS);
        let mut txn = self.doc.transact_mut();
        let peer_key = hex::encode(peer.as_bytes());
        members.insert(&mut txn, peer_key.as_str(), true);
    }

    /// Remove a member from the interface
    pub fn remove_member<I: PeerIdentity>(&self, peer: &I) {
        let members = self.doc.get_or_insert_map(keys::MEMBERS);
        let mut txn = self.doc.transact_mut();
        let peer_key = hex::encode(peer.as_bytes());
        members.remove(&mut txn, &peer_key);
    }

    /// Check if a peer is a member
    pub fn is_member<I: PeerIdentity>(&self, peer: &I) -> bool {
        let members = self.doc.get_or_insert_map(keys::MEMBERS);
        let txn = self.doc.transact();
        let peer_key = hex::encode(peer.as_bytes());
        members.get(&txn, &peer_key).is_some()
    }

    /// Get all members
    pub fn members<I: PeerIdentity>(&self) -> HashSet<I> {
        let members_map = self.doc.get_or_insert_map(keys::MEMBERS);
        let txn = self.doc.transact();
        let mut members = HashSet::new();

        for (key, _value) in members_map.iter(&txn) {
            if let Ok(bytes) = hex::decode(key) {
                if let Ok(peer) = I::from_bytes(&bytes) {
                    members.insert(peer);
                }
            }
        }

        members
    }

    /// Set interface metadata
    pub fn set_metadata<I: PeerIdentity>(&self, metadata: &InterfaceMetadata<I>) {
        let meta_map = self.doc.get_or_insert_map(keys::METADATA);
        let mut txn = self.doc.transact_mut();

        if let Some(name) = &metadata.name {
            meta_map.insert(&mut txn, keys::NAME, name.as_str());
        }
        if let Some(desc) = &metadata.description {
            meta_map.insert(&mut txn, keys::DESCRIPTION, desc.as_str());
        }
        meta_map.insert(
            &mut txn,
            keys::CREATED_AT,
            metadata.created_at.timestamp(),
        );
        meta_map.insert(
            &mut txn,
            keys::CREATOR,
            hex::encode(metadata.creator.as_bytes()).as_str(),
        );
    }

    /// Append an event to the event log
    ///
    /// Events are serialized with postcard and stored as byte buffers in the array.
    pub fn append_event<I: PeerIdentity>(
        &self,
        event: &InterfaceEvent<I>,
    ) -> Result<(), SyncError> {
        let event_bytes =
            postcard::to_allocvec(event).map_err(|e| SyncError::Serialization(e.to_string()))?;

        let events = self.doc.get_or_insert_array(keys::EVENTS);
        let mut txn = self.doc.transact_mut();
        let len = events.len(&txn);
        events.insert(&mut txn, len, Any::Buffer(event_bytes.into()));

        Ok(())
    }

    /// Get all events from the log
    pub fn events<I: PeerIdentity>(&self) -> Vec<InterfaceEvent<I>> {
        let events_array = self.doc.get_or_insert_array(keys::EVENTS);
        let txn = self.doc.transact();
        let mut events = Vec::new();

        for value in events_array.iter(&txn) {
            if let Out::Any(Any::Buffer(buf)) = value {
                if let Ok(event) = postcard::from_bytes::<InterfaceEvent<I>>(&buf) {
                    events.push(event);
                }
            }
        }

        events
    }

    /// Get events since a specific index
    pub fn events_since<I: PeerIdentity>(&self, since: usize) -> Vec<InterfaceEvent<I>> {
        let events_array = self.doc.get_or_insert_array(keys::EVENTS);
        let txn = self.doc.transact();
        let mut events = Vec::new();
        let len = events_array.len(&txn) as usize;

        for i in since..len {
            if let Some(value) = events_array.get(&txn, i as u32) {
                if let Out::Any(Any::Buffer(buf)) = value {
                    if let Ok(event) = postcard::from_bytes::<InterfaceEvent<I>>(&buf) {
                        events.push(event);
                    }
                }
            }
        }

        events
    }

    /// Get the number of events in the log
    pub fn event_count(&self) -> usize {
        let events_array = self.doc.get_or_insert_array(keys::EVENTS);
        let txn = self.doc.transact();
        events_array.len(&txn) as usize
    }

    /// Fork this document (create an independent copy)
    pub fn fork(&self) -> Result<Self, SyncError> {
        let bytes = self.save();
        Self::load(&bytes)
    }

    /// Merge another document into this one
    pub fn merge(&self, other: &InterfaceDocument) -> Result<(), SyncError> {
        let other_bytes = other.save();
        self.apply_sync_message(&other_bytes)
    }
}

impl Default for InterfaceDocument {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    #[test]
    fn test_document_creation() {
        let doc = InterfaceDocument::new();
        assert_eq!(doc.event_count(), 0);
    }

    #[test]
    fn test_member_management() {
        let doc = InterfaceDocument::new();
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();

        // Add members
        doc.add_member(&peer_a);
        doc.add_member(&peer_b);

        assert!(doc.is_member(&peer_a));
        assert!(doc.is_member(&peer_b));

        let members: HashSet<SimulationIdentity> = doc.members();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&peer_a));
        assert!(members.contains(&peer_b));

        // Remove member
        doc.remove_member(&peer_a);
        assert!(!doc.is_member(&peer_a));
        assert!(doc.is_member(&peer_b));
    }

    #[test]
    fn test_event_append_and_retrieval() {
        let doc = InterfaceDocument::new();
        let peer_a = SimulationIdentity::new('A').unwrap();

        let event = InterfaceEvent::message(peer_a, 1, b"Hello world".to_vec());

        doc.append_event(&event).unwrap();
        assert_eq!(doc.event_count(), 1);

        let events: Vec<InterfaceEvent<SimulationIdentity>> = doc.events();
        assert_eq!(events.len(), 1);

        match &events[0] {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Hello world");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_save_and_load() {
        let doc = InterfaceDocument::new();
        let peer_a = SimulationIdentity::new('A').unwrap();

        doc.add_member(&peer_a);
        doc.append_event(&InterfaceEvent::message(peer_a, 1, b"Test".to_vec()))
            .unwrap();

        let bytes = doc.save();

        let loaded = InterfaceDocument::load(&bytes).unwrap();
        assert!(loaded.is_member(&peer_a));
        assert_eq!(loaded.event_count(), 1);
    }

    #[test]
    fn test_sync_between_documents() {
        // Create doc1 and make changes
        let doc1 = InterfaceDocument::new();
        let peer_a = SimulationIdentity::new('A').unwrap();

        doc1.add_member(&peer_a);
        doc1.append_event(&InterfaceEvent::message(peer_a, 1, b"From A".to_vec()))
            .unwrap();

        // Save and reload as doc2 (simulates initial sync)
        let bytes = doc1.save();
        let doc2 = InterfaceDocument::load(&bytes).unwrap();

        // Doc2 should have the member and event
        assert!(doc2.is_member(&peer_a));
        assert_eq!(doc2.event_count(), 1);
    }

    #[test]
    fn test_incremental_sync() {
        // Create initial document
        let doc1 = InterfaceDocument::new();
        let peer_a = SimulationIdentity::new('A').unwrap();
        doc1.add_member(&peer_a);

        // Save initial state and create doc2 from it
        let initial_bytes = doc1.save();
        let doc2 = InterfaceDocument::load(&initial_bytes).unwrap();

        // Get doc2's state vector before any more changes
        let doc2_sv = doc2.state_vector();

        // Add an event to doc1
        doc1.append_event(&InterfaceEvent::message(peer_a, 1, b"New message".to_vec()))
            .unwrap();

        // Generate incremental sync
        let sync_msg = doc1.generate_sync_message(&doc2_sv).unwrap();

        // Apply to doc2
        doc2.apply_sync_message(&sync_msg).unwrap();

        // Doc2 should now have the new event
        assert_eq!(doc2.event_count(), 1);
        let events: Vec<InterfaceEvent<SimulationIdentity>> = doc2.events();
        match &events[0] {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"New message");
            }
            _ => panic!("Expected Message event"),
        }
    }
}
