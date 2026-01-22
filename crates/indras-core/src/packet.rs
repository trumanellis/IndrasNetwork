//! Packet types for store-and-forward delivery

use std::collections::HashSet;
use std::fmt::Display;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::PeerIdentity;

/// Unique identifier for a packet
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PacketId {
    /// Source peer that created this packet
    pub source_hash: u64,
    /// Sequence number from the source
    pub sequence: u64,
}

impl PacketId {
    /// Create a new packet ID
    pub fn new(source_hash: u64, sequence: u64) -> Self {
        Self {
            source_hash,
            sequence,
        }
    }
}

impl Display for PacketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08x}#{}", self.source_hash & 0xFFFF, self.sequence)
    }
}

/// Priority levels for packets
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub enum Priority {
    /// Low priority - can be delayed
    Low,
    /// Normal priority (default)
    #[default]
    Normal,
    /// High priority - deliver ASAP
    High,
    /// Critical - never drop
    Critical,
}

/// Encrypted payload wrapper
///
/// In simulation mode, this just holds plaintext.
/// In production, this contains actual encrypted data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// The payload data (encrypted in production, plaintext in simulation)
    pub data: Vec<u8>,
    /// Whether this payload is actually encrypted
    pub encrypted: bool,
}

impl EncryptedPayload {
    /// Create a plaintext payload (for simulation)
    pub fn plaintext(data: Vec<u8>) -> Self {
        Self {
            data,
            encrypted: false,
        }
    }

    /// Create an encrypted payload
    pub fn encrypted(data: Vec<u8>) -> Self {
        Self {
            data,
            encrypted: true,
        }
    }

    /// Get the payload data
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Get the payload length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if payload is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// A packet for store-and-forward delivery
///
/// Generic over the identity type, allowing use with both simulation
/// identities and real cryptographic identities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub struct Packet<I: PeerIdentity> {
    /// Unique packet identifier
    pub id: PacketId,
    /// Original sender
    pub source: I,
    /// Final destination
    pub destination: I,
    /// The message payload
    pub payload: EncryptedPayload,
    /// Routing hints: known mutual peers who might reach the destination
    pub routing_hints: Vec<I>,
    /// When the packet was created
    pub created_at: DateTime<Utc>,
    /// TTL: maximum hops before dropping
    pub ttl: u8,
    /// Peers who have already handled this packet (prevents loops)
    pub visited: HashSet<u64>, // Store hashes for serialization
    /// Priority level
    pub priority: Priority,
}

impl<I: PeerIdentity> Packet<I> {
    /// Create a new packet
    pub fn new(
        id: PacketId,
        source: I,
        destination: I,
        payload: EncryptedPayload,
        routing_hints: Vec<I>,
    ) -> Self {
        let mut visited = HashSet::new();
        // Hash the source identity for the visited set
        visited.insert(Self::hash_identity(&source));

        Self {
            id,
            source,
            destination,
            payload,
            routing_hints,
            created_at: Utc::now(),
            ttl: 10, // Default 10 hops
            visited,
            priority: Priority::Normal,
        }
    }

    /// Create a packet with custom TTL
    pub fn with_ttl(mut self, ttl: u8) -> Self {
        self.ttl = ttl;
        self
    }

    /// Create a packet with custom priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Record that a peer has handled this packet
    pub fn mark_visited(&mut self, peer: &I) {
        self.visited.insert(Self::hash_identity(peer));
    }

    /// Check if a peer has already handled this packet
    pub fn was_visited(&self, peer: &I) -> bool {
        self.visited.contains(&Self::hash_identity(peer))
    }

    /// Decrement TTL, returns false if packet should be dropped
    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl == 0 {
            return false;
        }
        self.ttl -= 1;
        true
    }

    /// Get the number of hops this packet has taken
    pub fn hop_count(&self) -> usize {
        self.visited.len()
    }

    /// Get the age of this packet
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }

    /// Hash an identity for the visited set
    fn hash_identity(identity: &I) -> u64 {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        identity.hash(&mut hasher);
        hasher.finish()
    }
}

/// Delivery confirmation for back-propagation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub struct DeliveryConfirmation<I: PeerIdentity> {
    /// The packet that was delivered
    pub packet_id: PacketId,
    /// Who received the packet
    pub delivered_to: I,
    /// When it was delivered
    pub delivered_at: DateTime<Utc>,
    /// The path the packet took (for back-propagation)
    pub path: Vec<I>,
}

impl<I: PeerIdentity> DeliveryConfirmation<I> {
    /// Create a new delivery confirmation
    pub fn new(packet_id: PacketId, delivered_to: I, path: Vec<I>) -> Self {
        Self {
            packet_id,
            delivered_to,
            delivered_at: Utc::now(),
            path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SimulationIdentity;

    #[test]
    fn test_packet_creation() {
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('C').unwrap();
        let id = PacketId::new(0x1234, 0);

        let packet = Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(b"Hello".to_vec()),
            vec![],
        );

        assert_eq!(packet.ttl, 10);
        assert!(packet.was_visited(&source));
        assert!(!packet.was_visited(&dest));
    }

    #[test]
    fn test_packet_ttl() {
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('C').unwrap();
        let id = PacketId::new(0x1234, 0);

        let mut packet = Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(vec![]),
            vec![],
        )
        .with_ttl(2);

        assert!(packet.decrement_ttl()); // 2 -> 1
        assert!(packet.decrement_ttl()); // 1 -> 0
        assert!(!packet.decrement_ttl()); // 0 -> can't decrement
    }

    #[test]
    fn test_packet_visited() {
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('C').unwrap();
        let relay = SimulationIdentity::new('B').unwrap();
        let id = PacketId::new(0x1234, 0);

        let mut packet = Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(vec![]),
            vec![],
        );

        assert!(!packet.was_visited(&relay));
        packet.mark_visited(&relay);
        assert!(packet.was_visited(&relay));
        assert_eq!(packet.hop_count(), 2); // source + relay
    }
}
