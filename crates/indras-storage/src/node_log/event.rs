//! Node-level event types

use serde::{Deserialize, Serialize};

use indras_core::{EventId, InterfaceId};

/// Events recorded in the node-level event log
///
/// These capture the *fact* of each state-mutating action at the node level.
/// Payload data stays in per-interface logs; this is a unified audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeEvent {
    // — Lifecycle —

    /// Node started up
    NodeStarted {
        /// Fingerprint of the node's transport identity (first 32 bytes of public key)
        identity_fingerprint: [u8; 32],
    },
    /// Node shut down gracefully
    NodeStopped,

    // — Interface management —

    /// A new interface was created
    InterfaceCreated {
        /// The interface ID
        interface_id: InterfaceId,
        /// Optional human-readable name
        name: Option<String>,
    },
    /// Joined an existing interface (via invite)
    InterfaceJoined {
        /// The interface ID
        interface_id: InterfaceId,
    },
    /// Left an interface
    InterfaceLeft {
        /// The interface ID
        interface_id: InterfaceId,
    },

    // — Membership —

    /// A new member was added to an interface
    MemberAdded {
        /// The interface ID
        interface_id: InterfaceId,
        /// The peer's public key bytes
        peer_id: Vec<u8>,
    },

    // — Inbound messages (after successful processing) —

    /// An encrypted event was received and processed
    EventReceived {
        /// The interface ID
        interface_id: InterfaceId,
        /// The event ID
        event_id: EventId,
        /// Sender's public key bytes
        sender: Vec<u8>,
        /// Size of the encrypted payload
        payload_size: u32,
    },
    /// A sync message was received and merged
    SyncReceived {
        /// The interface ID
        interface_id: InterfaceId,
        /// Peer's public key bytes
        peer: Vec<u8>,
        /// Bytes merged
        bytes_merged: u32,
    },
    /// An event acknowledgment was received
    EventAckReceived {
        /// The interface ID
        interface_id: InterfaceId,
        /// Peer's public key bytes
        peer: Vec<u8>,
        /// Acknowledged up to this event
        up_to: EventId,
    },

    // — Outbound messages (after successful send) —

    /// An event was sent to a peer
    EventSent {
        /// The interface ID
        interface_id: InterfaceId,
        /// The event ID
        event_id: EventId,
        /// Recipient's public key bytes
        recipient: Vec<u8>,
    },
    /// A sync message was sent to a peer
    SyncSent {
        /// The interface ID
        interface_id: InterfaceId,
        /// Peer's public key bytes
        peer: Vec<u8>,
        /// Bytes sent
        bytes_sent: u32,
    },
}
