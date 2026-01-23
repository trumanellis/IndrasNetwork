//! N-peer interface types
//!
//! Types for N-peer shared interfaces backed by append-only event logs
//! and Automerge CRDT documents.

use std::collections::HashSet;
use std::fmt::Display;
use std::hash::Hash;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::PeerIdentity;

/// Unique identifier for an interface
///
/// Derived from the gossip topic ID, serves as the primary key for interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InterfaceId(pub [u8; 32]);

impl InterfaceId {
    /// Create a new interface ID from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Generate a random interface ID
    pub fn generate() -> Self {
        use std::hash::Hasher;
        let mut bytes = [0u8; 32];
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        // Mix time with random data
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write_u128(now);
        hasher.write_u64(rand::random());
        hasher.write_u64(rand::random());

        let hash1 = hasher.finish();
        hasher.write_u64(rand::random());
        hasher.write_u64(rand::random());
        let hash2 = hasher.finish();
        hasher.write_u64(rand::random());
        hasher.write_u64(rand::random());
        let hash3 = hasher.finish();
        hasher.write_u64(rand::random());
        hasher.write_u64(rand::random());
        let hash4 = hasher.finish();

        bytes[0..8].copy_from_slice(&hash1.to_le_bytes());
        bytes[8..16].copy_from_slice(&hash2.to_le_bytes());
        bytes[16..24].copy_from_slice(&hash3.to_le_bytes());
        bytes[24..32].copy_from_slice(&hash4.to_le_bytes());

        Self(bytes)
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Create from a slice (must be 32 bytes)
    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        if slice.len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(slice);
            Some(Self(bytes))
        } else {
            None
        }
    }

    /// Short display form (first 8 hex chars)
    pub fn short(&self) -> String {
        hex::encode(&self.0[..4])
    }

    /// Create an interface ID from key bytes (hashes the key)
    ///
    /// This derives a deterministic interface ID from a key, useful when
    /// joining an interface via an invite where the ID needs to match.
    pub fn from_key_bytes(key_bytes: &[u8]) -> Self {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write(key_bytes);
        hasher.write(b"interface-id-v1");

        let mut bytes = [0u8; 32];
        let hash = hasher.finish();
        bytes[0..8].copy_from_slice(&hash.to_le_bytes());

        // Fill remaining bytes with more hashing
        hasher.write_u64(hash);
        let hash2 = hasher.finish();
        bytes[8..16].copy_from_slice(&hash2.to_le_bytes());

        hasher.write_u64(hash2);
        let hash3 = hasher.finish();
        bytes[16..24].copy_from_slice(&hash3.to_le_bytes());

        hasher.write_u64(hash3);
        let hash4 = hasher.finish();
        bytes[24..32].copy_from_slice(&hash4.to_le_bytes());

        Self(bytes)
    }
}

impl Display for InterfaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0[..8]))
    }
}

impl From<[u8; 32]> for InterfaceId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Unique identifier for an event within an interface
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId {
    /// Hash of the sender's identity
    pub sender_hash: u64,
    /// Sequence number from this sender
    pub sequence: u64,
}

impl EventId {
    /// Create a new event ID
    pub fn new(sender_hash: u64, sequence: u64) -> Self {
        Self { sender_hash, sequence }
    }

    /// Create from a peer identity and sequence
    pub fn from_peer<I: PeerIdentity>(peer: &I, sequence: u64) -> Self {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        peer.hash(&mut hasher);
        Self {
            sender_hash: hasher.finish(),
            sequence,
        }
    }

    /// Convert to bytes (16 bytes: 8 for sender_hash + 8 for sequence)
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&self.sender_hash.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.sequence.to_be_bytes());
        bytes
    }

    /// Create from bytes (16 bytes)
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        let sender_hash = u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        let sequence = u64::from_be_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        Self { sender_hash, sequence }
    }
}

impl Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08x}#{}", self.sender_hash & 0xFFFF, self.sequence)
    }
}

