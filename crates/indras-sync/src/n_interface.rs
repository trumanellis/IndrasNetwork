//! Full N-peer interface implementation
//!
//! The NInterface struct combines:
//! - [`InterfaceDocument`]: Yrs CRDT document for state synchronization
//! - [`EventStore`]: Store-and-forward event tracking with delivery confirmation
//! - [`SyncState`]: Peer synchronization state management
//!
//! This provides a complete implementation of the [`NInterfaceTrait`] from indras-core.

use std::collections::HashSet;
use std::sync::RwLock;

use async_trait::async_trait;
use indras_core::{
    EventId, InterfaceEvent, InterfaceId, PeerIdentity, SyncMessage,
    error::InterfaceError,
    traits::{NInterfaceTrait, TopicId},
};
use serde::{Deserialize, Serialize};

use crate::{EventStore, InterfaceDocument, SyncError, SyncState};

/// Full implementation of an N-peer interface
///
/// Combines InterfaceDocument (Yrs CRDT) + EventStore (pending tracking) + SyncState
/// to provide a complete implementation of the NInterfaceTrait.
///
/// ## Architecture
///
/// The NInterface manages three layers of data:
///
/// 1. **Yrs Document** (`InterfaceDocument`): The source of truth for membership,
///    metadata, and event history. Uses Yrs's CRDT properties for automatic
///    conflict resolution when peers sync.
///
/// 2. **Event Store** (`EventStore`): Tracks which events are pending delivery to which
///    peers. This enables store-and-forward: when a peer is offline, their pending
///    events accumulate and are delivered when they reconnect.
///
/// 3. **Sync State** (`SyncState`): Tracks the Yrs sync progress with each peer,
///    including their known state vectors and whether we're awaiting responses.
///
/// ## Usage
///
/// ```rust,ignore
/// use indras_sync::NInterface;
/// use indras_core::{SimulationIdentity, InterfaceEvent};
///
/// // Create a new interface with a creator
/// let alice = SimulationIdentity::new('A').unwrap();
/// let mut interface = NInterface::new(alice);
///
/// // Add another member
/// let bob = SimulationIdentity::new('B').unwrap();
/// interface.add_member(bob).unwrap();
///
/// // Append an event
/// let event = InterfaceEvent::message(alice, 1, b"Hello Bob!".to_vec());
/// interface.append(event).await.unwrap();
///
/// // Check pending events for Bob
/// let pending = interface.pending_for(&bob);
/// assert_eq!(pending.len(), 1);
/// ```
pub struct NInterface<I: PeerIdentity> {
    /// Unique identifier for this interface
    interface_id: InterfaceId,
    /// Yrs document for CRDT synchronization (wrapped in RwLock for thread-safe interior mutability)
    /// This is needed for concurrent access protection, even though Yrs methods take &self
    /// (Yrs uses interior mutability internally).
    document: RwLock<InterfaceDocument>,
    /// Event store for pending delivery tracking
    event_store: EventStore<I>,
    /// Sync state with peers
    sync_state: SyncState<I>,
    /// Current members (kept in sync with document)
    members: HashSet<I>,
}

