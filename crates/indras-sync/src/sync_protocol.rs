//! Sync protocol for N-peer interfaces
//!
//! This module handles the synchronization protocol between peers,
//! combining Automerge document sync with store-and-forward event delivery.

use std::collections::HashMap;

use automerge::ChangeHash;
use indras_core::{InterfaceId, PeerIdentity, SyncMessage};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::document::InterfaceDocument;
use crate::error::SyncError;

/// State of sync with a particular peer
#[derive(Debug, Clone, Default)]
pub struct PeerSyncState {
    /// Their last known document heads
    pub their_heads: Vec<ChangeHash>,
    /// Whether we're awaiting a sync response
    pub awaiting_response: bool,
    /// Number of sync rounds completed
    pub rounds: u32,
}

/// Manages sync state for multiple peers
pub struct SyncState<I: PeerIdentity> {
    /// Our interface ID
    interface_id: InterfaceId,
    /// Sync state per peer
    peer_states: HashMap<I, PeerSyncState>,
}

impl<I: PeerIdentity> SyncState<I> {
    /// Create a new sync state manager
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            peer_states: HashMap::new(),
        }
    }

    /// Get the interface ID this sync state is for
    pub fn interface_id(&self) -> InterfaceId {
        self.interface_id
    }

    /// Get or create sync state for a peer
    pub fn peer_state(&mut self, peer: &I) -> &mut PeerSyncState {
        self.peer_states.entry(peer.clone()).or_default()
    }

    /// Update peer's known heads
    pub fn update_peer_heads(&mut self, peer: &I, heads: Vec<ChangeHash>) {
        let state = self.peer_states.entry(peer.clone()).or_default();
        state.their_heads = heads;
        state.awaiting_response = false;
        state.rounds += 1;
    }

    /// Mark that we're awaiting a response from a peer
    pub fn mark_awaiting(&mut self, peer: &I) {
        if let Some(state) = self.peer_states.get_mut(peer) {
            state.awaiting_response = true;
        }
    }

    /// Check if we're awaiting a response from a peer
    pub fn is_awaiting(&self, peer: &I) -> bool {
        self.peer_states
            .get(peer)
            .map(|s| s.awaiting_response)
            .unwrap_or(false)
    }

    /// Get the number of completed sync rounds with a peer
    pub fn rounds(&self, peer: &I) -> u32 {
        self.peer_states.get(peer).map(|s| s.rounds).unwrap_or(0)
    }

    /// Remove a peer from sync tracking
    pub fn remove_peer(&mut self, peer: &I) {
        self.peer_states.remove(peer);
    }

    /// Get all tracked peers
    pub fn peers(&self) -> Vec<&I> {
        self.peer_states.keys().collect()
    }

    /// Get peer's known heads (returns empty if peer not tracked)
    pub fn peer_heads(&self, peer: &I) -> Vec<ChangeHash> {
        self.peer_states
            .get(peer)
            .map(|s| s.their_heads.clone())
            .unwrap_or_default()
    }
}

/// Convert byte array to ChangeHash
fn bytes_to_change_hash(bytes: &[u8; 32]) -> ChangeHash {
    ChangeHash(*bytes)
}

/// Protocol handler for interface synchronization
pub struct SyncProtocol;

impl SyncProtocol {
    /// Generate a sync request for a peer
    ///
    /// This creates a SyncMessage that can be sent to a peer to request
    /// their latest changes.
    #[instrument(skip(doc, sync_state, peer), fields(interface_id = %interface_id))]
    pub fn generate_sync_request<I: PeerIdentity>(
        interface_id: InterfaceId,
        doc: &mut InterfaceDocument,
        sync_state: &mut SyncState<I>,
        peer: &I,
    ) -> SyncMessage {
        let heads = doc.heads_as_bytes();
        let peer_state = sync_state.peer_state(peer);

        // Convert their known heads to ChangeHash for generating sync data
        let their_change_heads: Vec<ChangeHash> = peer_state.their_heads.to_vec();

        // Generate sync data (changes they don't have)
        let sync_data = doc.generate_sync_message(&their_change_heads);

        sync_state.mark_awaiting(peer);

        SyncMessage::request(interface_id, sync_data, heads)
    }

    /// Generate a sync response for a peer
    ///
    /// Called when we receive a sync request from a peer.
    #[instrument(skip(doc, their_heads), fields(interface_id = %interface_id, their_heads_count = their_heads.len()))]
    pub fn generate_sync_response<I: PeerIdentity>(
        interface_id: InterfaceId,
        doc: &mut InterfaceDocument,
        their_heads: &[[u8; 32]],
    ) -> SyncMessage {
        let heads = doc.heads_as_bytes();

        // Convert their heads to ChangeHash
        let their_change_heads: Vec<ChangeHash> =
            their_heads.iter().map(bytes_to_change_hash).collect();

        // Generate changes they don't have
        let sync_data = doc.generate_sync_message(&their_change_heads);

        SyncMessage::response(interface_id, sync_data, heads)
    }

