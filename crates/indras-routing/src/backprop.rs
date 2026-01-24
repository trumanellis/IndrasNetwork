//! Back-propagation for delivery confirmations
//!
//! When a packet is delivered, we need to confirm delivery back along
//! the path it took. This module manages that back-propagation process.
//!
//! ## How it works
//!
//! 1. When packet is delivered, `start_backprop` is called with the full path
//! 2. Confirmation travels backward: dest -> ... -> relay -> source
//! 3. `advance` is called as each hop confirms
//! 4. When all hops confirm, status becomes `Complete`
//! 5. If timeout occurs before completion, status becomes `TimedOut`

use std::time::{Duration, Instant};

use dashmap::DashMap;
use indras_core::{PacketId, PeerIdentity};
use tracing::{debug, info, instrument, warn};

/// State of an in-progress back-propagation
#[derive(Debug, Clone)]
pub struct BackPropState<I: PeerIdentity> {
    /// Full delivery path: [source, relay1, relay2, ..., dest]
    pub path: Vec<I>,
    /// Current position in back-propagation (starts at path.len() - 1, decrements)
    /// When current_hop == 0, we're at the source and backprop is complete
    pub current_hop: usize,
    /// When this backprop was started
    pub created_at: Instant,
    /// Timeout duration for this backprop
    pub timeout: Duration,
}

impl<I: PeerIdentity> BackPropState<I> {
    /// Create a new back-propagation state
    pub fn new(path: Vec<I>, timeout: Duration) -> Self {
        let current_hop = path.len().saturating_sub(1);
        Self {
            path,
            current_hop,
            created_at: Instant::now(),
            timeout,
        }
    }

    /// Check if this backprop has timed out
    pub fn is_timed_out(&self) -> bool {
        self.created_at.elapsed() > self.timeout
    }

    /// Get the peer that should confirm next
    pub fn next_confirmer(&self) -> Option<&I> {
        if self.current_hop > 0 {
            // Next confirmer is one hop before current
            Some(&self.path[self.current_hop - 1])
        } else {
            None
        }
    }

    /// Get the current peer in the path
    pub fn current_peer(&self) -> Option<&I> {
        self.path.get(self.current_hop)
    }

    /// Check if back-propagation is complete
    pub fn is_complete(&self) -> bool {
        self.current_hop == 0
    }

    /// Get elapsed time since creation
    pub fn elapsed(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Status of a back-propagation operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackPropStatus {
    /// All hops have confirmed delivery
    Complete,
    /// Back-propagation is in progress at the given hop index
    InProgress(usize),
    /// Back-propagation timed out before completion
    TimedOut,
    /// Packet ID not found (never started or already removed)
    NotFound,
}

/// Manages back-propagation of delivery confirmations
///
/// This tracks pending confirmations and advances them as peers confirm.
pub struct BackPropManager<I: PeerIdentity> {
    /// Pending back-propagations: packet_id -> state
    pending: DashMap<PacketId, BackPropState<I>>,
    /// Default timeout for back-propagation
    default_timeout: Duration,
}

impl<I: PeerIdentity> BackPropManager<I> {
    /// Create a new back-propagation manager
    ///
    /// # Arguments
    /// * `default_timeout` - Default timeout for back-propagation operations
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            pending: DashMap::new(),
            default_timeout,
        }
    }

    /// Start back-propagation for a delivered packet
    ///
    /// # Arguments
    /// * `packet_id` - The ID of the delivered packet
    /// * `path` - The full path the packet took: [source, relay1, ..., dest]
    #[instrument(skip(self, path), fields(packet_id = %packet_id, path_len = path.len()))]
    pub fn start_backprop(&self, packet_id: PacketId, path: Vec<I>) {
        if path.len() < 2 {
            debug!("No backprop needed for direct delivery");
            return;
        }
        let state = BackPropState::new(path, self.default_timeout);
        self.pending.insert(packet_id, state);
        debug!("Back-propagation started");
    }