/// Events that can occur in an N-peer interface
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub enum InterfaceEvent<I: PeerIdentity> {
    /// A message sent to the interface
    Message {
        /// Unique event identifier
        id: EventId,
        /// Who sent the message
        sender: I,
        /// Message content (application-defined)
        content: Vec<u8>,
        /// When the message was created
        timestamp: DateTime<Utc>,
    },

    /// Membership change in the interface
    MembershipChange {
        /// Unique event identifier
        id: EventId,
        /// The change that occurred
        change: MembershipChange<I>,
        /// When the change occurred
        timestamp: DateTime<Utc>,
    },

    /// Presence update from a peer
    Presence {
        /// Which peer's presence changed
        peer: I,
        /// New presence status
        status: PresenceStatus,
        /// When the status changed
        timestamp: DateTime<Utc>,
    },

    /// Application-specific custom event
    Custom {
        /// Unique event identifier
        id: EventId,
        /// Who sent the event
        sender: I,
        /// Event type identifier (application-defined)
        event_type: String,
        /// Event payload (application-defined)
        payload: Vec<u8>,
        /// When the event was created
        timestamp: DateTime<Utc>,
    },

    /// Sync marker (internal use for Automerge sync points)
    SyncMarker {
        /// Which peer created this marker
        peer: I,
        /// Automerge heads at this point
        heads: Vec<[u8; 32]>,
        /// When the marker was created
        timestamp: DateTime<Utc>,
    },
}

impl<I: PeerIdentity> InterfaceEvent<I> {
    /// Create a new message event
    pub fn message(sender: I, sequence: u64, content: Vec<u8>) -> Self {
        Self::Message {
            id: EventId::from_peer(&sender, sequence),
            sender,
            content,
            timestamp: Utc::now(),
        }
    }

    /// Create a membership change event
    pub fn membership(actor: &I, sequence: u64, change: MembershipChange<I>) -> Self {
        Self::MembershipChange {
            id: EventId::from_peer(actor, sequence),
            change,
            timestamp: Utc::now(),
        }
    }

    /// Create a presence event
    pub fn presence(peer: I, status: PresenceStatus) -> Self {
        Self::Presence {
            peer,
            status,
            timestamp: Utc::now(),
        }
    }

    /// Create a custom event
    pub fn custom(sender: I, sequence: u64, event_type: String, payload: Vec<u8>) -> Self {
        Self::Custom {
            id: EventId::from_peer(&sender, sequence),
            sender,
            event_type,
            payload,
            timestamp: Utc::now(),
        }
    }

    /// Get the event ID if applicable
    pub fn event_id(&self) -> Option<EventId> {
        match self {
            Self::Message { id, .. } => Some(*id),
            Self::MembershipChange { id, .. } => Some(*id),
            Self::Custom { id, .. } => Some(*id),
            Self::Presence { .. } | Self::SyncMarker { .. } => None,
        }
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::Message { timestamp, .. } => *timestamp,
            Self::MembershipChange { timestamp, .. } => *timestamp,
            Self::Presence { timestamp, .. } => *timestamp,
            Self::Custom { timestamp, .. } => *timestamp,
            Self::SyncMarker { timestamp, .. } => *timestamp,
        }
    }

    /// Get the sender if applicable
    pub fn sender(&self) -> Option<&I> {
        match self {
            Self::Message { sender, .. } => Some(sender),
            Self::Custom { sender, .. } => Some(sender),
            Self::Presence { peer, .. } => Some(peer),
            Self::SyncMarker { peer, .. } => Some(peer),
            Self::MembershipChange { change, .. } => change.actor(),
        }
    }
}

/// Types of membership changes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub enum MembershipChange<I: PeerIdentity> {
    /// A peer joined the interface (self-join or accepted invite)
    Joined {
        /// Who joined
        peer: I,
    },

    /// A peer left the interface voluntarily
    Left {
        /// Who left
        peer: I,
    },

    /// A peer was invited by an existing member
    Invited {
        /// Who sent the invite
        by: I,
        /// Who was invited
        peer: I,
    },

    /// A peer was removed by an existing member
    Removed {
        /// Who removed them
        by: I,
        /// Who was removed
        peer: I,
    },

    /// Interface was created (first event)
    Created {
        /// Who created the interface
        creator: I,
    },
}

