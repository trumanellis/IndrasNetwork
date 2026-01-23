//! Core types for Indra's Network simulation
//!
//! Models a mesh of named peers (A-Z) with store-and-forward routing
//! for delivering messages to offline peers through mutual connections.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Unique identifier for a peer in the network (A-Z)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PeerId(pub char);

impl PeerId {
    /// Create a new PeerId from a capital letter
    pub fn new(c: char) -> Option<Self> {
        if c.is_ascii_uppercase() {
            Some(Self(c))
        } else {
            None
        }
    }

    /// Generate all peer IDs from A to the given letter (inclusive)
    pub fn range_to(end: char) -> Vec<Self> {
        ('A'..=end)
            .filter_map(Self::new)
            .collect()
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a packet (monotonic counter + source)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PacketId {
    pub source: PeerId,
    pub sequence: u64,
}

impl std::fmt::Display for PacketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.source, self.sequence)
    }
}

/// A sealed packet for store-and-forward delivery
/// 
/// When the destination is offline, intermediate peers can hold and forward
/// this packet. The payload is conceptually encrypted for the destination
/// (in simulation, we just mark it as sealed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedPacket {
    /// Unique packet identifier
    pub id: PacketId,
    /// Original sender
    pub source: PeerId,
    /// Final destination
    pub destination: PeerId,
    /// The actual message payload (sealed for destination)
    pub payload: Vec<u8>,
    /// Routing hints: known mutual peers who might reach the destination
    pub routing_hints: BTreeSet<PeerId>,
    /// When the packet was created (simulation tick)
    pub created_at: u64,
    /// TTL: maximum hops before dropping
    pub ttl: u8,
    /// Peers who have already handled this packet (prevents loops)
    pub visited: BTreeSet<PeerId>,
}

impl SealedPacket {
    pub fn new(
        id: PacketId,
        source: PeerId,
        destination: PeerId,
        payload: Vec<u8>,
        routing_hints: BTreeSet<PeerId>,
        tick: u64,
    ) -> Self {
        let mut visited = BTreeSet::new();
        visited.insert(source);
        Self {
            id,
            source,
            destination,
            payload,
            routing_hints,
            created_at: tick,
            ttl: 10, // Default 10 hops
            visited,
        }
    }

    /// Record that a peer has handled this packet
    pub fn mark_visited(&mut self, peer: PeerId) {
        self.visited.insert(peer);
    }

    /// Check if a peer has already handled this packet
    pub fn was_visited(&self, peer: PeerId) -> bool {
        self.visited.contains(&peer)
    }

    /// Decrement TTL, returns false if packet should be dropped
    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl == 0 {
            return false;
        }
        self.ttl -= 1;
        true
    }
}

