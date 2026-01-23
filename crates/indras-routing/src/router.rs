//! Store-and-forward router implementation
//!
//! The [`StoreForwardRouter`] implements the core routing logic for
//! Indras Network. It uses mutual peer tracking for relay selection
//! and supports store-and-forward delivery for offline peers.
//!
//! ## Routing Algorithm
//!
//! 1. **DIRECT**: If destination is online and directly connected, deliver directly
//! 2. **HOLD**: If destination is offline but directly connected, store for later
//! 3. **RELAY**: If not directly connected, use mutual peers as relay candidates
//! 4. **DROP**: If no route is available, drop the packet

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use indras_core::{
    DropReason, NetworkTopology, Packet, PacketStore, PeerIdentity, Router, RoutingDecision,
};
use tracing::{debug, info, info_span, instrument, trace, warn};

use crate::backprop::BackPropManager;
use crate::error::RoutingError;
use crate::mutual::MutualPeerTracker;
use crate::table::RoutingTable;

/// Store-and-forward router
///
/// This router implements the core routing logic for Indras Network:
/// - Direct delivery to online, connected peers
/// - Store-and-forward for offline peers
/// - Relay through mutual peers when not directly connected
/// - Back-propagation of delivery confirmations
pub struct StoreForwardRouter<I, T, S>
where
    I: PeerIdentity,
    T: NetworkTopology<I>,
    S: PacketStore<I>,
{
    /// Network topology for connectivity information
    topology: Arc<T>,
    /// Packet storage for store-and-forward
    storage: Arc<S>,
    /// Mutual peer tracker for relay selection
    mutual_tracker: MutualPeerTracker<I>,
    /// Routing table for route caching
    routing_table: RoutingTable<I>,
    /// Back-propagation manager for delivery confirmations
    backprop: BackPropManager<I>,
}

impl<I, T, S> StoreForwardRouter<I, T, S>
where
    I: PeerIdentity,
    T: NetworkTopology<I>,
    S: PacketStore<I>,
{
    /// Create a new store-forward router
    ///
    /// # Arguments
    /// * `topology` - Network topology for connectivity information
    /// * `storage` - Packet storage for store-and-forward
    pub fn new(topology: Arc<T>, storage: Arc<S>) -> Self {
        Self {
            topology,
            storage,
            mutual_tracker: MutualPeerTracker::new(),
            routing_table: RoutingTable::new(Duration::from_secs(300)),
            backprop: BackPropManager::new(Duration::from_secs(30)),
        }
    }

    /// Create a router with custom timeouts
    pub fn with_timeouts(
        topology: Arc<T>,
        storage: Arc<S>,
        route_stale_timeout: Duration,
        backprop_timeout: Duration,
    ) -> Self {
        Self {
            topology,
            storage,
            mutual_tracker: MutualPeerTracker::new(),
            routing_table: RoutingTable::new(route_stale_timeout),
            backprop: BackPropManager::new(backprop_timeout),
        }
    }

    /// Get a reference to the mutual peer tracker
    pub fn mutual_tracker(&self) -> &MutualPeerTracker<I> {
        &self.mutual_tracker
    }

    /// Get a reference to the routing table
    pub fn routing_table(&self) -> &RoutingTable<I> {
        &self.routing_table
    }

    /// Get a reference to the back-propagation manager
    pub fn backprop(&self) -> &BackPropManager<I> {
        &self.backprop
    }

    /// Notify the router that two peers have connected
    ///
    /// This updates the mutual peer cache for routing decisions.
    #[instrument(skip(self), fields(peer_a = %a, peer_b = %b))]
    pub fn on_peer_connect(&self, a: &I, b: &I) {
        debug!("Updating mutual peer cache for new connection");
        self.mutual_tracker.on_connect(a, b, self.topology.as_ref());
    }

    /// Notify the router that two peers have disconnected
    #[instrument(skip(self), fields(peer_a = %a, peer_b = %b))]
    pub fn on_peer_disconnect(&self, a: &I, b: &I) {
        debug!("Removing mutual peer cache for disconnection");
        self.mutual_tracker.on_disconnect(a, b);
    }

    /// Store a packet for later delivery
    ///
    /// Used when the destination is offline but we can reach them when they come online.
    #[instrument(skip(self, packet), fields(packet_id = %packet.id, destination = %packet.destination))]
    pub async fn store_packet(&self, packet: Packet<I>) -> Result<(), RoutingError> {
        debug!("Storing packet for offline peer");
        self.storage
            .store(packet)
            .await
            .map_err(|_| RoutingError::StorageFailed)
    }

    /// Get pending packets for a destination
    ///
    /// Called when a peer comes online to deliver stored packets.
    #[instrument(skip(self), fields(destination = %dest))]
    pub async fn get_pending(&self, dest: &I) -> Result<Vec<Packet<I>>, RoutingError> {
        let result = self.storage
            .pending_for(dest)
            .await
            .map_err(|_| RoutingError::StorageFailed);

        if let Ok(ref packets) = result {
            debug!(count = packets.len(), "Retrieved pending packets");
        }
        result
    }

    /// Delete a packet from storage (after delivery)
    pub async fn delete_packet(&self, packet: &Packet<I>) -> Result<(), RoutingError> {
        self.storage
            .delete(&packet.id)
            .await
            .map_err(|_| RoutingError::StorageFailed)
    }

    /// Start back-propagation for a delivered packet
    #[instrument(skip(self, packet, path), fields(packet_id = %packet.id, path_len = path.len()))]
    pub fn start_backprop(&self, packet: &Packet<I>, path: Vec<I>) {
        info!("Starting back-propagation");
        self.backprop.start_backprop(packet.id, path);
    }

    /// Check for timed-out back-propagations
    pub fn check_backprop_timeouts(&self) -> Vec<indras_core::PacketId> {
        self.backprop.check_timeouts()
    }

    /// Prune stale routes from the routing table
    pub fn prune_stale_routes(&self) {
        self.routing_table.prune_stale();
    }

    /// Filter relay candidates to only online, unvisited peers
    fn filter_relays(&self, relays: Vec<I>, packet: &Packet<I>) -> Vec<I> {
        relays
            .into_iter()
            .filter(|r| {
                // Must be online
                if !self.topology.is_online(r) {
                    trace!(relay = %r, "Relay not online, skipping");
                    return false;
                }
                // Must not have been visited
                if packet.was_visited(r) {
                    trace!(relay = %r, "Relay already visited, skipping");
                    return false;
                }
                true
            })
            .collect()
    }
}

