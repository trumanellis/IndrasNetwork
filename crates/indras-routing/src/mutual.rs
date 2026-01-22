//! Mutual peer tracking for store-and-forward routing
//!
//! The [`MutualPeerTracker`] computes and caches mutual peers between
//! connected peers. This is key for store-and-forward routing:
//!
//! When A wants to send to C but they're not directly connected,
//! we look for mutual peers B where: A-B and B-C exist.
//!
//! For group interfaces with N members, we compute the union of
//! all pairwise mutual peers.

use std::collections::HashSet;

use dashmap::DashMap;
use indras_core::{NetworkTopology, PeerIdentity};

/// Tracks mutual peers between connected peer pairs
///
/// When peers A and B connect, we compute:
/// `mutual_peers(A, B) = neighbors(A) ∩ neighbors(B)`
///
/// This enables efficient relay candidate lookup for store-and-forward routing.
pub struct MutualPeerTracker<I: PeerIdentity> {
    /// Cache of pairwise mutual peers: (a.as_bytes(), b.as_bytes()) -> mutual peers
    /// Keys are always ordered (smaller first) to avoid duplicates
    pairwise_mutuals: DashMap<(Vec<u8>, Vec<u8>), Vec<I>>,
}

impl<I: PeerIdentity> Default for MutualPeerTracker<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: PeerIdentity> MutualPeerTracker<I> {
    /// Create a new mutual peer tracker
    pub fn new() -> Self {
        Self {
            pairwise_mutuals: DashMap::new(),
        }
    }

    /// Create an ordered key pair for consistent cache lookups
    fn make_key(a: &I, b: &I) -> (Vec<u8>, Vec<u8>) {
        let a_bytes = a.as_bytes();
        let b_bytes = b.as_bytes();
        if a_bytes <= b_bytes {
            (a_bytes, b_bytes)
        } else {
            (b_bytes, a_bytes)
        }
    }

    /// Called when peers connect - computes and caches mutual peers
    ///
    /// This should be called whenever a new connection is established
    /// between two peers to update the mutual peer cache.
    pub fn on_connect<T: NetworkTopology<I>>(&self, a: &I, b: &I, topology: &T) {
        let key = Self::make_key(a, b);

        // Compute mutual peers using topology
        let mutuals = topology.mutual_peers(a, b);

        // Cache the result
        self.pairwise_mutuals.insert(key, mutuals);
    }

    /// Called when peers disconnect - removes cached mutual peers
    pub fn on_disconnect(&self, a: &I, b: &I) {
        let key = Self::make_key(a, b);
        self.pairwise_mutuals.remove(&key);
    }

