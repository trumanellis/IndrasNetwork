//! Network events

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::PeerIdentity;
use crate::packet::PacketId;

/// Events that occur in the network
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub enum NetworkEvent<I: PeerIdentity> {
    /// A new peer was discovered
    PeerDiscovered {
        peer: I,
        timestamp: DateTime<Utc>,
    },

    /// A peer went offline or became unreachable
    PeerLost {
        peer: I,
        timestamp: DateTime<Utc>,
    },

    /// Peer came online (simulation-specific)
    PeerAwake {
        peer: I,
        timestamp: DateTime<Utc>,
    },

    /// Peer went offline (simulation-specific)
    PeerSleep {
        peer: I,
        timestamp: DateTime<Utc>,
    },

    /// A packet was sent
    PacketSent {
        from: I,
        to: I,
        packet_id: PacketId,
        timestamp: DateTime<Utc>,
    },

    /// A packet was relayed through an intermediate peer
    PacketRelayed {
        from: I,
        via: I,
        to: I,
        packet_id: PacketId,
        timestamp: DateTime<Utc>,
    },

    /// A packet was delivered to its destination
    PacketDelivered {
        to: I,
        packet_id: PacketId,
        timestamp: DateTime<Utc>,
    },

    /// A packet was dropped
    PacketDropped {
        packet_id: PacketId,
        reason: DropReason,
        timestamp: DateTime<Utc>,
    },

    /// A delivery confirmation was received (back-propagation)
    ConfirmationReceived {
        packet_id: PacketId,
        from: I,
        to: I,
        timestamp: DateTime<Utc>,
    },

    /// Back-propagation step completed
    BackPropStep {
        packet_id: PacketId,
        from: I,
        to: I,
        timestamp: DateTime<Utc>,
    },
}

impl<I: PeerIdentity> NetworkEvent<I> {
    /// Get the timestamp of this event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::PeerDiscovered { timestamp, .. } => *timestamp,
            Self::PeerLost { timestamp, .. } => *timestamp,
            Self::PeerAwake { timestamp, .. } => *timestamp,
            Self::PeerSleep { timestamp, .. } => *timestamp,
            Self::PacketSent { timestamp, .. } => *timestamp,
            Self::PacketRelayed { timestamp, .. } => *timestamp,
            Self::PacketDelivered { timestamp, .. } => *timestamp,
            Self::PacketDropped { timestamp, .. } => *timestamp,
            Self::ConfirmationReceived { timestamp, .. } => *timestamp,
            Self::BackPropStep { timestamp, .. } => *timestamp,
        }
    }

    /// Create a peer discovered event
    pub fn peer_discovered(peer: I) -> Self {
        Self::PeerDiscovered {
            peer,
            timestamp: Utc::now(),
        }
    }

    /// Create a peer lost event
    pub fn peer_lost(peer: I) -> Self {
        Self::PeerLost {
            peer,
            timestamp: Utc::now(),
        }
    }

    /// Create a packet sent event
    pub fn packet_sent(from: I, to: I, packet_id: PacketId) -> Self {
        Self::PacketSent {
            from,
            to,
            packet_id,
            timestamp: Utc::now(),
        }
    }

    /// Create a packet delivered event
    pub fn packet_delivered(to: I, packet_id: PacketId) -> Self {
        Self::PacketDelivered {
            to,
            packet_id,
            timestamp: Utc::now(),
        }
    }

    /// Create a packet dropped event
    pub fn packet_dropped(packet_id: PacketId, reason: DropReason) -> Self {
        Self::PacketDropped {
            packet_id,
            reason,
            timestamp: Utc::now(),
        }
    }
}

/// Reasons a packet might be dropped
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DropReason {
    /// TTL (time-to-live / hop count) expired
    TtlExpired,
    /// No route available to destination
    NoRoute,
    /// Duplicate packet (already processed)
    Duplicate,
    /// Message expired (waited too long in queue)
    Expired,
    /// Sender never came online after max retries
    SenderOffline,
    /// Storage capacity exceeded
    StorageFull,
    /// Packet too large
    TooLarge,
}

impl std::fmt::Display for DropReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TtlExpired => write!(f, "TTL expired"),
            Self::NoRoute => write!(f, "No route available"),
            Self::Duplicate => write!(f, "Duplicate packet"),
            Self::Expired => write!(f, "Message expired"),
            Self::SenderOffline => write!(f, "Sender offline"),
            Self::StorageFull => write!(f, "Storage full"),
            Self::TooLarge => write!(f, "Packet too large"),
        }
    }
}
