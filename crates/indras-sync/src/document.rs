//! Automerge document backing an N-peer interface
//!
//! The InterfaceDocument stores:
//! - Member list (who is in the interface)
//! - Interface metadata (name, description, settings)
//! - Event log (for Automerge-based causal ordering)

use std::collections::HashSet;

use automerge::{
    transaction::Transactable, AutoCommit, ChangeHash, ObjId, ObjType, ReadDoc, ScalarValue,
};
use indras_core::{InterfaceEvent, InterfaceMetadata, PeerIdentity};
use serde::{Deserialize, Serialize};

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
///     { serialized event bytes as base64 or similar }
///   ]
/// }
/// ```
pub struct InterfaceDocument {
    doc: AutoCommit,
    /// Root object IDs for quick access
    members_id: ObjId,
    metadata_id: ObjId,
    events_id: ObjId,
}

impl InterfaceDocument {
    /// Create a new empty document
    pub fn new() -> Self {
        let mut doc = AutoCommit::new();

        // Create root objects
        let members_id = doc
            .put_object(automerge::ROOT, keys::MEMBERS, ObjType::Map)
            .expect("Failed to create members map");

        let metadata_id = doc
            .put_object(automerge::ROOT, keys::METADATA, ObjType::Map)
            .expect("Failed to create metadata map");

        let events_id = doc
            .put_object(automerge::ROOT, keys::EVENTS, ObjType::List)
            .expect("Failed to create events list");

        Self {
            doc,
            members_id,
            metadata_id,
            events_id,
        }
    }

    /// Create from existing Automerge bytes
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError> {
        let doc = AutoCommit::load(bytes).map_err(|e| SyncError::DocumentLoad(e.to_string()))?;

        // Find root object IDs
        let members_id = doc
            .get(automerge::ROOT, keys::MEMBERS)
            .map_err(|e| SyncError::DocumentLoad(e.to_string()))?
            .and_then(|(_, obj_id)| Some(obj_id))
            .ok_or_else(|| SyncError::DocumentLoad("Missing members object".to_string()))?;

        let metadata_id = doc
            .get(automerge::ROOT, keys::METADATA)
            .map_err(|e| SyncError::DocumentLoad(e.to_string()))?
            .and_then(|(_, obj_id)| Some(obj_id))
            .ok_or_else(|| SyncError::DocumentLoad("Missing metadata object".to_string()))?;

        let events_id = doc
            .get(automerge::ROOT, keys::EVENTS)
            .map_err(|e| SyncError::DocumentLoad(e.to_string()))?
            .and_then(|(_, obj_id)| Some(obj_id))
            .ok_or_else(|| SyncError::DocumentLoad("Missing events object".to_string()))?;

        Ok(Self {
            doc,
            members_id,
            metadata_id,
            events_id,
        })
    }

    /// Export document as bytes
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    /// Get the current document heads (for sync protocol)
    pub fn heads(&mut self) -> Vec<ChangeHash> {
        self.doc.get_heads()
    }

    /// Get heads as raw bytes (32-byte arrays)
    pub fn heads_as_bytes(&mut self) -> Vec<[u8; 32]> {
        self.doc
            .get_heads()
            .into_iter()
            .map(|h| {
                let mut arr = [0u8; 32];
                // ChangeHash implements Into<[u8; 32]>
                arr.copy_from_slice(&h.0);
                arr
            })
            .collect()
    }

    /// Add a member to the interface
    pub fn add_member<I: PeerIdentity>(&mut self, peer: &I) {
        let peer_key = hex::encode(peer.as_bytes());
        let _ = self.doc.put(&self.members_id, peer_key, true);
    }

    /// Remove a member from the interface
    pub fn remove_member<I: PeerIdentity>(&mut self, peer: &I) {
        let peer_key = hex::encode(peer.as_bytes());
        let _ = self.doc.delete(&self.members_id, peer_key);
    }

    /// Check if a peer is a member
    pub fn is_member<I: PeerIdentity>(&self, peer: &I) -> bool {
        let peer_key = hex::encode(peer.as_bytes());
        self.doc
            .get(&self.members_id, peer_key)
            .ok()
            .flatten()
            .is_some()
    }

