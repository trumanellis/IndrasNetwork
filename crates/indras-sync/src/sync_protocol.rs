//! Sync protocol for N-peer interfaces
//!
//! This module handles the synchronization protocol between peers,
//! using Automerge's built-in sync protocol with per-peer state.

use std::collections::HashMap;

use indras_core::{InterfaceId, PeerIdentity, SyncMessage};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::document::InterfaceDocument;
use crate::error::SyncError;

/// State of sync with a particular peer
///
/// Wraps an `automerge::sync::State` along with bookkeeping.
/// `sync::State` is not Clone or Serialize, so this struct has
/// manual implementations where needed.
pub struct PeerSyncState {
    /// Automerge per-peer sync state
    pub sync_state: automerge::sync::State,
    /// Whether we're awaiting a sync response
    pub awaiting_response: bool,
    /// Number of sync rounds completed
    pub rounds: u32,
}

impl Default for PeerSyncState {
    fn default() -> Self {
        Self {
            sync_state: automerge::sync::State::new(),
            awaiting_response: false,
            rounds: 0,
        }
    }
}

impl Clone for PeerSyncState {
    fn clone(&self) -> Self {
        // sync::State is not Clone — create a fresh one (will reconverge)
        Self {
            sync_state: automerge::sync::State::new(),
            awaiting_response: self.awaiting_response,
            rounds: self.rounds,
        }
    }
}

impl std::fmt::Debug for PeerSyncState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerSyncState")
            .field("awaiting_response", &self.awaiting_response)
            .field("rounds", &self.rounds)
            .finish_non_exhaustive()
    }
}

/// Manages sync state for multiple peers
pub struct SyncState<I: PeerIdentity> {
    /// Our interface ID
    interface_id: InterfaceId,
    /// Sync state per peer
    pub(crate) peer_states: HashMap<I, PeerSyncState>,
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
}

/// Protocol handler for interface synchronization using Automerge sync protocol
pub struct SyncProtocol;

impl SyncProtocol {
    /// Generate a sync message for a peer.
    ///
    /// Uses Automerge's built-in sync protocol. Returns `None` when
    /// the peer is fully up-to-date (nothing to send).
    #[instrument(skip(doc, sync_state, peer), fields(interface_id = %interface_id))]
    pub fn generate_sync_message<I: PeerIdentity>(
        interface_id: InterfaceId,
        doc: &mut InterfaceDocument,
        sync_state: &mut SyncState<I>,
        peer: &I,
    ) -> Option<SyncMessage> {
        let peer_sync = sync_state.peer_state(peer);
        let msg = doc.generate_sync_message(&mut peer_sync.sync_state)?;
        peer_sync.awaiting_response = true;
        Some(SyncMessage::request(interface_id, msg.encode(), vec![]))
    }