impl<I: PeerIdentity> NInterface<I>
where
    I: Serialize + for<'de> Deserialize<'de>,
{
    /// Create a new interface with a creator
    ///
    /// The creator automatically becomes the first member of the interface.
    ///
    /// # Arguments
    ///
    /// * `creator` - The peer identity of the interface creator
    ///
    /// # Returns
    ///
    /// A new NInterface with a randomly generated InterfaceId
    pub fn new(creator: I) -> Self {
        let interface_id = InterfaceId::generate();
        let mut members = HashSet::new();
        members.insert(creator.clone());

        let document = InterfaceDocument::new();
        // Add creator to document members
        document.add_member(&creator);

        let event_store = EventStore::with_members(members.clone());
        let sync_state = SyncState::new(interface_id);

        Self {
            interface_id,
            document: RwLock::new(document),
            event_store,
            sync_state,
            members,
        }
    }

    /// Create with specific interface ID (for loading existing interfaces)
    ///
    /// Use this when you know the interface ID ahead of time, such as when
    /// joining an existing interface or loading from persistence.
    ///
    /// # Arguments
    ///
    /// * `interface_id` - The known InterfaceId
    /// * `creator` - The peer identity of the interface creator
    ///
    /// # Returns
    ///
    /// A new NInterface with the specified InterfaceId
    pub fn with_id(interface_id: InterfaceId, creator: I) -> Self {
        let mut members = HashSet::new();
        members.insert(creator.clone());

        let document = InterfaceDocument::new();
        document.add_member(&creator);

        let event_store = EventStore::with_members(members.clone());
        let sync_state = SyncState::new(interface_id);

        Self {
            interface_id,
            document: RwLock::new(document),
            event_store,
            sync_state,
            members,
        }
    }

    /// Load from existing Yrs document bytes
    ///
    /// Reconstructs an NInterface from previously saved document bytes.
    /// The members are loaded from the document.
    ///
    /// # Arguments
    ///
    /// * `interface_id` - The InterfaceId for this interface
    /// * `doc_bytes` - Yrs document bytes from a previous `save()` call
    ///
    /// # Returns
    ///
    /// The reconstructed NInterface, or an error if the bytes are invalid
    pub fn load(interface_id: InterfaceId, doc_bytes: &[u8]) -> Result<Self, SyncError> {
        let document = InterfaceDocument::load(doc_bytes)?;

        // Extract members from the document
        let members: HashSet<I> = document.members();

        let event_store = EventStore::with_members(members.clone());
        let sync_state = SyncState::new(interface_id);

        Ok(Self {
            interface_id,
            document: RwLock::new(document),
            event_store,
            sync_state,
            members,
        })
    }

    /// Get document bytes for persistence
    ///
    /// Serializes the Yrs document to bytes that can be saved to disk
    /// and later loaded with `load()`.
    ///
    /// # Returns
    ///
    /// The serialized document bytes
    pub fn save(&self) -> Result<Vec<u8>, SyncError> {
        Ok(self.document.read().map_err(|_| SyncError::LockPoisoned)?.save())
    }

    /// Add a member to the interface
    ///
    /// Adds the peer to both the Yrs document and the event store.
    /// New members will receive all existing events as pending.
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer identity to add
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an error if the member already exists
    pub fn add_member(&mut self, peer: I) -> Result<(), SyncError> {
        if self.members.contains(&peer) {
            return Ok(()); // Already a member, no-op
        }

        // Add to Yrs document
        self.document.write().map_err(|_| SyncError::LockPoisoned)?.add_member(&peer);

        // Add to local members set
        self.members.insert(peer.clone());

        // Add to event store (new member gets all existing events as pending)
        self.event_store.add_member(peer);

        Ok(())
    }

    /// Remove a member from the interface
    ///
    /// Removes the peer from both the Yrs document and the event store.
    /// Pending events for the removed peer are discarded.
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer identity to remove
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    pub fn remove_member(&mut self, peer: &I) -> Result<(), SyncError> {
        // Remove from Yrs document
        self.document.write().map_err(|_| SyncError::LockPoisoned)?.remove_member(peer);

        // Remove from local members set
        self.members.remove(peer);

        // Remove from event store
        self.event_store.remove_member(peer);

        // Remove from sync state
        self.sync_state.remove_peer(peer);

        Ok(())
    }

    /// Get the sync state (for external sync management)
    ///
    /// Provides read-only access to the sync state for inspection.
    pub fn sync_state(&self) -> &SyncState<I> {
        &self.sync_state
    }

    /// Get mutable sync state (for external sync management)
    ///
    /// Provides mutable access for updating sync state from external sources.
    pub fn sync_state_mut(&mut self) -> &mut SyncState<I> {
        &mut self.sync_state
    }

    /// Get the document (for direct Yrs operations)
    ///
    /// Provides access to the underlying Yrs document via RwLock.
    /// Returns a read guard for the document.
    pub fn document(&self) -> Result<std::sync::RwLockReadGuard<'_, InterfaceDocument>, SyncError> {
        self.document.read().map_err(|_| SyncError::LockPoisoned)
    }

    /// Get mutable document (for direct Yrs operations)
    ///
    /// Provides mutable access for advanced Yrs operations.
    /// Use with caution - modifications may desync the members set.
    /// Returns a write guard for the document.
    pub fn document_mut(&self) -> Result<std::sync::RwLockWriteGuard<'_, InterfaceDocument>, SyncError> {
        self.document.write().map_err(|_| SyncError::LockPoisoned)
    }

    /// Get the event store (for direct event operations)
    ///
    /// Provides read-only access to the event store.
    pub fn event_store(&self) -> &EventStore<I> {
        &self.event_store
    }

    /// Get mutable event store (for direct event operations)
    ///
    /// Provides mutable access for advanced event store operations.
    pub fn event_store_mut(&mut self) -> &mut EventStore<I> {
        &mut self.event_store
    }

    /// Synchronize the internal members set with the document
    ///
    /// Call this after merging external sync data to ensure
    /// the members set is up to date.
    pub fn sync_members(&mut self) -> Result<(), SyncError> {
        let doc_members: HashSet<I> = self.document.read().map_err(|_| SyncError::LockPoisoned)?.members();

        // Update event store members
        self.event_store.set_members(doc_members.clone());

        // Update local members set
        self.members = doc_members;
        Ok(())
    }
}