    /// Get all members
    pub fn members<I: PeerIdentity>(&self) -> HashSet<I> {
        let mut members = HashSet::new();

        for key in self.doc.keys(&self.members_id) {
            // key is the hex-encoded peer ID
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
        if let Some(name) = &metadata.name {
            let _ = self.doc.put(&self.metadata_id, keys::NAME, name.as_str());
        }
        if let Some(desc) = &metadata.description {
            let _ = self
                .doc
                .put(&self.metadata_id, keys::DESCRIPTION, desc.as_str());
        }
        let _ = self.doc.put(
            &self.metadata_id,
            keys::CREATED_AT,
            metadata.created_at.timestamp(),
        );
        let _ = self.doc.put(
            &self.metadata_id,
            keys::CREATOR,
            hex::encode(metadata.creator.as_bytes()),
        );
    }

    /// Append an event to the event log
    ///
    /// Events are serialized with postcard and stored as bytes in the list.
    /// Returns the Automerge ChangeHash for this change.
    pub fn append_event<I: PeerIdentity>(
        &mut self,
        event: &InterfaceEvent<I>,
    ) -> Result<ChangeHash, SyncError>
    where
        I: Serialize,
    {
        let event_bytes =
            postcard::to_allocvec(event).map_err(|e| SyncError::Serialization(e.to_string()))?;

        let len = self.doc.length(&self.events_id);
        self.doc
            .insert(&self.events_id, len, ScalarValue::Bytes(event_bytes))
            .map_err(|e| SyncError::DocumentOperation(e.to_string()))?;

        // Get the latest head after this change
        let heads = self.doc.get_heads();
        heads
            .into_iter()
            .last()
            .ok_or_else(|| SyncError::DocumentOperation("No heads after append".to_string()))
    }

    /// Get all events from the log
    pub fn events<I: PeerIdentity>(&self) -> Vec<InterfaceEvent<I>>
    where
        I: for<'de> Deserialize<'de>,
    {
        let mut events = Vec::new();
        let len = self.doc.length(&self.events_id);

        for i in 0..len {
            if let Ok(Some((value, _))) = self.doc.get(&self.events_id, i) {
                if let automerge::Value::Scalar(scalar) = value {
                    if let ScalarValue::Bytes(bytes) = scalar.as_ref() {
                        if let Ok(event) = postcard::from_bytes::<InterfaceEvent<I>>(bytes) {
                            events.push(event);
                        }
                    }
                }
            }
        }

        events
    }

    /// Get events since a specific index
    pub fn events_since<I: PeerIdentity>(&self, since: usize) -> Vec<InterfaceEvent<I>>
    where
        I: for<'de> Deserialize<'de>,
    {
        let mut events = Vec::new();
        let len = self.doc.length(&self.events_id);

        for i in since..len {
            if let Ok(Some((value, _))) = self.doc.get(&self.events_id, i) {
                if let automerge::Value::Scalar(scalar) = value {
                    if let ScalarValue::Bytes(bytes) = scalar.as_ref() {
                        if let Ok(event) = postcard::from_bytes::<InterfaceEvent<I>>(bytes) {
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
        self.doc.length(&self.events_id)
    }

    /// Generate a sync message for a peer given their known heads
    pub fn generate_sync_message(&mut self, their_heads: &[ChangeHash]) -> Vec<u8> {
        // Generate changes since their known heads
        self.doc.save_after(their_heads)
    }

    /// Apply a sync message (incremental changes) from a peer
    pub fn apply_sync_message(&mut self, changes: &[u8]) -> Result<(), SyncError> {
        self.doc
            .load_incremental(changes)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        Ok(())
    }

    /// Merge another document into this one
    pub fn merge(&mut self, other: &mut AutoCommit) -> Result<(), SyncError> {
        self.doc
            .merge(other)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        Ok(())
    }

    /// Fork this document (create an independent copy)
    pub fn fork(&mut self) -> Self {
        let doc = self.doc.fork();

        // Re-find object IDs in forked doc
        let members_id = doc
            .get(automerge::ROOT, keys::MEMBERS)
            .ok()
            .flatten()
            .map(|(_, id)| id)
            .unwrap_or(ObjId::Root);

        let metadata_id = doc
            .get(automerge::ROOT, keys::METADATA)
            .ok()
            .flatten()
            .map(|(_, id)| id)
            .unwrap_or(ObjId::Root);

        let events_id = doc
            .get(automerge::ROOT, keys::EVENTS)
            .ok()
            .flatten()
            .map(|(_, id)| id)
            .unwrap_or(ObjId::Root);

        Self {
            doc,
            members_id,
            metadata_id,
            events_id,
        }
    }

    /// Get access to the underlying AutoCommit for advanced operations
    pub fn inner(&self) -> &AutoCommit {
        &self.doc
    }

    /// Get mutable access to the underlying AutoCommit
    pub fn inner_mut(&mut self) -> &mut AutoCommit {
        &mut self.doc
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

        let _hash = doc.append_event(&event).unwrap();
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
    fn test_incremental_sync() {
        // Create initial document
        let mut doc1 = InterfaceDocument::new();
        let peer_a = SimulationIdentity::new('A').unwrap();
        doc1.add_member(&peer_a);

        // Save initial state and create doc2 from it
        let initial_bytes = doc1.save();
        let mut doc2 = InterfaceDocument::load(&initial_bytes).unwrap();

        // Get doc2's heads before any more changes
        let doc2_heads = doc2.heads();

        // Add an event to doc1
        doc1.append_event(&InterfaceEvent::message(peer_a, 1, b"New message".to_vec()))
            .unwrap();

        // Generate incremental sync
        let sync_msg = doc1.generate_sync_message(&doc2_heads);

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