#[async_trait]
impl<I, T, S> Router<I> for StoreForwardRouter<I, T, S>
where
    I: PeerIdentity,
    T: NetworkTopology<I>,
    S: PacketStore<I>,
{
    /// Make a routing decision for a packet
    ///
    /// Implements the store-and-forward routing algorithm:
    /// 1. DIRECT: destination online and directly connected
    /// 2. HOLD: destination offline but directly connected
    /// 3. RELAY: use mutual peers for store-and-forward
    /// 4. DROP: no route available
    async fn route(
        &self,
        packet: &Packet<I>,
        current: &I,
    ) -> Result<RoutingDecision<I>, indras_core::RoutingError> {
        let dest = &packet.destination;

        // Create span for this routing operation
        let span = info_span!(
            "route_packet",
            packet_id = %packet.id,
            source = %packet.source,
            destination = %dest,
            current_peer = %current,
            correlation_id = ?packet.correlation_id,
            ttl = packet.ttl,
            hop_count = packet.hop_count(),
        );
        let _guard = span.enter();

        debug!("Starting routing decision");

        // Check TTL first
        if packet.ttl == 0 {
            info!(decision = "drop", reason = "ttl_expired", "TTL expired");
            return Ok(RoutingDecision::drop(DropReason::TtlExpired));
        }

        // 1. DIRECT: destination online and directly connected
        if self.topology.is_online(dest) && self.topology.are_connected(current, dest) {
            info!(
                decision = "direct",
                "Direct delivery: destination online and connected"
            );
            return Ok(RoutingDecision::direct(dest.clone()));
        }

        // 2. HOLD: destination offline but directly connected
        // We can deliver when they come online
        if !self.topology.is_online(dest) && self.topology.are_connected(current, dest) {
            info!(
                decision = "hold",
                "Hold: destination offline but directly connected"
            );
            // Store the packet for later delivery
            if let Err(e) = self.storage.store(packet.clone()).await {
                warn!(error = ?e, "Failed to store packet for offline peer");
                return Err(indras_core::RoutingError::NoRoute);
            }
            return Ok(RoutingDecision::hold());
        }

        // 3. RELAY: use mutual peers for store-and-forward
        // First check routing hints from the packet
        let mut candidates = packet.routing_hints.clone();

        // Add mutual peers from our tracker
        let mutual_relays = self.mutual_tracker.get_relays_for(current, dest);
        for relay in mutual_relays {
            if !candidates.contains(&relay) {
                candidates.push(relay);
            }
        }

        // Filter to online, unvisited candidates
        let online_relays = self.filter_relays(candidates, packet);

        if !online_relays.is_empty() {
            info!(
                decision = "relay",
                relay_count = online_relays.len(),
                "Found relay candidates"
            );
            return Ok(RoutingDecision::relay_multi(online_relays));
        }

        // 4. NO ROUTE: no way to reach destination
        info!(
            decision = "drop",
            reason = "no_route",
            "No route: destination not reachable"
        );
        Ok(RoutingDecision::drop(DropReason::NoRoute))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::{EncryptedPayload, PacketId, SimulationIdentity, StorageError};
    use std::collections::{HashMap, HashSet};
    use std::sync::RwLock;

    /// Test topology implementation
    struct TestTopology {
        connections: RwLock<HashMap<SimulationIdentity, HashSet<SimulationIdentity>>>,
        online: RwLock<HashSet<SimulationIdentity>>,
    }

    impl TestTopology {
        fn new() -> Self {
            Self {
                connections: RwLock::new(HashMap::new()),
                online: RwLock::new(HashSet::new()),
            }
        }

        fn connect(&self, a: SimulationIdentity, b: SimulationIdentity) {
            let mut conns = self.connections.write().unwrap();
            conns.entry(a).or_default().insert(b);
            conns.entry(b).or_default().insert(a);
        }

        fn set_online(&self, peer: SimulationIdentity) {
            self.online.write().unwrap().insert(peer);
        }

        fn set_offline(&self, peer: SimulationIdentity) {
            self.online.write().unwrap().remove(&peer);
        }
    }

    impl NetworkTopology<SimulationIdentity> for TestTopology {
        fn peers(&self) -> Vec<SimulationIdentity> {
            self.connections.read().unwrap().keys().cloned().collect()
        }

        fn neighbors(&self, peer: &SimulationIdentity) -> Vec<SimulationIdentity> {
            self.connections
                .read()
                .unwrap()
                .get(peer)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default()
        }

        fn are_connected(&self, a: &SimulationIdentity, b: &SimulationIdentity) -> bool {
            self.connections
                .read()
                .unwrap()
                .get(a)
                .map(|n| n.contains(b))
                .unwrap_or(false)
        }

        fn is_online(&self, peer: &SimulationIdentity) -> bool {
            self.online.read().unwrap().contains(peer)
        }
    }

    /// Test storage implementation
    struct TestStorage {
        packets: RwLock<HashMap<PacketId, Packet<SimulationIdentity>>>,
    }

    impl TestStorage {
        fn new() -> Self {
            Self {
                packets: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl PacketStore<SimulationIdentity> for TestStorage {
        async fn store(&self, packet: Packet<SimulationIdentity>) -> Result<(), StorageError> {
            self.packets.write().unwrap().insert(packet.id, packet);
            Ok(())
        }

        async fn retrieve(
            &self,
            id: &PacketId,
        ) -> Result<Option<Packet<SimulationIdentity>>, StorageError> {
            Ok(self.packets.read().unwrap().get(id).cloned())
        }

        async fn pending_for(
            &self,
            destination: &SimulationIdentity,
        ) -> Result<Vec<Packet<SimulationIdentity>>, StorageError> {
            Ok(self
                .packets
                .read()
                .unwrap()
                .values()
                .filter(|p| &p.destination == destination)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: &PacketId) -> Result<(), StorageError> {
            self.packets.write().unwrap().remove(id);
            Ok(())
        }

        async fn all_packets(&self) -> Result<Vec<Packet<SimulationIdentity>>, StorageError> {
            Ok(self.packets.read().unwrap().values().cloned().collect())
        }

        async fn count(&self) -> Result<usize, StorageError> {
            Ok(self.packets.read().unwrap().len())
        }

        async fn clear(&self) -> Result<(), StorageError> {
            self.packets.write().unwrap().clear();
            Ok(())
        }
    }

    fn make_id(c: char) -> SimulationIdentity {
        SimulationIdentity::new(c).unwrap()
    }

    fn make_packet(
        source: char,
        dest: char,
        seq: u64,
    ) -> Packet<SimulationIdentity> {
        Packet::new(
            PacketId::new(source as u64, seq),
            make_id(source),
            make_id(dest),
            EncryptedPayload::plaintext(b"test".to_vec()),
            vec![],
        )
    }

    #[tokio::test]
    async fn test_direct_delivery() {
        // Setup: A connected to B, both online
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));

        let router = StoreForwardRouter::new(topology, storage);

        let packet = make_packet('A', 'B', 1);
        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        assert!(decision.is_delivery());
        if let RoutingDecision::DirectDelivery { destination } = decision {
            assert_eq!(destination, make_id('B'));
        }
    }

    #[tokio::test]
    async fn test_hold_for_offline() {
        // Setup: A connected to B, B is offline
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.set_online(make_id('A'));
        // B is offline

        let router = StoreForwardRouter::new(topology, storage.clone());

        let packet = make_packet('A', 'B', 1);
        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        assert!(decision.is_hold());

        // Verify packet was stored
        let stored = storage.count().await.unwrap();
        assert_eq!(stored, 1);
    }

    #[tokio::test]
    async fn test_relay_through_mutual() {
        // Setup: A-B-C (A connected to B, B connected to C)
        // A wants to send to C, should relay through B
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.connect(make_id('B'), make_id('C'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));
        topology.set_online(make_id('C'));

        let router = StoreForwardRouter::new(topology.clone(), storage);

        // Tell router about connections
        router.on_peer_connect(&make_id('A'), &make_id('C'));

        let packet = make_packet('A', 'C', 1);
        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        assert!(decision.is_relay());
        if let RoutingDecision::RelayThrough { next_hops } = decision {
            assert!(!next_hops.is_empty());
            // B should be a relay candidate (mutual peer of A and C)
            assert!(next_hops.contains(&make_id('B')));
        }
    }

    #[tokio::test]
    async fn test_no_route() {
        // Setup: A and C not connected, no path
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.set_online(make_id('A'));
        topology.set_online(make_id('C'));
        // No connections!

        let router = StoreForwardRouter::new(topology, storage);

        let packet = make_packet('A', 'C', 1);
        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        assert!(decision.is_drop());
        if let RoutingDecision::Drop { reason } = decision {
            assert_eq!(reason, DropReason::NoRoute);
        }
    }

    #[tokio::test]
    async fn test_ttl_expired() {
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));

        let router = StoreForwardRouter::new(topology, storage);

        let mut packet = make_packet('A', 'B', 1);
        packet.ttl = 0; // Expired

        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        assert!(decision.is_drop());
        if let RoutingDecision::Drop { reason } = decision {
            assert_eq!(reason, DropReason::TtlExpired);
        }
    }

    #[tokio::test]
    async fn test_skip_visited_relay() {
        // Setup: A-B-C, but packet already visited B
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.connect(make_id('B'), make_id('C'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));
        topology.set_online(make_id('C'));

        let router = StoreForwardRouter::new(topology.clone(), storage);
        router.on_peer_connect(&make_id('A'), &make_id('C'));

        let mut packet = make_packet('A', 'C', 1);
        packet.mark_visited(&make_id('B')); // Already visited B

        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        // Should drop since B (the only relay) was visited
        assert!(decision.is_drop());
    }

    #[tokio::test]
    async fn test_routing_hints_used() {
        // Setup: A and C not directly connected, but packet has hint to use B
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        // A-B and B-C exist but A-C doesn't
        topology.connect(make_id('A'), make_id('B'));
        topology.connect(make_id('B'), make_id('C'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));
        topology.set_online(make_id('C'));

        let router = StoreForwardRouter::new(topology, storage);

        // Create packet with routing hint
        let mut packet = make_packet('A', 'C', 1);
        packet.routing_hints = vec![make_id('B')];

        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        assert!(decision.is_relay());
        if let RoutingDecision::RelayThrough { next_hops } = decision {
            assert!(next_hops.contains(&make_id('B')));
        }
    }

    #[tokio::test]
    async fn test_peer_connect_disconnect() {
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.connect(make_id('B'), make_id('C'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));
        topology.set_online(make_id('C'));

        let router = StoreForwardRouter::new(topology, storage);

        // Connect and verify relays are cached
        router.on_peer_connect(&make_id('A'), &make_id('C'));
        let relays = router.mutual_tracker().get_relays_for(&make_id('A'), &make_id('C'));
        assert!(!relays.is_empty());

        // Disconnect and verify relays are removed
        router.on_peer_disconnect(&make_id('A'), &make_id('C'));
        let relays = router.mutual_tracker().get_relays_for(&make_id('A'), &make_id('C'));
        assert!(relays.is_empty());
    }

    #[tokio::test]
    async fn test_store_and_retrieve_packet() {
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.set_online(make_id('A'));

        let router = StoreForwardRouter::new(topology, storage.clone());

        let packet = make_packet('A', 'B', 1);
        router.store_packet(packet.clone()).await.unwrap();

        // Retrieve pending
        let pending = router.get_pending(&make_id('B')).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, packet.id);

        // Delete packet
        router.delete_packet(&packet).await.unwrap();
        let pending = router.get_pending(&make_id('B')).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_backprop_integration() {
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        let router = StoreForwardRouter::new(topology, storage);

        let packet = make_packet('A', 'D', 1);
        let path = vec![make_id('A'), make_id('B'), make_id('C'), make_id('D')];

        // Start back-propagation
        router.start_backprop(&packet, path);

        // Verify back-propagation is tracked
        assert!(router.backprop().is_pending(&packet.id));

        // Check timeouts (should be empty since we just started)
        let timed_out = router.check_backprop_timeouts();
        assert!(timed_out.is_empty());
    }

    #[tokio::test]
    async fn test_routing_table_integration() {
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.connect(make_id('B'), make_id('C'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));
        topology.set_online(make_id('C'));

        let router = StoreForwardRouter::new(topology, storage);

        // Insert route
        let route = indras_core::routing::RouteInfo::new(
            make_id('C'),
            make_id('B'),
            2,
        );
        router.routing_table().insert(&make_id('C'), route);

        // Verify route exists
        assert!(router.routing_table().get(&make_id('C')).is_some());

        // Prune stale routes
        router.prune_stale_routes();

        // Route should still exist (not stale yet)
        assert!(router.routing_table().get(&make_id('C')).is_some());
    }

    #[tokio::test]
    async fn test_custom_timeouts() {
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        let router = StoreForwardRouter::with_timeouts(
            topology,
            storage,
            Duration::from_secs(60),
            Duration::from_secs(10),
        );

        // Verify router was created successfully
        assert!(router.routing_table().is_empty());
        assert!(router.mutual_tracker().is_empty());
    }

    #[tokio::test]
    async fn test_offline_relay_skipped() {
        // Setup: A-B-C where B is offline
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.connect(make_id('B'), make_id('C'));
        topology.set_online(make_id('A'));
        // B is offline
        topology.set_online(make_id('C'));

        let router = StoreForwardRouter::new(topology.clone(), storage);
        router.on_peer_connect(&make_id('A'), &make_id('C'));

        let packet = make_packet('A', 'C', 1);
        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        // Should drop since B (the only relay) is offline
        assert!(decision.is_drop());
    }

    #[tokio::test]
    async fn test_multiple_relay_candidates() {
        // Setup: A-B-D, A-C-D (two paths from A to D)
        let topology = Arc::new(TestTopology::new());
        let storage = Arc::new(TestStorage::new());

        topology.connect(make_id('A'), make_id('B'));
        topology.connect(make_id('A'), make_id('C'));
        topology.connect(make_id('B'), make_id('D'));
        topology.connect(make_id('C'), make_id('D'));
        topology.set_online(make_id('A'));
        topology.set_online(make_id('B'));
        topology.set_online(make_id('C'));
        topology.set_online(make_id('D'));

        let router = StoreForwardRouter::new(topology.clone(), storage);
        router.on_peer_connect(&make_id('A'), &make_id('D'));

        let packet = make_packet('A', 'D', 1);
        let decision = router.route(&packet, &make_id('A')).await.unwrap();

        assert!(decision.is_relay());
        if let RoutingDecision::RelayThrough { next_hops } = decision {
            // Both B and C should be relay candidates
            assert!(next_hops.len() >= 2);
            assert!(next_hops.contains(&make_id('B')) || next_hops.contains(&make_id('C')));
        }
    }
}