/// Events that occur in the network simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    /// Peer comes online and broadcasts awake signal
    Awake {
        peer: PeerId,
        tick: u64,
    },
    /// Peer goes offline
    Sleep {
        peer: PeerId,
        tick: u64,
    },
    /// Direct message attempt (may succeed or need routing)
    Send {
        from: PeerId,
        to: PeerId,
        payload: Vec<u8>,
        tick: u64,
    },
    /// Packet relayed through intermediate peer
    Relay {
        from: PeerId,
        via: PeerId,
        to: PeerId,
        packet_id: PacketId,
        tick: u64,
    },
    /// Packet successfully delivered to destination
    Delivered {
        packet_id: PacketId,
        to: PeerId,
        tick: u64,
    },
    /// Back-propagation: delivery confirmation traveling back to source
    BackProp {
        packet_id: PacketId,
        from: PeerId,
        via: PeerId,
        to: PeerId,
        tick: u64,
    },
    /// Packet dropped (TTL expired or unroutable)
    Dropped {
        packet_id: PacketId,
        reason: DropReason,
        tick: u64,
    },

    // Post-quantum cryptography events (simulated)
    /// PQ signature created
    PQSignatureCreated {
        peer: PeerId,
        latency_us: u64,
        message_size: usize,
        tick: u64,
    },
    /// PQ signature verified
    PQSignatureVerified {
        peer: PeerId,
        sender: PeerId,
        latency_us: u64,
        success: bool,
        tick: u64,
    },
    /// KEM encapsulation performed
    KEMEncapsulation {
        peer: PeerId,
        target: PeerId,
        latency_us: u64,
        tick: u64,
    },
    /// KEM decapsulation performed
    KEMDecapsulation {
        peer: PeerId,
        sender: PeerId,
        latency_us: u64,
        success: bool,
        tick: u64,
    },
    /// Invite created
    InviteCreated {
        from: PeerId,
        to: PeerId,
        interface_id: String,
        tick: u64,
    },
    /// Invite accepted
    InviteAccepted {
        peer: PeerId,
        interface_id: String,
        tick: u64,
    },
    /// Invite failed
    InviteFailed {
        peer: PeerId,
        interface_id: String,
        reason: String,
        tick: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DropReason {
    TtlExpired,
    NoRoute,
    Duplicate,
    /// Message expired (waited too long in queue)
    Expired,
    /// Sender never came online after max retries
    SenderOffline,
}

/// State of a peer in the network
#[derive(Debug, Clone)]
pub struct PeerState {
    pub id: PeerId,
    pub online: bool,
    /// Direct connections to other peers (bidirectional)
    pub connections: BTreeSet<PeerId>,
    /// Packets addressed to this peer waiting for delivery
    pub inbox: Vec<SealedPacket>,
    /// Packets this peer is holding for relay to others
    pub relay_queue: Vec<SealedPacket>,
    /// Packets that have been delivered (for tracking)
    pub delivered: Vec<PacketId>,
    /// Pending back-propagation confirmations
    pub pending_backprops: Vec<BackPropRecord>,
    /// Sequence counter for generating packet IDs
    pub sequence: u64,
    /// Last tick when peer was online
    pub last_online_tick: Option<u64>,
}

/// Record of a packet relay for back-propagation
#[derive(Debug, Clone)]
pub struct BackPropRecord {
    pub packet_id: PacketId,
    /// Who we received the packet from (to send confirmation back)
    pub received_from: PeerId,
    /// Final destination of the original packet
    pub destination: PeerId,
}

impl PeerState {
    pub fn new(id: PeerId) -> Self {
        Self {
            id,
            online: false,
            connections: BTreeSet::new(),
            inbox: Vec::new(),
            relay_queue: Vec::new(),
            delivered: Vec::new(),
            pending_backprops: Vec::new(),
            sequence: 0,
            last_online_tick: None,
        }
    }

    /// Generate the next packet ID for this peer
    pub fn next_packet_id(&mut self) -> PacketId {
        let id = PacketId {
            source: self.id,
            sequence: self.sequence,
        };
        self.sequence += 1;
        id
    }

    /// Check if this peer can directly reach another peer
    pub fn can_reach(&self, other: PeerId) -> bool {
        self.connections.contains(&other)
    }

    /// Get mutual peers between this peer and a target
    pub fn mutual_peers_with(&self, other: &PeerState) -> BTreeSet<PeerId> {
        self.connections
            .intersection(&other.connections)
            .copied()
            .collect()
    }
}

/// The append-only event log for a peer interface (CRDT-style)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventLog {
    pub events: Vec<NetworkEvent>,
}

impl EventLog {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn append(&mut self, event: NetworkEvent) {
        self.events.push(event);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Synced interface between two peers
/// This represents the shared state that both peers maintain
#[derive(Debug, Clone)]
pub struct PeerInterface {
    /// The two peers this interface connects
    pub peer_a: PeerId,
    pub peer_b: PeerId,
    /// Append-only event log (synced via CRDT)
    pub event_log: EventLog,
    /// Packets being held for relay
    pub pending_packets: Vec<SealedPacket>,
}

impl PeerInterface {
    pub fn new(peer_a: PeerId, peer_b: PeerId) -> Self {
        // Normalize ordering so (A,B) and (B,A) create the same interface
        let (peer_a, peer_b) = if peer_a < peer_b {
            (peer_a, peer_b)
        } else {
            (peer_b, peer_a)
        };
        Self {
            peer_a,
            peer_b,
            event_log: EventLog::new(),
            pending_packets: Vec::new(),
        }
    }

    /// Get the interface key for storage in a map
    pub fn key(&self) -> (PeerId, PeerId) {
        (self.peer_a, self.peer_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_id_creation() {
        assert!(PeerId::new('A').is_some());
        assert!(PeerId::new('Z').is_some());
        assert!(PeerId::new('a').is_none());
        assert!(PeerId::new('1').is_none());
    }

    #[test]
    fn test_peer_id_range() {
        let peers = PeerId::range_to('C');
        assert_eq!(peers.len(), 3);
        assert_eq!(peers[0].0, 'A');
        assert_eq!(peers[1].0, 'B');
        assert_eq!(peers[2].0, 'C');
    }

    #[test]
    fn test_sealed_packet_visited() {
        let packet_id = PacketId { source: PeerId('A'), sequence: 0 };
        let mut packet = SealedPacket::new(
            packet_id,
            PeerId('A'),
            PeerId('C'),
            vec![1, 2, 3],
            BTreeSet::new(),
            0,
        );
        
        assert!(packet.was_visited(PeerId('A'))); // Source is auto-visited
        assert!(!packet.was_visited(PeerId('B')));
        
        packet.mark_visited(PeerId('B'));
        assert!(packet.was_visited(PeerId('B')));
    }
}
