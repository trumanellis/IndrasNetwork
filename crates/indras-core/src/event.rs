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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationIdentity;

    #[test]
    fn test_drop_reason_display() {
        assert_eq!(format!("{}", DropReason::TtlExpired), "TTL expired");
        assert_eq!(format!("{}", DropReason::NoRoute), "No route available");
        assert_eq!(format!("{}", DropReason::Duplicate), "Duplicate packet");
        assert_eq!(format!("{}", DropReason::Expired), "Message expired");
        assert_eq!(format!("{}", DropReason::SenderOffline), "Sender offline");
        assert_eq!(format!("{}", DropReason::StorageFull), "Storage full");
        assert_eq!(format!("{}", DropReason::TooLarge), "Packet too large");
    }

    #[test]
    fn test_peer_discovered_factory() {
        let peer = SimulationIdentity::new('A').unwrap();
        let before = Utc::now();
        let event = NetworkEvent::peer_discovered(peer);
        let after = Utc::now();

        let ts = event.timestamp();
        assert!(ts >= before && ts <= after);

        if let NetworkEvent::PeerDiscovered {
            peer: event_peer, ..
        } = event
        {
            assert_eq!(event_peer, SimulationIdentity::new('A').unwrap());
        } else {
            panic!("Expected PeerDiscovered event");
        }
    }

    #[test]
    fn test_peer_lost_factory() {
        let peer = SimulationIdentity::new('B').unwrap();
        let event = NetworkEvent::peer_lost(peer);

        if let NetworkEvent::PeerLost {
            peer: event_peer, ..
        } = event
        {
            assert_eq!(event_peer, SimulationIdentity::new('B').unwrap());
        } else {
            panic!("Expected PeerLost event");
        }
    }

    #[test]
    fn test_packet_sent_factory() {
        let from = SimulationIdentity::new('A').unwrap();
        let to = SimulationIdentity::new('B').unwrap();
        let packet_id = PacketId::new(0xABCD, 1);

        let event = NetworkEvent::packet_sent(from.clone(), to.clone(), packet_id);

        if let NetworkEvent::PacketSent {
            from: event_from,
            to: event_to,
            packet_id: event_id,
            ..
        } = event
        {
            assert_eq!(event_from, from);
            assert_eq!(event_to, to);
            assert_eq!(event_id, packet_id);
        } else {
            panic!("Expected PacketSent event");
        }
    }

    #[test]
    fn test_packet_delivered_factory() {
        let to = SimulationIdentity::new('C').unwrap();
        let packet_id = PacketId::new(0xABCD, 1);

        let event = NetworkEvent::packet_delivered(to.clone(), packet_id);

        if let NetworkEvent::PacketDelivered {
            to: event_to,
            packet_id: event_id,
            ..
        } = event
        {
            assert_eq!(event_to, to);
            assert_eq!(event_id, packet_id);
        } else {
            panic!("Expected PacketDelivered event");
        }
    }

    #[test]
    fn test_packet_dropped_factory() {
        let packet_id = PacketId::new(0xABCD, 1);
        let reason = DropReason::TtlExpired;

        let event = NetworkEvent::<SimulationIdentity>::packet_dropped(packet_id, reason);

        if let NetworkEvent::PacketDropped {
            packet_id: event_id,
            reason: event_reason,
            ..
        } = event
        {
            assert_eq!(event_id, packet_id);
            assert_eq!(event_reason, DropReason::TtlExpired);
        } else {
            panic!("Expected PacketDropped event");
        }
    }

    #[test]
    fn test_timestamp_extraction_all_variants() {
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();
        let packet_id = PacketId::new(0xABCD, 1);
        let now = Utc::now();

        // Test all variants have timestamp extraction
        let events: Vec<NetworkEvent<SimulationIdentity>> = vec![
            NetworkEvent::PeerDiscovered {
                peer: peer_a.clone(),
                timestamp: now,
            },
            NetworkEvent::PeerLost {
                peer: peer_a.clone(),
                timestamp: now,
            },
            NetworkEvent::PeerAwake {
                peer: peer_a.clone(),
                timestamp: now,
            },
            NetworkEvent::PeerSleep {
                peer: peer_a.clone(),
                timestamp: now,
            },
            NetworkEvent::PacketSent {
                from: peer_a.clone(),
                to: peer_b.clone(),
                packet_id,
                timestamp: now,
            },
            NetworkEvent::PacketRelayed {
                from: peer_a.clone(),
                via: peer_b.clone(),
                to: peer_a.clone(),
                packet_id,
                timestamp: now,
            },
            NetworkEvent::PacketDelivered {
                to: peer_a.clone(),
                packet_id,
                timestamp: now,
            },
            NetworkEvent::PacketDropped {
                packet_id,
                reason: DropReason::TtlExpired,
                timestamp: now,
            },
            NetworkEvent::ConfirmationReceived {
                packet_id,
                from: peer_a.clone(),
                to: peer_b.clone(),
                timestamp: now,
            },
            NetworkEvent::BackPropStep {
                packet_id,
                from: peer_a.clone(),
                to: peer_b.clone(),
                timestamp: now,
            },
        ];

        for event in events {
            assert_eq!(event.timestamp(), now);
        }
    }

    #[test]
    fn test_drop_reason_equality() {
        assert_eq!(DropReason::TtlExpired, DropReason::TtlExpired);
        assert_ne!(DropReason::TtlExpired, DropReason::NoRoute);
    }

    #[test]
    fn test_drop_reason_serialization() {
        let reason = DropReason::StorageFull;
        let serialized = postcard::to_allocvec(&reason).unwrap();
        let deserialized: DropReason = postcard::from_bytes(&serialized).unwrap();
        assert_eq!(reason, deserialized);
    }

    #[test]
    fn test_network_event_serialization() {
        let peer = SimulationIdentity::new('A').unwrap();
        let event = NetworkEvent::peer_discovered(peer);

        let serialized = postcard::to_allocvec(&event).unwrap();
        let deserialized: NetworkEvent<SimulationIdentity> =
            postcard::from_bytes(&serialized).unwrap();

        // Can't directly compare due to timestamp, but we can check structure
        if let NetworkEvent::PeerDiscovered {
            peer: deser_peer, ..
        } = deserialized
        {
            assert_eq!(deser_peer, SimulationIdentity::new('A').unwrap());
        } else {
            panic!("Deserialization produced wrong variant");
        }
    }
}