    /// Start back-propagation with a custom timeout
    pub fn start_backprop_with_timeout(
        &self,
        packet_id: PacketId,
        path: Vec<I>,
        timeout: Duration,
    ) {
        if path.len() < 2 {
            return;
        }
        let state = BackPropState::new(path, timeout);
        self.pending.insert(packet_id, state);
    }

    /// Advance back-propagation when a peer confirms
    ///
    /// Returns the new status of the back-propagation.
    ///
    /// # Arguments
    /// * `packet_id` - The packet being confirmed
    /// * `confirming_peer` - The peer sending the confirmation
    #[instrument(skip(self), fields(packet_id = %packet_id, confirming_peer = %confirming_peer))]
    pub fn advance(&self, packet_id: &PacketId, confirming_peer: &I) -> BackPropStatus {
        let mut entry = match self.pending.get_mut(packet_id) {
            Some(e) => e,
            None => {
                debug!("Back-propagation not found");
                return BackPropStatus::NotFound;
            }
        };

        let state = entry.value_mut();

        // Check timeout first
        if state.is_timed_out() {
            warn!(elapsed_ms = ?state.elapsed().as_millis(), "Back-propagation timed out");
            drop(entry);
            self.pending.remove(packet_id);
            return BackPropStatus::TimedOut;
        }

        // Verify the confirming peer is the expected one
        if let Some(expected) = state.next_confirmer()
            && expected != confirming_peer
        {
            debug!(expected = %expected, "Wrong peer confirming, ignoring");
            return BackPropStatus::InProgress(state.current_hop);
        }

        // Advance to the next hop
        if state.current_hop > 0 {
            state.current_hop -= 1;
        }

        debug!(current_hop = state.current_hop, "Back-propagation advanced");

        // Check if complete
        if state.is_complete() {
            info!(elapsed_ms = ?state.elapsed().as_millis(), "Back-propagation complete");
            drop(entry);
            self.pending.remove(packet_id);
            BackPropStatus::Complete
        } else {
            BackPropStatus::InProgress(state.current_hop)
        }
    }

    /// Check for timed out back-propagations
    ///
    /// Returns the packet IDs that have timed out.
    /// Note: This does not remove them - call `remove` explicitly.
    pub fn check_timeouts(&self) -> Vec<PacketId> {
        self.pending
            .iter()
            .filter(|entry| entry.value().is_timed_out())
            .map(|entry| *entry.key())
            .collect()
    }

    /// Remove a back-propagation (after completion, timeout, or cancellation)
    pub fn remove(&self, packet_id: &PacketId) {
        self.pending.remove(packet_id);
    }

    /// Get the current state of a back-propagation
    pub fn get_state(&self, packet_id: &PacketId) -> Option<BackPropState<I>> {
        self.pending.get(packet_id).map(|r| r.value().clone())
    }

    /// Get the number of pending back-propagations
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Check if a back-propagation is pending for a packet
    pub fn is_pending(&self, packet_id: &PacketId) -> bool {
        self.pending.contains_key(packet_id)
    }

    /// Clear all pending back-propagations
    pub fn clear(&self) {
        self.pending.clear();
    }

    /// Get the status of a back-propagation without advancing it
    pub fn status(&self, packet_id: &PacketId) -> BackPropStatus {
        match self.pending.get(packet_id) {
            None => BackPropStatus::NotFound,
            Some(entry) => {
                let state = entry.value();
                if state.is_timed_out() {
                    BackPropStatus::TimedOut
                } else if state.is_complete() {
                    BackPropStatus::Complete
                } else {
                    BackPropStatus::InProgress(state.current_hop)
                }
            }
        }
    }
}