#[async_trait]
impl<I: PeerIdentity> NInterfaceTrait<I> for NInterface<I>
where
    I: Serialize + for<'de> Deserialize<'de>,
{
    /// Get the interface identifier
    fn id(&self) -> InterfaceId {
        self.interface_id
    }

    /// Get the gossip topic for this interface
    ///
    /// The topic ID is derived from the interface ID for gossip-based broadcast.
    fn topic_id(&self) -> TopicId {
        TopicId::from_interface(self.interface_id)
    }

    /// Get current members
    fn members(&self) -> HashSet<I> {
        self.members.clone()
    }

    /// Append an event to the log
    ///
    /// The event will be:
    /// 1. Added to the event store (tracks pending for other members)
    /// 2. Optionally added to the Yrs document for CRDT sync
    ///
    /// # Arguments
    ///
    /// * `event` - The event to append
    ///
    /// # Returns
    ///
    /// The EventId of the appended event
    async fn append(&mut self, event: InterfaceEvent<I>) -> Result<EventId, InterfaceError> {
        // 1. Append to event store (tracks pending for other members)
        let event_id = self.event_store.append(event.clone());

        // 2. Also add to Yrs document for CRDT sync
        self.document
            .write()
            .map_err(|_| InterfaceError::AppendFailed("Lock poisoned".to_string()))?
            .append_event(&event)
            .map_err(|e| InterfaceError::AppendFailed(e.to_string()))?;

        Ok(event_id)
    }

    /// Get events since a global sequence number
    ///
    /// Returns events in causal order since the given sequence.
    fn events_since(&self, since: u64) -> Vec<InterfaceEvent<I>> {
        self.event_store.since_owned(since)
    }

    /// Get pending events for an offline peer
    ///
    /// These are events that haven't been confirmed delivered to the peer.
    fn pending_for(&self, peer: &I) -> Vec<InterfaceEvent<I>> {
        self.event_store.pending_for_owned(peer)
    }

    /// Mark events as delivered to a peer
    ///
    /// Called when we receive confirmation that a peer has received events.
    fn mark_delivered(&mut self, peer: &I, up_to: EventId) {
        self.event_store.mark_delivered(peer, up_to);
    }

    /// Merge incoming sync state
    ///
    /// Applies a Yrs sync message from a peer, updating our document state.
    /// After merging, syncs the internal members set with the document.
    async fn merge_sync(&mut self, sync_msg: SyncMessage) -> Result<(), InterfaceError> {
        // Verify this sync is for our interface
        if sync_msg.interface_id != self.interface_id {
            return Err(InterfaceError::SyncFailed(
                "Sync message for different interface".to_string(),
            ));
        }

        // Apply the sync data if present
        if !sync_msg.sync_data.is_empty() {
            self.document
                .write()
                .map_err(|_| InterfaceError::SyncFailed("Lock poisoned".to_string()))?
                .apply_sync_message(&sync_msg.sync_data)
                .map_err(|e| InterfaceError::SyncFailed(e.to_string()))?;
        }

        // Sync members after merge (in case membership changed)
        self.sync_members().map_err(|e| InterfaceError::SyncFailed(e.to_string()))?;

        Ok(())
    }

    /// Generate sync state for a peer
    ///
    /// Creates a Yrs sync message to send to a peer for synchronization.
    fn generate_sync(&self, for_peer: &I) -> SyncMessage {
        // generate_sync cannot return Result per trait, unwrap is acceptable
        let doc = self.document.read().unwrap();

        // Get our current state vector
        let our_state_vector = doc.state_vector();

        // Get peer's known state vector from sync state
        let their_state_vector = self.sync_state.peer_state_vector(for_peer);

        // Generate sync data (changes they don't have)
        let sync_data = doc.generate_sync_message(&their_state_vector)
            .unwrap_or_default();

        SyncMessage::request(self.interface_id, sync_data, our_state_vector)
    }

    /// Get the current document state vector (for sync protocol)
    fn state_vector(&self) -> Vec<u8> {
        // state_vector cannot return Result per trait, unwrap is acceptable
        self.document.read().unwrap().state_vector()
    }

    /// Check if we have pending events for any peer
    fn has_pending(&self) -> bool {
        self.event_store.has_pending()
    }

    /// Get the total number of events in the log
    fn event_count(&self) -> usize {
        self.event_store.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    fn create_peers() -> (SimulationIdentity, SimulationIdentity, SimulationIdentity) {
        (
            SimulationIdentity::new('A').unwrap(),
            SimulationIdentity::new('B').unwrap(),
            SimulationIdentity::new('C').unwrap(),
        )
    }

    #[test]
    fn test_new_interface() {
        let (alice, _, _) = create_peers();
        let interface = NInterface::new(alice);

        assert!(interface.members().contains(&alice));
        assert_eq!(interface.members().len(), 1);
        assert_eq!(interface.event_count(), 0);
        assert!(!interface.has_pending());
    }

    #[test]
    fn test_with_id() {
        let (alice, _, _) = create_peers();
        let interface_id = InterfaceId::new([0x42; 32]);
        let interface = NInterface::with_id(interface_id, alice);

        assert_eq!(interface.id(), interface_id);
        assert!(interface.members().contains(&alice));
    }

    #[test]
    fn test_add_remove_member() {
        let (alice, bob, _) = create_peers();
        let mut interface = NInterface::new(alice);

        // Add Bob
        interface.add_member(bob).unwrap();
        assert!(interface.members().contains(&bob));
        assert_eq!(interface.members().len(), 2);

        // Document should also have Bob
        assert!(interface.document().unwrap().is_member(&bob));

        // Remove Bob
        interface.remove_member(&bob).unwrap();
        assert!(!interface.members().contains(&bob));
        assert_eq!(interface.members().len(), 1);
    }

    #[test]
    fn test_add_existing_member_is_noop() {
        let (alice, _, _) = create_peers();
        let mut interface = NInterface::new(alice);

        // Adding alice again should be a no-op
        interface.add_member(alice).unwrap();
        assert_eq!(interface.members().len(), 1);
    }

    #[tokio::test]
    async fn test_append_event() {
        let (alice, bob, _) = create_peers();
        let mut interface = NInterface::new(alice);
        interface.add_member(bob).unwrap();

        // Alice sends a message
        let event = InterfaceEvent::message(alice, 1, b"Hello Bob!".to_vec());
        let event_id = interface.append(event).await.unwrap();

        assert_eq!(interface.event_count(), 1);

        // Event should be in both event store and document
        assert_eq!(interface.document().unwrap().event_count(), 1);

        // Bob should have this event pending
        let pending = interface.pending_for(&bob);
        assert_eq!(pending.len(), 1);

        // Alice shouldn't have it pending (she sent it)
        let alice_pending = interface.pending_for(&alice);
        assert_eq!(alice_pending.len(), 0);

        // Verify event_id is returned
        assert_eq!(event_id.sequence, 1);
    }

    #[tokio::test]
    async fn test_mark_delivered() {
        let (alice, bob, _) = create_peers();
        let mut interface = NInterface::new(alice);
        interface.add_member(bob).unwrap();

        // Alice sends multiple messages
        let event1 = InterfaceEvent::message(alice, 1, b"Message 1".to_vec());
        let event2 = InterfaceEvent::message(alice, 2, b"Message 2".to_vec());
        let event3 = InterfaceEvent::message(alice, 3, b"Message 3".to_vec());

        interface.append(event1).await.unwrap();
        let id2 = interface.append(event2).await.unwrap();
        interface.append(event3).await.unwrap();

        // Bob has 3 pending
        assert_eq!(interface.pending_for(&bob).len(), 3);
        assert!(interface.has_pending());

        // Mark delivered up to message 2
        interface.mark_delivered(&bob, id2);

        // Bob should have 1 pending (message 3)
        let pending = interface.pending_for(&bob);
        assert_eq!(pending.len(), 1);

        match &pending[0] {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Message 3");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[tokio::test]
    async fn test_events_since() {
        let (alice, _, _) = create_peers();
        let mut interface = NInterface::new(alice);

        // Add some events
        interface
            .append(InterfaceEvent::message(alice, 1, b"First".to_vec()))
            .await
            .unwrap();
        interface
            .append(InterfaceEvent::message(alice, 2, b"Second".to_vec()))
            .await
            .unwrap();
        interface
            .append(InterfaceEvent::message(alice, 3, b"Third".to_vec()))
            .await
            .unwrap();

        // Get events since sequence 1
        let events = interface.events_since(1);
        assert_eq!(events.len(), 2); // Should get events 2 and 3
    }

    #[test]
    fn test_save_and_load() {
        let (alice, bob, _) = create_peers();
        let mut interface = NInterface::new(alice);
        interface.add_member(bob).unwrap();

        // Save the interface
        let interface_id = interface.id();
        let bytes = interface.save().unwrap();

        // Load it back
        let loaded: NInterface<SimulationIdentity> =
            NInterface::load(interface_id, &bytes).unwrap();

        assert_eq!(loaded.id(), interface_id);
        assert!(loaded.members().contains(&alice));
        assert!(loaded.members().contains(&bob));
        assert_eq!(loaded.members().len(), 2);
    }

    #[tokio::test]
    async fn test_save_load_with_events() {
        let (alice, bob, _) = create_peers();
        let mut interface = NInterface::new(alice);
        interface.add_member(bob).unwrap();

        // Add an event
        interface
            .append(InterfaceEvent::message(alice, 1, b"Hello".to_vec()))
            .await
            .unwrap();

        // Save and reload
        let interface_id = interface.id();
        let bytes = interface.save().unwrap();
        let loaded: NInterface<SimulationIdentity> =
            NInterface::load(interface_id, &bytes).unwrap();

        // Document should have the event
        assert_eq!(loaded.document().unwrap().event_count(), 1);

        let events: Vec<InterfaceEvent<SimulationIdentity>> = loaded.document().unwrap().events();
        match &events[0] {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Hello");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_topic_id() {
        let (alice, _, _) = create_peers();
        let interface = NInterface::new(alice);

        let topic_id = interface.topic_id();
        let expected_topic = TopicId::from_interface(interface.id());

        assert_eq!(topic_id, expected_topic);
    }

    #[test]
    fn test_generate_sync() {
        let (alice, bob, _) = create_peers();
        let interface = NInterface::new(alice);

        let sync_msg = interface.generate_sync(&bob);

        assert_eq!(sync_msg.interface_id, interface.id());
        assert!(sync_msg.is_request);
        // State vector should be present
        assert!(!sync_msg.state_vector.is_empty());
    }

    #[tokio::test]
    async fn test_merge_sync() {
        // Create two interfaces that will sync
        let (alice, bob, _) = create_peers();

        let mut interface1 = NInterface::new(alice);
        interface1.add_member(bob).unwrap();

        // Add an event to interface1
        interface1
            .append(InterfaceEvent::message(alice, 1, b"From Alice".to_vec()))
            .await
            .unwrap();

        // Save interface1 and create interface2 from it
        let interface_id = interface1.id();
        let bytes = interface1.save().unwrap();
        let mut interface2: NInterface<SimulationIdentity> =
            NInterface::load(interface_id, &bytes).unwrap();

        // Add event to interface2
        interface2
            .append(InterfaceEvent::message(bob, 1, b"From Bob".to_vec()))
            .await
            .unwrap();

        // Generate sync from interface2
        let sync_msg = interface2.generate_sync(&alice);

        // Merge into interface1
        interface1.merge_sync(sync_msg).await.unwrap();

        // Interface1 should now have Bob's event in the document
        let doc_events: Vec<InterfaceEvent<SimulationIdentity>> = interface1.document().unwrap().events();
        assert_eq!(doc_events.len(), 2);
    }

    #[tokio::test]
    async fn test_merge_sync_wrong_interface() {
        let (alice, _bob, _) = create_peers();
        let mut interface = NInterface::new(alice);

        // Create a sync message for a different interface
        let wrong_interface_id = InterfaceId::new([0xFF; 32]);
        let sync_msg = SyncMessage::request(wrong_interface_id, vec![], vec![]);

        // Should fail
        let result = interface.merge_sync(sync_msg).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_state_vector() {
        let (alice, _, _) = create_peers();
        let interface = NInterface::new(alice);

        let state_vector = interface.state_vector();
        // New document should have a state vector
        assert!(!state_vector.is_empty());
    }

    #[test]
    fn test_new_member_gets_all_pending_events() {
        let (alice, bob, charlie) = create_peers();
        let mut interface = NInterface::new(alice);
        interface.add_member(bob).unwrap();

        // Note: We can't use async in this test, so we use the event store directly
        // to simulate having events before Charlie joins
        let event = InterfaceEvent::message(alice, 1, b"Before Charlie".to_vec());
        interface.event_store.append(event);

        // Charlie joins
        interface.add_member(charlie).unwrap();

        // Charlie should have the existing event pending
        let pending = interface.pending_for(&charlie);
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn test_is_member_via_trait() {
        let (alice, bob, charlie) = create_peers();
        let mut interface = NInterface::new(alice);
        interface.add_member(bob).unwrap();

        // Use trait method
        assert!(interface.is_member(&alice));
        assert!(interface.is_member(&bob));
        assert!(!interface.is_member(&charlie));
    }

    #[tokio::test]
    async fn test_all_events_via_trait() {
        let (alice, _, _) = create_peers();
        let mut interface = NInterface::new(alice);

        interface
            .append(InterfaceEvent::message(alice, 1, b"First".to_vec()))
            .await
            .unwrap();
        interface
            .append(InterfaceEvent::message(alice, 2, b"Second".to_vec()))
            .await
            .unwrap();

        // Use trait method
        let all_events = interface.all_events();
        assert_eq!(all_events.len(), 2);
    }

    #[test]
    fn test_sync_members_after_document_change() {
        let (alice, bob, _) = create_peers();
        let mut interface = NInterface::new(alice);

        // Manually add bob to document (simulating external sync)
        interface.document_mut().unwrap().add_member(&bob);

        // Members set should not have bob yet
        assert!(!interface.members().contains(&bob));

        // Sync members
        interface.sync_members().unwrap();

        // Now bob should be in members
        assert!(interface.members().contains(&bob));
    }

    #[test]
    fn test_accessors() {
        let (alice, _, _) = create_peers();
        let mut interface = NInterface::new(alice);

        // Test all accessor methods compile and return correct types
        // Must drop guards before getting new ones due to borrowing rules
        {
            let _doc = interface.document().unwrap();
        }
        {
            let _doc_mut = interface.document_mut().unwrap();
        }
        let _sync = interface.sync_state();
        let _sync_mut = interface.sync_state_mut();
        let _events = interface.event_store();
        let _events_mut = interface.event_store_mut();
    }
}
