//! Automerge document backing an N-peer interface
//!
//! The InterfaceDocument stores:
//! - Member list (who is in the interface)
//! - Interface metadata (name, description, settings)
//! - Event log (serialized events as byte buffers in an Automerge List)

use std::collections::HashSet;

use automerge::sync::SyncDoc;
use automerge::transaction::Transactable;
use automerge::{AutoCommit, ObjId, ObjType, ReadDoc, ScalarValue, Value, ROOT};
use indras_core::{InterfaceEvent, InterfaceMetadata, PeerIdentity};

use crate::error::SyncError;

/// Keys used in the Automerge document structure
mod keys {
    pub const MEMBERS: &str = "members";
    pub const METADATA: &str = "metadata";
    pub const EVENTS: &str = "events";
    pub const NAME: &str = "name";
    pub const DESCRIPTION: &str = "description";
    pub const CREATED_AT: &str = "created_at";
    pub const CREATOR: &str = "creator";
}

/// Automerge document backing an NInterface
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
///     <serialized event bytes>,
///     ...
///   ]
/// }
/// ```
pub struct InterfaceDocument {
    doc: AutoCommit,
}

impl InterfaceDocument {
    // ===== Dynamic ObjId lookup helpers =====
    // CRITICAL: Never cache ObjIds — they go stale after sync/merge.

    fn members_obj(&self) -> ObjId {
        self.doc
            .get(ROOT, keys::MEMBERS)
            .expect("members lookup failed")
            .expect("members map missing")
            .1
    }

    fn metadata_obj(&self) -> ObjId {
        self.doc
            .get(ROOT, keys::METADATA)
            .expect("metadata lookup failed")
            .expect("metadata map missing")
            .1
    }

    fn events_obj(&self) -> ObjId {
        self.doc
            .get(ROOT, keys::EVENTS)
            .expect("events lookup failed")
            .expect("events list missing")
            .1
    }

    /// Create a new empty document with pre-initialized shared types
    pub fn new() -> Self {
        let mut doc = AutoCommit::new();

        doc.put_object(ROOT, keys::MEMBERS, ObjType::Map)
            .expect("Failed to create members map");
        doc.put_object(ROOT, keys::METADATA, ObjType::Map)
            .expect("Failed to create metadata map");
        doc.put_object(ROOT, keys::EVENTS, ObjType::List)
            .expect("Failed to create events list");

        Self { doc }
    }

    /// Create from existing Automerge document bytes
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError> {
        let doc =
            AutoCommit::load(bytes).map_err(|e| SyncError::DocumentLoad(e.to_string()))?;
        Ok(Self { doc })
    }