impl<I: PeerIdentity> Default for BackPropManager<I> {
    fn default() -> Self {
        // Default 30 second timeout
        Self::new(Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    fn make_path(chars: &str) -> Vec<SimulationIdentity> {
        chars
            .chars()
            .map(|c| SimulationIdentity::new(c).unwrap())
            .collect()
    }

    #[test]
    fn test_backprop_complete_flow() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_secs(60));

        let packet_id = PacketId::new(0x1234, 1);
        let path = make_path("ABCD"); // A -> B -> C -> D

        // Start backprop
        manager.start_backprop(packet_id, path.clone());
        assert!(manager.is_pending(&packet_id));

        // Initial state: current_hop = 3 (at D)
        let state = manager.get_state(&packet_id).unwrap();
        assert_eq!(state.current_hop, 3);
        assert!(!state.is_complete());

        // D confirms to C
        let c = SimulationIdentity::new('C').unwrap();
        let status = manager.advance(&packet_id, &c);
        assert_eq!(status, BackPropStatus::InProgress(2));

        // C confirms to B
        let b = SimulationIdentity::new('B').unwrap();
        let status = manager.advance(&packet_id, &b);
        assert_eq!(status, BackPropStatus::InProgress(1));

        // B confirms to A (source)
        let a = SimulationIdentity::new('A').unwrap();
        let status = manager.advance(&packet_id, &a);
        assert_eq!(status, BackPropStatus::Complete);

        // Should be removed after completion
        assert!(!manager.is_pending(&packet_id));
    }

    #[test]
    fn test_direct_delivery_no_backprop() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_secs(60));

        let packet_id = PacketId::new(0x1234, 1);
        let path = make_path("AB"); // Direct A -> B

        // Start backprop - but direct delivery shouldn't need it
        // Actually, A->B still needs one confirmation (B back to A)
        manager.start_backprop(packet_id, path);

        // For path length 2, we still track it
        assert!(manager.is_pending(&packet_id));

        let a = SimulationIdentity::new('A').unwrap();
        let status = manager.advance(&packet_id, &a);
        assert_eq!(status, BackPropStatus::Complete);
    }

    #[test]
    fn test_single_peer_path() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_secs(60));

        let packet_id = PacketId::new(0x1234, 1);
        let path = make_path("A"); // Single peer - no backprop needed

        manager.start_backprop(packet_id, path);

        // Should not be tracked
        assert!(!manager.is_pending(&packet_id));
    }

    #[test]
    fn test_timeout() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_millis(1));

        let packet_id = PacketId::new(0x1234, 1);
        let path = make_path("ABCD");

        manager.start_backprop(packet_id, path);

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(10));

        // Check timeouts
        let timed_out = manager.check_timeouts();
        assert!(timed_out.contains(&packet_id));

        // Advance should return TimedOut
        let c = SimulationIdentity::new('C').unwrap();
        let status = manager.advance(&packet_id, &c);
        assert_eq!(status, BackPropStatus::TimedOut);

        // Should be removed
        assert!(!manager.is_pending(&packet_id));
    }

    #[test]
    fn test_wrong_confirmer_ignored() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_secs(60));

        let packet_id = PacketId::new(0x1234, 1);
        let path = make_path("ABCD");

        manager.start_backprop(packet_id, path);

        // Wrong peer tries to confirm (should be C, not A)
        let a = SimulationIdentity::new('A').unwrap();
        let status = manager.advance(&packet_id, &a);

        // Should still be at the same hop
        assert_eq!(status, BackPropStatus::InProgress(3));
    }

    #[test]
    fn test_not_found() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_secs(60));

        let packet_id = PacketId::new(0x1234, 1);
        let a = SimulationIdentity::new('A').unwrap();

        let status = manager.advance(&packet_id, &a);
        assert_eq!(status, BackPropStatus::NotFound);
    }

    #[test]
    fn test_clear() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_secs(60));

        let path = make_path("ABCD");
        manager.start_backprop(PacketId::new(1, 1), path.clone());
        manager.start_backprop(PacketId::new(2, 1), path.clone());
        manager.start_backprop(PacketId::new(3, 1), path);

        assert_eq!(manager.pending_count(), 3);

        manager.clear();

        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_custom_timeout() {
        let manager: BackPropManager<SimulationIdentity> =
            BackPropManager::new(Duration::from_secs(60));

        let packet_id = PacketId::new(0x1234, 1);
        let path = make_path("ABCD");

        // Start with short custom timeout
        manager.start_backprop_with_timeout(packet_id, path, Duration::from_millis(1));

        std::thread::sleep(Duration::from_millis(10));

        let status = manager.status(&packet_id);
        assert_eq!(status, BackPropStatus::TimedOut);
    }
}