    /// Get relay candidates for routing from source to destination
    ///
    /// Returns cached mutual peers between source and dest.
    /// If not cached, returns empty vec (caller should use topology directly).
    pub fn get_relays_for(&self, source: &I, dest: &I) -> Vec<I> {
        let key = Self::make_key(source, dest);
        self.pairwise_mutuals
            .get(&key)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Compute group mutual peers for N-peer interfaces
    ///
    /// For a group interface, we want all peers that could relay
    /// from `source` to any of the `group_members`.
    ///
    /// Group mutuals = UNION of mutual peers from all individuals:
    /// `union(mutual_peers(source, m) for m in group_members)`
    ///
    /// This ensures a message from source can reach any group member
    /// through the relay candidates.
    pub fn get_group_relays(&self, source: &I, group_members: &[I]) -> Vec<I> {
        let mut all_relays: HashSet<Vec<u8>> = HashSet::new();
        let mut relay_identities: Vec<I> = Vec::new();

        for member in group_members {
            // Skip self
            if member == source {
                continue;
            }

            let key = Self::make_key(source, member);
            if let Some(mutuals) = self.pairwise_mutuals.get(&key) {
                for relay in mutuals.value() {
                    let relay_bytes = relay.as_bytes();
                    if all_relays.insert(relay_bytes) {
                        relay_identities.push(relay.clone());
                    }
                }
            }
        }

        relay_identities
    }

    /// Update mutual peers for a specific pair using provided topology
    ///
    /// This is useful when the topology changes and we need to refresh
    /// the cached mutual peers.
    pub fn refresh<T: NetworkTopology<I>>(&self, a: &I, b: &I, topology: &T) {
        self.on_connect(a, b, topology);
    }

    /// Clear all cached mutual peers
    pub fn clear(&self) {
        self.pairwise_mutuals.clear();
    }

    /// Get the number of cached peer pairs
    pub fn len(&self) -> usize {
        self.pairwise_mutuals.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.pairwise_mutuals.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;
    use std::collections::HashMap;
    use std::sync::RwLock;

    /// Test topology for unit tests
    struct TestTopology {
        connections: HashMap<SimulationIdentity, Vec<SimulationIdentity>>,
        online: RwLock<HashSet<SimulationIdentity>>,
    }

    impl TestTopology {
        fn new() -> Self {
            Self {
                connections: HashMap::new(),
                online: RwLock::new(HashSet::new()),
            }
        }

        fn add_connection(&mut self, a: SimulationIdentity, b: SimulationIdentity) {
            self.connections.entry(a).or_default().push(b);
            self.connections.entry(b).or_default().push(a);
        }

        fn set_online(&self, peer: SimulationIdentity) {
            self.online.write().unwrap().insert(peer);
        }
    }

    impl NetworkTopology<SimulationIdentity> for TestTopology {
        fn peers(&self) -> Vec<SimulationIdentity> {
            self.connections.keys().cloned().collect()
        }

        fn neighbors(&self, peer: &SimulationIdentity) -> Vec<SimulationIdentity> {
            self.connections.get(peer).cloned().unwrap_or_default()
        }

        fn are_connected(&self, a: &SimulationIdentity, b: &SimulationIdentity) -> bool {
            self.connections
                .get(a)
                .map(|n| n.contains(b))
                .unwrap_or(false)
        }

        fn is_online(&self, peer: &SimulationIdentity) -> bool {
            self.online.read().unwrap().contains(peer)
        }
    }

    #[test]
    fn test_mutual_peer_tracking() {
        // Create topology: A-B-C where B is mutual peer of A and C
        //   A --- B --- C
        //   |           |
        //   +----- D ---+
        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();
        let d = SimulationIdentity::new('D').unwrap();

        topology.add_connection(a, b);
        topology.add_connection(b, c);
        topology.add_connection(a, d);
        topology.add_connection(c, d);

        let tracker: MutualPeerTracker<SimulationIdentity> = MutualPeerTracker::new();

        // Connect A and C - their mutual peers should be B and D
        tracker.on_connect(&a, &c, &topology);

        let relays = tracker.get_relays_for(&a, &c);
        assert_eq!(relays.len(), 2);
        assert!(relays.contains(&b) || relays.contains(&d));
    }

    #[test]
    fn test_key_ordering() {
        let a = SimulationIdentity::new('A').unwrap();
        let c = SimulationIdentity::new('C').unwrap();

        // Keys should be the same regardless of order
        let key1 = MutualPeerTracker::<SimulationIdentity>::make_key(&a, &c);
        let key2 = MutualPeerTracker::<SimulationIdentity>::make_key(&c, &a);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_on_disconnect() {
        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();

        topology.add_connection(a, b);
        topology.add_connection(b, c);

        let tracker: MutualPeerTracker<SimulationIdentity> = MutualPeerTracker::new();
        tracker.on_connect(&a, &c, &topology);

        assert!(!tracker.get_relays_for(&a, &c).is_empty());

        tracker.on_disconnect(&a, &c);
        assert!(tracker.get_relays_for(&a, &c).is_empty());
    }

    #[test]
    fn test_group_relays() {
        // Create topology for group scenario with overlapping mutual peers
        //   A --- B --- C
        //   |     |     |
        //   +---- D ----+
        //
        // Group members: B, C (A wants to send to both)
        // A's neighbors: B, D
        // B's neighbors: A, C, D
        // C's neighbors: B, D
        //
        // Relays for A->B: empty (A and B are neighbors, no mutual besides themselves)
        // Wait, that's not what mutual means. Mutual = neighbors(A) ∩ neighbors(B)
        // neighbors(A) = {B, D}, neighbors(B) = {A, C, D}
        // A->B mutual = {B, D} ∩ {A, C, D} = {D}
        //
        // A->C mutual: neighbors(A) ∩ neighbors(C) = {B, D} ∩ {B, D} = {B, D}
        //
        // Group relays for A->{B, C} = union({D}, {B, D}) = {B, D}
        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();
        let d = SimulationIdentity::new('D').unwrap();

        // Create a diamond/square topology
        topology.add_connection(a, b);
        topology.add_connection(b, c);
        topology.add_connection(a, d);
        topology.add_connection(b, d);
        topology.add_connection(c, d);

        let tracker: MutualPeerTracker<SimulationIdentity> = MutualPeerTracker::new();

        // Cache mutual peers
        tracker.on_connect(&a, &b, &topology);
        tracker.on_connect(&a, &c, &topology);

        let group_members = vec![b, c];
        let relays = tracker.get_group_relays(&a, &group_members);

        // Should have D and B as relays (union of A-B and A-C mutuals)
        assert!(!relays.is_empty(), "Should have relay candidates");
        // D should definitely be a relay (mutual of both A-B and A-C)
        assert!(relays.contains(&d), "D should be a relay candidate");
    }

    #[test]
    fn test_clear() {
        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();

        topology.add_connection(a, b);
        topology.add_connection(b, c);

        let tracker: MutualPeerTracker<SimulationIdentity> = MutualPeerTracker::new();
        tracker.on_connect(&a, &c, &topology);

        assert!(!tracker.is_empty());
        tracker.clear();
        assert!(tracker.is_empty());
    }
}