    /// Export the full document state as bytes
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    /// Get the document state as bytes (for sync compatibility)
    ///
    /// With Automerge, the state vector concept is replaced by full document
    /// bytes. This returns the same as `save()`.
    pub fn state_vector(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    /// Generate a sync message using Automerge's built-in sync protocol.
    ///
    /// Returns `None` when the peer is fully up-to-date.
    pub fn generate_sync_message(
        &mut self,
        peer_state: &mut automerge::sync::State,
    ) -> Option<automerge::sync::Message> {
        self.doc.sync().generate_sync_message(peer_state)
    }

    /// Receive a sync message using Automerge's built-in sync protocol.
    pub fn receive_sync_message(
        &mut self,
        peer_state: &mut automerge::sync::State,
        message: automerge::sync::Message,
    ) -> Result<(), SyncError> {
        self.doc
            .sync()
            .receive_sync_message(peer_state, message)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))
    }

    /// Apply raw document bytes by merging (convenience for simple sync).
    ///
    /// Loads the bytes as an Automerge document and merges it into ours.
    /// Used by `merge_sync` and tests that don't need per-peer sync state.
    pub fn apply_update(&mut self, bytes: &[u8]) -> Result<(), SyncError> {
        let mut other =
            AutoCommit::load(bytes).map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        self.doc
            .merge(&mut other)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        Ok(())
    }

    /// Get the current change heads (for sync completion checking)
    pub fn get_heads(&mut self) -> Vec<automerge::ChangeHash> {
        self.doc.get_heads()
    }

    /// Add a member to the interface
    pub fn add_member<I: PeerIdentity>(&mut self, peer: &I) {
        let members = self.members_obj();
        let peer_key = hex::encode(peer.as_bytes());
        self.doc
            .put(&members, peer_key.as_str(), true)
            .expect("Failed to add member");
    }

    /// Remove a member from the interface
    pub fn remove_member<I: PeerIdentity>(&mut self, peer: &I) {
        let members = self.members_obj();
        let peer_key = hex::encode(peer.as_bytes());
        self.doc
            .delete(&members, peer_key.as_str())
            .expect("Failed to remove member");
    }

    /// Check if a peer is a member
    pub fn is_member<I: PeerIdentity>(&self, peer: &I) -> bool {
        let members = self.members_obj();
        let peer_key = hex::encode(peer.as_bytes());
        self.doc
            .get(&members, peer_key.as_str())
            .ok()
            .flatten()
            .is_some()
    }

    /// Get all members
    pub fn members<I: PeerIdentity>(&self) -> HashSet<I> {
        let members_obj = self.members_obj();
        let mut members = HashSet::new();

        for key in self.doc.keys(&members_obj) {
            if let Ok(bytes) = hex::decode(&key) {
                if let Ok(peer) = I::from_bytes(&bytes) {
                    members.insert(peer);
                }
            }
        }

        members
    }

    /// Set interface metadata
    pub fn set_metadata<I: PeerIdentity>(&mut self, metadata: &InterfaceMetadata<I>) {
        let meta = self.metadata_obj();

        if let Some(name) = &metadata.name {
            self.doc
                .put(&meta, keys::NAME, name.as_str())
                .expect("Failed to set name");
        }
        if let Some(desc) = &metadata.description {
            self.doc
                .put(&meta, keys::DESCRIPTION, desc.as_str())
                .expect("Failed to set description");
        }
        self.doc
            .put(&meta, keys::CREATED_AT, metadata.created_at.timestamp())
            .expect("Failed to set created_at");
        self.doc
            .put(
                &meta,
                keys::CREATOR,
                hex::encode(metadata.creator.as_bytes()).as_str(),
            )
            .expect("Failed to set creator");
    }

    /// Append an event to the event log
    ///
    /// Events are serialized with postcard and stored as byte buffers in the list.
    pub fn append_event<I: PeerIdentity>(
        &mut self,
        event: &InterfaceEvent<I>,
    ) -> Result<(), SyncError> {
        let event_bytes =
            postcard::to_allocvec(event).map_err(|e| SyncError::Serialization(e.to_string()))?;

        let events = self.events_obj();
        let len = self.doc.length(&events);
        self.doc
            .insert(&events, len, ScalarValue::Bytes(event_bytes))
            .map_err(|e| SyncError::DocumentOperation(e.to_string()))?;

        Ok(())
    }

    /// Get all events from the log
    pub fn events<I: PeerIdentity>(&self) -> Vec<InterfaceEvent<I>> {
        let events_obj = self.events_obj();
        let len = self.doc.length(&events_obj);
        let mut events = Vec::new();

        for i in 0..len {
            if let Ok(Some((value, _))) = self.doc.get(&events_obj, i) {
                if let Value::Scalar(cow) = value {
                    if let ScalarValue::Bytes(buf) = cow.as_ref() {
                        if let Ok(event) = postcard::from_bytes::<InterfaceEvent<I>>(buf) {
                            events.push(event);
                        }
                    }
                }
            }
        }

        events
    }

    /// Get events since a specific index
    pub fn events_since<I: PeerIdentity>(&self, since: usize) -> Vec<InterfaceEvent<I>> {
        let events_obj = self.events_obj();
        let len = self.doc.length(&events_obj);
        let mut events = Vec::new();

        for i in since..len {
            if let Ok(Some((value, _))) = self.doc.get(&events_obj, i) {
                if let Value::Scalar(cow) = value {
                    if let ScalarValue::Bytes(buf) = cow.as_ref() {
                        if let Ok(event) = postcard::from_bytes::<InterfaceEvent<I>>(buf) {
                            events.push(event);
                        }
                    }
                }
            }
        }

        events
    }

    /// Get the number of events in the log
    pub fn event_count(&self) -> usize {
        let events_obj = self.events_obj();
        self.doc.length(&events_obj)
    }

    /// Fork this document (create an independent copy)
    pub fn fork(&mut self) -> Result<Self, SyncError> {
        let bytes = self.save();
        Self::load(&bytes)
    }

    /// Merge another document into this one
    pub fn merge(&mut self, other: &mut InterfaceDocument) -> Result<(), SyncError> {
        self.doc
            .merge(&mut other.doc)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        Ok(())
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
        let mut doc = InterfaceDocument::new();
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
        let mut doc = InterfaceDocument::new();
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
        let mut doc = InterfaceDocument::new();
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
        let mut doc1 = InterfaceDocument::new();
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
    fn test_merge_sync() {
        // Create initial document
        let mut doc1 = InterfaceDocument::new();
        let peer_a = SimulationIdentity::new('A').unwrap();
        doc1.add_member(&peer_a);

        // Save initial state and create doc2 from it
        let initial_bytes = doc1.save();
        let mut doc2 = InterfaceDocument::load(&initial_bytes).unwrap();

        // Add an event to doc1
        doc1.append_event(&InterfaceEvent::message(peer_a, 1, b"New message".to_vec()))
            .unwrap();

        // Use apply_update to sync doc1 -> doc2
        let doc1_bytes = doc1.save();
        doc2.apply_update(&doc1_bytes).unwrap();

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

    #[test]
    fn test_automerge_sync_protocol() {
        // Test the full automerge sync protocol with per-peer state.
        // Both docs must share a common base (fork) so their root objects
        // (members map, events list) are the same Automerge objects.
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();

        let mut doc1 = InterfaceDocument::new();
        doc1.add_member(&peer_a);
        doc1.add_member(&peer_b);

        // Fork so both docs share the same root structure
        let mut doc2 = doc1.fork().unwrap();

        // Each adds an event independently (simulating partition)
        doc1.append_event(&InterfaceEvent::message(peer_a, 1, b"From A".to_vec()))
            .unwrap();
        doc2.append_event(&InterfaceEvent::message(peer_b, 1, b"From B".to_vec()))
            .unwrap();

        // Sync using automerge sync protocol
        let mut state1 = automerge::sync::State::new();
        let mut state2 = automerge::sync::State::new();

        for _ in 0..20 {
            let msg1 = doc1.generate_sync_message(&mut state1);
            let msg2 = doc2.generate_sync_message(&mut state2);

            if msg1.is_none() && msg2.is_none() {
                break;
            }

            if let Some(msg) = msg1 {
                doc2.receive_sync_message(&mut state2, msg).unwrap();
            }
            if let Some(msg) = msg2 {
                doc1.receive_sync_message(&mut state1, msg).unwrap();
            }
        }

        // Both should have both events (and both members)
        assert_eq!(doc1.get_heads(), doc2.get_heads());
        assert_eq!(doc1.event_count(), 2);
        assert_eq!(doc2.event_count(), 2);
        assert!(doc1.is_member(&peer_a));
        assert!(doc1.is_member(&peer_b));
    }
}
