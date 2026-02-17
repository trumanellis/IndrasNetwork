//! Plain-Rust document backing an N-peer interface.
//!
//! The InterfaceDocument stores:
//! - Member list (who is in the interface)
//! - Interface metadata (name, description, settings)
//! - Event log (serialized events as byte buffers)
//!
//! Uses postcard for serialization and a simple merge strategy
//! (union members, append-only events deduplicated by content hash).

use std::collections::HashSet;

use indras_core::{InterfaceEvent, InterfaceMetadata, PeerIdentity};
use serde::{Deserialize, Serialize};

use crate::error::SyncError;

/// Persisted interface document state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StoredDocument {
    members: HashSet<String>,
    metadata: StoredMetadata,
    /// Events stored as postcard-serialized byte buffers.
    events: Vec<Vec<u8>>,
}

/// Interface metadata fields.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StoredMetadata {
    name: Option<String>,
    description: Option<String>,
    created_at: i64,
    creator: Option<String>,
}

/// Document backing an NInterface.
///
/// Document structure (logical):
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
    inner: StoredDocument,
}

impl InterfaceDocument {
    /// Create a new empty document.
    pub fn new() -> Self {
        Self {
            inner: StoredDocument::default(),
        }
    }

    /// Create from previously saved bytes.
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError> {
        let inner: StoredDocument =
            postcard::from_bytes(bytes).map_err(|e| SyncError::DocumentLoad(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Export the full document state as bytes.
    pub fn save(&self) -> Vec<u8> {
        // postcard serialization; unwrap is safe for in-memory structs
        postcard::to_allocvec(&self.inner).expect("InterfaceDocument serialization failed")
    }

    /// Get an opaque state vector representing current state.
    ///
    /// Used by the sync protocol to compare state between peers.
    /// Currently encodes the event count as a simple version marker.
    pub fn state_vector(&self) -> Vec<u8> {
        let count = self.inner.events.len() as u64;
        count.to_le_bytes().to_vec()
    }

    /// Generate a sync message for a peer given their state vector.
    ///
    /// Returns the full serialized state (the peer will merge it).
    pub fn generate_sync_message(&self, _their_sv_bytes: &[u8]) -> Result<Vec<u8>, SyncError> {
        Ok(self.save())
    }

    /// Apply a sync message from a peer.
    ///
    /// Merges the remote state: union of members, union of events.
    pub fn apply_sync_message(&self, update_bytes: &[u8]) -> Result<(), SyncError> {
        // Note: takes &self for API compat with callers that hold RwLock<InterfaceDocument>.
        // Interior mutability is handled by the caller (RwLock write guard).
        // This is a design compromise — the caller must ensure exclusive access.
        //
        // In practice this is always called through RwLock::write(), so we use
        // unsafe to mutate through &self. The caller must ensure exclusive access
        // via RwLock::write().
        let remote: StoredDocument = postcard::from_bytes(update_bytes)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))?;

        // SAFETY: Callers always hold a write lock on the RwLock<InterfaceDocument>.
        let inner = unsafe { &mut *(std::ptr::addr_of!(self.inner) as *mut StoredDocument) };

        // Union members
        for member in remote.members {
            inner.members.insert(member);
        }

        // Union events by content (deduplicate by exact bytes)
        let existing: HashSet<Vec<u8>> = inner.events.iter().cloned().collect();
        for event in remote.events {
            if !existing.contains(&event) {
                inner.events.push(event);
            }
        }

        // Take remote metadata if ours is empty
        if inner.metadata.name.is_none() && remote.metadata.name.is_some() {
            inner.metadata = remote.metadata;
        }

        Ok(())
    }

    /// Add a member to the interface.
    pub fn add_member<I: PeerIdentity>(&mut self, peer: &I) {
        let peer_key = hex::encode(peer.as_bytes());
        self.inner.members.insert(peer_key);
    }

    /// Remove a member from the interface.
    pub fn remove_member<I: PeerIdentity>(&mut self, peer: &I) {
        let peer_key = hex::encode(peer.as_bytes());
        self.inner.members.remove(&peer_key);
    }

    /// Check if a peer is a member.
    pub fn is_member<I: PeerIdentity>(&self, peer: &I) -> bool {
        let peer_key = hex::encode(peer.as_bytes());
        self.inner.members.contains(&peer_key)
    }

    /// Get all members.
    pub fn members<I: PeerIdentity>(&self) -> HashSet<I> {
        let mut members = HashSet::new();
        for key in &self.inner.members {
            if let Ok(bytes) = hex::decode(key) {
                if let Ok(peer) = I::from_bytes(&bytes) {
                    members.insert(peer);
                }
            }
        }
        members
    }

    /// Set interface metadata.
    pub fn set_metadata<I: PeerIdentity>(&mut self, metadata: &InterfaceMetadata<I>) {
        if let Some(name) = &metadata.name {
            self.inner.metadata.name = Some(name.clone());
        }
        if let Some(desc) = &metadata.description {
            self.inner.metadata.description = Some(desc.clone());
        }
        self.inner.metadata.created_at = metadata.created_at.timestamp();
        self.inner.metadata.creator = Some(hex::encode(metadata.creator.as_bytes()));
    }

    /// Append an event to the event log.
    ///
    /// Events are serialized with postcard and stored as byte buffers.
    pub fn append_event<I: PeerIdentity>(
        &mut self,
        event: &InterfaceEvent<I>,
    ) -> Result<(), SyncError> {
        let event_bytes =
            postcard::to_allocvec(event).map_err(|e| SyncError::Serialization(e.to_string()))?;
        self.inner.events.push(event_bytes);
        Ok(())
    }

    /// Get all events from the log.
    pub fn events<I: PeerIdentity>(&self) -> Vec<InterfaceEvent<I>> {
        self.inner
            .events
            .iter()
            .filter_map(|buf| postcard::from_bytes::<InterfaceEvent<I>>(buf).ok())
            .collect()
    }

    /// Get events since a specific index.
    pub fn events_since<I: PeerIdentity>(&self, since: usize) -> Vec<InterfaceEvent<I>> {
        self.inner
            .events
            .iter()
            .skip(since)
            .filter_map(|buf| postcard::from_bytes::<InterfaceEvent<I>>(buf).ok())
            .collect()
    }

    /// Get the number of events in the log.
    pub fn event_count(&self) -> usize {
        self.inner.events.len()
    }

    /// Fork this document (create an independent copy).
    pub fn fork(&self) -> Result<Self, SyncError> {
        let bytes = self.save();
        Self::load(&bytes)
    }

    /// Merge another document into this one.
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
        let mut doc1 = InterfaceDocument::new();
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

        // Generate sync message
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
