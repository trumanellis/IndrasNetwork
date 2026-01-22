//! Core traits for Indras Network
//!
//! These traits provide the abstractions needed to build a modular,
//! testable P2P networking stack.
//!
//! ## Key Traits
//!
//! - [`NetworkTopology`]: Abstraction over network structure
//! - [`Router`]: Routing decision logic (legacy, for packet-based routing)
//! - [`NInterfaceTrait`]: N-peer shared interface with append-only event log
//! - [`Clock`]: Time abstraction for testability

use std::collections::HashSet;
use std::future::Future;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{InterfaceError, RoutingError, StorageError};
use crate::identity::PeerIdentity;
use crate::interface::{EventId, InterfaceEvent, InterfaceId};
use crate::packet::{Packet, PacketId};
use crate::routing::RoutingDecision;

/// Abstraction over network topology
///
/// This trait allows routing logic to work with both static test topologies
/// and dynamic real-world networks where peers come and go.
pub trait NetworkTopology<I: PeerIdentity>: Send + Sync {
    /// Get all known peers in the network
    fn peers(&self) -> Vec<I>;

    /// Get the neighbors of a peer (directly connected peers)
    fn neighbors(&self, peer: &I) -> Vec<I>;

    /// Check if two peers are directly connected
    fn are_connected(&self, a: &I, b: &I) -> bool;

    /// Check if a peer is currently online/reachable
    fn is_online(&self, peer: &I) -> bool;

    /// Get mutual peers between two peers (common neighbors)
    fn mutual_peers(&self, a: &I, b: &I) -> Vec<I> {
        let neighbors_a: std::collections::HashSet<_> = self.neighbors(a).into_iter().collect();
        let neighbors_b: std::collections::HashSet<_> = self.neighbors(b).into_iter().collect();
        neighbors_a.intersection(&neighbors_b).cloned().collect()
    }
}

/// Routing decision logic
///
/// Implementations of this trait determine how packets are routed
/// through the network.
#[async_trait]
pub trait Router<I: PeerIdentity>: Send + Sync {
    /// Make a routing decision for a packet
    ///
    /// Given a packet and the current peer, decide how to route it:
    /// - Direct delivery if destination is reachable
    /// - Relay through intermediate peers
    /// - Hold for later if no route available
    /// - Drop if unroutable
    async fn route(
        &self,
        packet: &Packet<I>,
        current_peer: &I,
    ) -> Result<RoutingDecision<I>, RoutingError>;
}

/// Storage abstraction for packets
///
/// This trait allows different storage backends (memory, disk, database)
/// to be used interchangeably.
#[async_trait]
pub trait PacketStore<I: PeerIdentity>: Send + Sync {
    /// Store a packet
    async fn store(&self, packet: Packet<I>) -> Result<(), StorageError>;

    /// Retrieve a packet by ID
    async fn retrieve(&self, id: &PacketId) -> Result<Option<Packet<I>>, StorageError>;

    /// Get all packets pending for a destination
    async fn pending_for(&self, destination: &I) -> Result<Vec<Packet<I>>, StorageError>;

    /// Delete a packet by ID
    async fn delete(&self, id: &PacketId) -> Result<(), StorageError>;

    /// Get all stored packets
    async fn all_packets(&self) -> Result<Vec<Packet<I>>, StorageError>;

    /// Get the count of stored packets
    async fn count(&self) -> Result<usize, StorageError>;

    /// Clear all stored packets
    async fn clear(&self) -> Result<(), StorageError>;
}

/// Time abstraction for testability
///
/// This trait allows tests to control time, enabling deterministic
/// testing of time-dependent behavior.
pub trait Clock: Send + Sync {
    /// Get the current instant (monotonic time)
    fn now(&self) -> Instant;

    /// Get the current UTC datetime
    fn now_utc(&self) -> DateTime<Utc>;

    /// Sleep for a duration (async)
    fn sleep(&self, duration: std::time::Duration) -> impl Future<Output = ()> + Send;
}

/// Real clock implementation using system time
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    async fn sleep(&self, duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }
}

/// Event handler trait for network events
#[async_trait]
pub trait EventHandler<I: PeerIdentity>: Send + Sync {
    /// Handle a network event
    async fn handle(&self, event: crate::event::NetworkEvent<I>);
}

// ============================================================================
// N-Peer Interface Types and Traits
// ============================================================================

/// Topic identifier for gossip-based broadcast
///
/// This is an abstraction over the underlying gossip topic ID.
/// In production, this maps to iroh-gossip's TopicId.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TopicId(pub [u8; 32]);

impl TopicId {
    /// Create a new topic ID from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create from an InterfaceId (they share the same value)
    pub fn from_interface(id: InterfaceId) -> Self {
        Self(*id.as_bytes())
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<InterfaceId> for TopicId {
    fn from(id: InterfaceId) -> Self {
        Self::from_interface(id)
    }
}

impl From<TopicId> for InterfaceId {
    fn from(topic: TopicId) -> Self {
        InterfaceId::new(topic.0)
    }
}

/// Sync message for Automerge document synchronization
///
/// This wraps Automerge sync protocol messages for exchange between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    /// The interface this sync is for
    pub interface_id: InterfaceId,
    /// Automerge sync message bytes
    pub sync_data: Vec<u8>,
    /// Our current heads (for the peer to know our state)
    pub heads: Vec<[u8; 32]>,
    /// Whether this is a request (true) or response (false)
    pub is_request: bool,
}

impl SyncMessage {
    /// Create a new sync request
    pub fn request(interface_id: InterfaceId, sync_data: Vec<u8>, heads: Vec<[u8; 32]>) -> Self {
        Self {
            interface_id,
            sync_data,
            heads,
            is_request: true,
        }
    }