    /// Receive and process a sync message from a peer.
    ///
    /// Decodes the Automerge sync message and applies it to the document.
    #[instrument(skip(doc, sync_state, peer, msg), fields(interface_id = %msg.interface_id, sync_data_len = msg.sync_data.len()))]
    pub fn receive_sync_message<I: PeerIdentity>(
        doc: &mut InterfaceDocument,
        sync_state: &mut SyncState<I>,
        peer: &I,
        msg: SyncMessage,
    ) -> Result<(), SyncError> {
        if msg.sync_data.is_empty() {
            return Ok(());
        }
        let peer_sync = sync_state.peer_state(peer);
        let incoming = automerge::sync::Message::decode(&msg.sync_data)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))?;
        doc.receive_sync_message(&mut peer_sync.sync_state, incoming)?;
        peer_sync.awaiting_response = false;
        peer_sync.rounds += 1;
        Ok(())
    }

    /// Handle an incoming sync message and generate a response.
    ///
    /// Responder-side pattern: receive a message from a peer, then
    /// generate a response for them. Returns `None` when fully synced.
    #[instrument(skip(doc, sync_state, peer, incoming), fields(interface_id = %interface_id))]
    pub fn handle_sync_message<I: PeerIdentity>(
        interface_id: InterfaceId,
        doc: &mut InterfaceDocument,
        sync_state: &mut SyncState<I>,
        peer: &I,
        incoming: SyncMessage,
    ) -> Result<Option<SyncMessage>, SyncError> {
        Self::receive_sync_message(doc, sync_state, peer, incoming)?;
        Ok(Self::generate_sync_message(
            interface_id, doc, sync_state, peer,
        ))
    }

    /// Check if sync is complete with a peer.
    ///
    /// Sync is considered complete when we're not awaiting a response
    /// and we've completed at least one round.
    pub fn is_sync_complete<I: PeerIdentity>(
        sync_state: &SyncState<I>,
        peer: &I,
    ) -> bool {
        match sync_state.peer_states.get(peer) {
            Some(state) => !state.awaiting_response && state.rounds > 0,
            None => false,
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
        let _peer_state = state.peer_state(&peer_a);

        // Mark awaiting and verify
        state.mark_awaiting(&peer_a);
        assert!(state.is_awaiting(&peer_a));
    }

    #[test]
    fn test_sync_message_generation() {
        let id = test_interface_id();
        let mut doc = InterfaceDocument::new();
        let mut sync_state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();

        // First message should always be Some (initial sync)
        let msg =
            SyncProtocol::generate_sync_message(id, &mut doc, &mut sync_state, &peer_a);

        assert!(msg.is_some());
        let msg = msg.unwrap();
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
    fn test_peer_sync_state_default() {
        let state = PeerSyncState::default();
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
    fn test_is_sync_complete_unknown_peer() {
        let id = test_interface_id();
        let sync_state: SyncState<SimulationIdentity> = SyncState::new(id);
        let unknown = SimulationIdentity::new('X').unwrap();

        // Unknown peer should not be considered synced
        assert!(!SyncProtocol::is_sync_complete(&sync_state, &unknown));
    }

    #[test]
    fn test_is_sync_complete_awaiting_blocks() {
        let id = test_interface_id();
        let mut sync_state: SyncState<SimulationIdentity> = SyncState::new(id);
        let peer_a = SimulationIdentity::new('A').unwrap();

        // Peer exists but we're awaiting
        sync_state.peer_state(&peer_a);
        sync_state.mark_awaiting(&peer_a);
        assert!(!SyncProtocol::is_sync_complete(
            &sync_state,
            &peer_a
        ));
    }

    #[test]
    fn test_full_sync_protocol_roundtrip() {
        let id = test_interface_id();
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();

        // Create a shared base document, then fork so both share the same
        // root Automerge objects (members map, events list).
        let mut doc_a = InterfaceDocument::new();
        doc_a.add_member(&peer_a);
        doc_a.add_member(&peer_b);

        let mut doc_b = doc_a.fork().unwrap();

        // Each appends an event independently (simulating partition)
        doc_a
            .append_event(&indras_core::InterfaceEvent::message(
                peer_a,
                1,
                b"From A".to_vec(),
            ))
            .unwrap();

        doc_b
            .append_event(&indras_core::InterfaceEvent::message(
                peer_b,
                1,
                b"From B".to_vec(),
            ))
            .unwrap();

        let mut sync_a: SyncState<SimulationIdentity> = SyncState::new(id);
        let mut sync_b: SyncState<SimulationIdentity> = SyncState::new(id);

        // Sync loop
        for _ in 0..10 {
            let msg_a =
                SyncProtocol::generate_sync_message(id, &mut doc_a, &mut sync_a, &peer_b);
            let msg_b =
                SyncProtocol::generate_sync_message(id, &mut doc_b, &mut sync_b, &peer_a);

            if msg_a.is_none() && msg_b.is_none() {
                break;
            }

            if let Some(msg) = msg_a {
                SyncProtocol::receive_sync_message(&mut doc_b, &mut sync_b, &peer_a, msg)
                    .unwrap();
            }
            if let Some(msg) = msg_b {
                SyncProtocol::receive_sync_message(&mut doc_a, &mut sync_a, &peer_b, msg)
                    .unwrap();
            }
        }

        // Both should have both events
        assert_eq!(doc_a.event_count(), 2);
        assert_eq!(doc_b.event_count(), 2);
        assert_eq!(doc_a.get_heads(), doc_b.get_heads());
    }
}