impl<I: PeerIdentity> MembershipChange<I> {
    /// Get the actor who initiated the change (if any)
    pub fn actor(&self) -> Option<&I> {
        match self {
            Self::Joined { peer } => Some(peer),
            Self::Left { peer } => Some(peer),
            Self::Invited { by, .. } => Some(by),
            Self::Removed { by, .. } => Some(by),
            Self::Created { creator } => Some(creator),
        }
    }

    /// Get the peer affected by the change
    pub fn affected_peer(&self) -> &I {
        match self {
            Self::Joined { peer } => peer,
            Self::Left { peer } => peer,
            Self::Invited { peer, .. } => peer,
            Self::Removed { peer, .. } => peer,
            Self::Created { creator } => creator,
        }
    }
}

/// Presence status of a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PresenceStatus {
    /// Peer is online and active
    Online,
    /// Peer is offline
    #[default]
    Offline,
    /// Peer is online but away/idle
    Away,
    /// Peer is online but busy/do-not-disturb
    Busy,
}

impl Display for PresenceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Offline => write!(f, "offline"),
            Self::Away => write!(f, "away"),
            Self::Busy => write!(f, "busy"),
        }
    }
}

/// Metadata about an interface
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub struct InterfaceMetadata<I: PeerIdentity> {
    /// Interface identifier
    pub id: InterfaceId,
    /// Human-readable name (optional)
    pub name: Option<String>,
    /// Description (optional)
    pub description: Option<String>,
    /// Who created the interface
    pub creator: I,
    /// When the interface was created
    pub created_at: DateTime<Utc>,
    /// Current members
    pub members: HashSet<I>,
    /// Application-specific metadata
    pub custom: Vec<u8>,
}

impl<I: PeerIdentity> InterfaceMetadata<I> {
    /// Create metadata for a new interface
    pub fn new(id: InterfaceId, creator: I) -> Self {
        let mut members = HashSet::new();
        members.insert(creator.clone());

        Self {
            id,
            name: None,
            description: None,
            creator,
            created_at: Utc::now(),
            members,
            custom: vec![],
        }
    }

    /// Set the interface name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the interface description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SimulationIdentity;

    #[test]
    fn test_interface_id_generation() {
        let id1 = InterfaceId::generate();
        let id2 = InterfaceId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_interface_id_display() {
        let id = InterfaceId::new([0xAB; 32]);
        let display = format!("{}", id);
        assert_eq!(display, "abababababababab");
    }

    #[test]
    fn test_event_id_from_peer() {
        let peer = SimulationIdentity::new('A').unwrap();
        let id1 = EventId::from_peer(&peer, 1);
        let id2 = EventId::from_peer(&peer, 2);

        assert_eq!(id1.sender_hash, id2.sender_hash);
        assert_ne!(id1.sequence, id2.sequence);
    }

    #[test]
    fn test_interface_event_message() {
        let sender = SimulationIdentity::new('A').unwrap();
        let event = InterfaceEvent::message(sender, 1, b"Hello".to_vec());

        match event {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Hello");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_membership_change() {
        let creator = SimulationIdentity::new('A').unwrap();
        let invitee = SimulationIdentity::new('B').unwrap();

        let change = MembershipChange::Invited {
            by: creator,
            peer: invitee,
        };

        assert_eq!(change.actor(), Some(&creator));
        assert_eq!(change.affected_peer(), &invitee);
    }

    #[test]
    fn test_interface_metadata() {
        let creator = SimulationIdentity::new('A').unwrap();
        let id = InterfaceId::generate();

        let meta = InterfaceMetadata::new(id, creator)
            .with_name("Test Interface")
            .with_description("A test interface");

        assert_eq!(meta.name, Some("Test Interface".to_string()));
        assert!(meta.members.contains(&creator));
    }
}