    /// Create a new sync response
    pub fn response(interface_id: InterfaceId, sync_data: Vec<u8>, heads: Vec<[u8; 32]>) -> Self {
        Self {
            interface_id,
            sync_data,
            heads,
            is_request: false,
        }
    }
}

/// Trait for N-peer shared interface
///
/// An N-peer interface is a shared space where N peers can communicate.
/// It uses an append-only event log with store-and-forward delivery,
/// backed by an Automerge CRDT document for state synchronization.
///
/// Key features:
/// - Events are broadcast to all members via gossip
/// - Offline peers receive missed events on reconnect (store-and-forward)
/// - Document state is synchronized via Automerge sync protocol
/// - Two peers is just a special case of N peers
#[async_trait]
pub trait NInterfaceTrait<I: PeerIdentity>: Send + Sync {
    /// Get the interface identifier
    fn id(&self) -> InterfaceId;

    /// Get the gossip topic for this interface
    fn topic_id(&self) -> TopicId;

    /// Get current members
    fn members(&self) -> HashSet<I>;

    /// Check if a peer is a member
    fn is_member(&self, peer: &I) -> bool {
        self.members().contains(peer)
    }

    /// Append an event to the log
    ///
    /// The event will be:
    /// 1. Added to the local event log
    /// 2. Broadcast to online members via gossip
    /// 3. Stored for offline members (store-and-forward)
    async fn append(&mut self, event: InterfaceEvent<I>) -> Result<EventId, InterfaceError>;

    /// Get events since a global sequence number
    ///
    /// Returns events in causal order since the given sequence.
    fn events_since(&self, since: u64) -> Vec<InterfaceEvent<I>>;

    /// Get all events in the interface
    fn all_events(&self) -> Vec<InterfaceEvent<I>> {
        self.events_since(0)
    }

    /// Get pending events for an offline peer
    ///
    /// These are events that haven't been confirmed delivered to the peer.
    fn pending_for(&self, peer: &I) -> Vec<InterfaceEvent<I>>;

    /// Mark events as delivered to a peer
    ///
    /// Called when we receive confirmation that a peer has received events.
    fn mark_delivered(&mut self, peer: &I, up_to: EventId);

    /// Merge incoming sync state
    ///
    /// Applies an Automerge sync message from a peer, updating our document state.
    async fn merge_sync(&mut self, sync_msg: SyncMessage) -> Result<(), InterfaceError>;

    /// Generate sync state for a peer
    ///
    /// Creates an Automerge sync message to send to a peer for synchronization.
    fn generate_sync(&self, for_peer: &I) -> SyncMessage;

    /// Get the current document heads (for sync protocol)
    fn heads(&self) -> Vec<[u8; 32]>;

    /// Check if we have pending events for any peer
    fn has_pending(&self) -> bool;

    /// Get the total number of events in the log
    fn event_count(&self) -> usize;
}

/// Connection manager trait
#[async_trait]
pub trait ConnectionManager<I: PeerIdentity>: Send + Sync {
    /// Connect to a peer
    async fn connect(&self, peer: &I) -> Result<(), crate::error::TransportError>;

    /// Disconnect from a peer
    async fn disconnect(&self, peer: &I) -> Result<(), crate::error::TransportError>;

    /// Check if connected to a peer
    fn is_connected(&self, peer: &I) -> bool;

    /// Get all connected peers
    fn connected_peers(&self) -> Vec<I>;

    /// Send data to a peer
    async fn send(&self, peer: &I, data: &[u8]) -> Result<(), crate::error::TransportError>;

    /// Receive data from any peer
    async fn receive(&self) -> Result<(I, Vec<u8>), crate::error::TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SimulationIdentity;
    use std::collections::HashMap;
    use std::sync::RwLock;

    /// Simple in-memory topology for testing
    struct TestTopology {
        peers: Vec<SimulationIdentity>,
        connections: HashMap<SimulationIdentity, Vec<SimulationIdentity>>,
        online: RwLock<std::collections::HashSet<SimulationIdentity>>,
    }

    impl NetworkTopology<SimulationIdentity> for TestTopology {
        fn peers(&self) -> Vec<SimulationIdentity> {
            self.peers.clone()
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
    fn test_topology_trait() {
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();

        let mut connections = HashMap::new();
        connections.insert(a, vec![b, c]);
        connections.insert(b, vec![a, c]);
        connections.insert(c, vec![a, b]);

        let mut online = std::collections::HashSet::new();
        online.insert(a);
        online.insert(b);

        let topology = TestTopology {
            peers: vec![a, b, c],
            connections,
            online: RwLock::new(online),
        };

        assert_eq!(topology.peers().len(), 3);
        assert!(topology.are_connected(&a, &b));
        assert!(topology.is_online(&a));
        assert!(!topology.is_online(&c));

        let mutual = topology.mutual_peers(&a, &b);
        assert!(mutual.contains(&c));
    }
}