    /// Process an incoming sync message
    ///
    /// Returns true if the sync resulted in changes to our document.
    #[instrument(skip(doc, sync_state, peer, msg), fields(interface_id = %msg.interface_id, is_request = msg.is_request, sync_data_len = msg.sync_data.len()))]
    pub fn process_sync_message<I: PeerIdentity>(
        doc: &mut InterfaceDocument,
        sync_state: &mut SyncState<I>,
        peer: &I,
        msg: SyncMessage,
    ) -> Result<bool, SyncError> {
        // Update peer's known heads
        let their_heads: Vec<ChangeHash> = msg.heads.iter().map(bytes_to_change_hash).collect();

        sync_state.update_peer_heads(peer, their_heads);

        // Apply their changes if any
        if !msg.sync_data.is_empty() {
            let before_heads = doc.heads();
            doc.apply_sync_message(&msg.sync_data)?;
            let after_heads = doc.heads();

            // Check if we got new changes
            Ok(before_heads != after_heads)
        } else {
            Ok(false)
        }
    }

    /// Check if sync is complete with a peer (bidirectional).
    ///
    /// Sync is complete when:
    /// 1. All their heads are in our document (we have everything they have)
    /// 2. We're not awaiting a response from them
    pub fn is_sync_complete<I: PeerIdentity>(
        doc: &mut InterfaceDocument,
        sync_state: &SyncState<I>,
        peer: &I,
    ) -> bool {
        if let Some(state) = sync_state.peer_states.get(peer) {
            let our_heads = doc.heads();
            let their_heads = &state.their_heads;

            // Check: all their heads are in our document
            let we_have_theirs = their_heads.iter().all(|h| our_heads.contains(h));

            // Also check we're not still waiting for their response
            we_have_theirs && !state.awaiting_response
        } else {
            false
        }
    }
}

/// Stored events for offline delivery
///
/// This structure holds encrypted events that need to be delivered
/// to a peer when they come online.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDelivery {
    /// Interface the events belong to
    pub interface_id: InterfaceId,
    /// Encrypted event payloads
    pub encrypted_events: Vec<Vec<u8>>,
}

impl PendingDelivery {
    /// Create a new pending delivery batch
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            encrypted_events: Vec::new(),
        }
    }

    /// Add an encrypted event to the batch
    pub fn add(&mut self, encrypted: Vec<u8>) {
        self.encrypted_events.push(encrypted);
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.encrypted_events.is_empty()
    }

    /// Get the number of pending events
    pub fn len(&self) -> usize {
        self.encrypted_events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    fn test_interface_id() -> InterfaceId {
        InterfaceId::new([0x42; 32])
    }

    #[test]
    fn test_sync_state_creation() {
        let id = test_interface_id();
        let state: SyncState<SimulationIdentity> = SyncState::new(id);
        assert!(state.peers().is_empty());
    }

    #[test]
    fn test_peer_state_tracking() {
        let id = test_interface_id();
        let mut state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();

        // Initially no state
        assert_eq!(state.rounds(&peer_a), 0);

        // Create peer state
        let peer_state = state.peer_state(&peer_a);
        assert!(peer_state.their_heads.is_empty());

        // Mark awaiting and verify
        state.mark_awaiting(&peer_a);
        assert!(state.is_awaiting(&peer_a));

        // Update heads (simulates receiving sync)
        state.update_peer_heads(&peer_a, vec![]);
        assert!(!state.is_awaiting(&peer_a));
        assert_eq!(state.rounds(&peer_a), 1);
    }

    #[test]
    fn test_sync_request_generation() {
        let id = test_interface_id();
        let mut doc = InterfaceDocument::new();
        let mut sync_state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();

        let msg = SyncProtocol::generate_sync_request(id, &mut doc, &mut sync_state, &peer_a);

        assert_eq!(msg.interface_id, id);
        assert!(msg.is_request);
        assert!(sync_state.is_awaiting(&peer_a));
    }

    #[test]
    fn test_pending_delivery() {
        let id = test_interface_id();
        let mut pending = PendingDelivery::new(id);

        assert!(pending.is_empty());

        pending.add(vec![1, 2, 3]);
        pending.add(vec![4, 5, 6]);

        assert!(!pending.is_empty());
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_sync_state_multi_peer_tracking() {
        let id = test_interface_id();
        let mut state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();
        let peer_c = SimulationIdentity::new('C').unwrap();

        // Create state for multiple peers
        state.peer_state(&peer_a);
        state.peer_state(&peer_b);
        state.peer_state(&peer_c);

        assert_eq!(state.peers().len(), 3);

        // Update different peers
        state.update_peer_heads(&peer_a, vec![]);
        state.update_peer_heads(&peer_b, vec![]);

        assert_eq!(state.rounds(&peer_a), 1);
        assert_eq!(state.rounds(&peer_b), 1);
        assert_eq!(state.rounds(&peer_c), 0);
    }

    #[test]
    fn test_sync_state_remove_peer() {
        let id = test_interface_id();
        let mut state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();

        state.peer_state(&peer_a);
        state.peer_state(&peer_b);
        assert_eq!(state.peers().len(), 2);

        state.remove_peer(&peer_a);
        assert_eq!(state.peers().len(), 1);

        // Removed peer should have default state
        assert_eq!(state.rounds(&peer_a), 0);
        assert!(!state.is_awaiting(&peer_a));
    }

    #[test]
    fn test_sync_state_peer_heads() {
        let id = test_interface_id();
        let mut state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();

        // Unknown peer should return empty heads
        assert!(state.peer_heads(&peer_a).is_empty());

        // Create peer state
        state.peer_state(&peer_a);
        assert!(state.peer_heads(&peer_a).is_empty());

        // Create fake change hashes
        let hash1 = ChangeHash([0x11; 32]);
        let hash2 = ChangeHash([0x22; 32]);

        state.update_peer_heads(&peer_a, vec![hash1, hash2]);

        let heads = state.peer_heads(&peer_a);
        assert_eq!(heads.len(), 2);
        assert!(heads.contains(&hash1));
        assert!(heads.contains(&hash2));
    }

    #[test]
    fn test_sync_state_interface_id() {
        let id = test_interface_id();
        let state: SyncState<SimulationIdentity> = SyncState::new(id);
        assert_eq!(state.interface_id(), id);
    }

    #[test]
    fn test_sync_state_awaiting_unknown_peer() {
        let id = test_interface_id();
        let state: SyncState<SimulationIdentity> = SyncState::new(id);
        let unknown_peer = SimulationIdentity::new('X').unwrap();

        // Awaiting check for unknown peer should return false
        assert!(!state.is_awaiting(&unknown_peer));
    }

    #[test]
    fn test_sync_state_mark_awaiting_unknown_peer() {
        let id = test_interface_id();
        let mut state: SyncState<SimulationIdentity> = SyncState::new(id);
        let unknown_peer = SimulationIdentity::new('X').unwrap();

        // Marking awaiting for unknown peer should be a no-op
        state.mark_awaiting(&unknown_peer);
        assert!(!state.is_awaiting(&unknown_peer));
    }

    #[test]
    fn test_sync_response_generation() {
        let id = test_interface_id();
        let mut doc = InterfaceDocument::new();

        // Empty heads array
        let msg = SyncProtocol::generate_sync_response::<SimulationIdentity>(id, &mut doc, &[]);

        assert_eq!(msg.interface_id, id);
        assert!(!msg.is_request); // It's a response
    }

    #[test]
    fn test_sync_response_with_known_heads() {
        let id = test_interface_id();
        let mut doc = InterfaceDocument::new();

        // Provide some fake heads
        let their_heads = [[0x11; 32], [0x22; 32]];
        let msg =
            SyncProtocol::generate_sync_response::<SimulationIdentity>(id, &mut doc, &their_heads);

        assert_eq!(msg.interface_id, id);
        assert!(!msg.is_request);
    }

    #[test]
    fn test_peer_sync_state_default() {
        let state = PeerSyncState::default();
        assert!(state.their_heads.is_empty());
        assert!(!state.awaiting_response);
        assert_eq!(state.rounds, 0);
    }

    #[test]
    fn test_pending_delivery_interface_id() {
        let id = test_interface_id();
        let pending = PendingDelivery::new(id);
        assert_eq!(pending.interface_id, id);
    }

    #[test]
    fn test_sync_state_multiple_rounds() {
        let id = test_interface_id();
        let mut state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();

        state.peer_state(&peer_a);

        // Simulate multiple sync rounds
        for expected_round in 1..=5 {
            state.update_peer_heads(&peer_a, vec![]);
            assert_eq!(state.rounds(&peer_a), expected_round);
        }
    }

    #[test]
    fn test_bytes_to_change_hash() {
        let bytes = [0xab; 32];
        let hash = bytes_to_change_hash(&bytes);
        assert_eq!(hash.0, bytes);
    }

    #[test]
    fn test_is_sync_complete_unknown_peer() {
        let id = test_interface_id();
        let mut doc = InterfaceDocument::new();
        let sync_state: SyncState<SimulationIdentity> = SyncState::new(id);
        let unknown = SimulationIdentity::new('X').unwrap();

        // Unknown peer should not be considered synced
        assert!(!SyncProtocol::is_sync_complete(&mut doc, &sync_state, &unknown));
    }

    #[test]
    fn test_is_sync_complete_awaiting_blocks() {
        let id = test_interface_id();
        let mut doc = InterfaceDocument::new();
        let mut sync_state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();

        // Peer has empty heads (matches our empty doc) but we're awaiting
        sync_state.peer_state(&peer_a);
        sync_state.mark_awaiting(&peer_a);
        assert!(!SyncProtocol::is_sync_complete(&mut doc, &sync_state, &peer_a));

        // Clear awaiting by updating heads
        sync_state.update_peer_heads(&peer_a, vec![]);
        assert!(SyncProtocol::is_sync_complete(&mut doc, &sync_state, &peer_a));
    }
}
