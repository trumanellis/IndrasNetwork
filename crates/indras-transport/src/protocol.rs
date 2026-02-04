//! Wire protocol for Indras Network
//!
//! Defines the ALPN identifier and message types for peer communication.
//!
//! ## N-Peer Interface Messages
//!
//! This module includes message types for N-peer interface synchronization:
//! - `InterfaceEvent`: Broadcast events to all interface members
//! - `InterfaceSyncRequest`: Request Automerge sync state from a peer
//! - `InterfaceSyncResponse`: Respond with sync data and pending events
//! - `InterfaceJoin`: Announce joining an interface
//! - `InterfaceLeave`: Announce leaving an interface

use bytes::Bytes;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use indras_core::packet::{DeliveryConfirmation, PacketId};
use indras_core::{EventId, InterfaceId, PresenceStatus};

use crate::identity::IrohIdentity;

// ============================================================================
// Protocol Handler for Router-based ALPN dispatch
// ============================================================================

/// Handler for incoming connections using the indras/1 ALPN protocol.
///
/// The Router dispatches incoming connections to this handler based on ALPN.
/// It forwards accepted connections to the adapter via an mpsc channel.
#[derive(Debug, Clone)]
pub struct IndrasProtocolHandler {
    sender: mpsc::Sender<(IrohIdentity, Connection)>,
}

impl IndrasProtocolHandler {
    /// Create a new protocol handler that forwards connections to the given channel.
    pub fn new(sender: mpsc::Sender<(IrohIdentity, Connection)>) -> Self {
        Self { sender }
    }
}

impl ProtocolHandler for IndrasProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let peer_key = connection.remote_id();
        let peer_id = IrohIdentity::new(peer_key);
        self.sender
            .send((peer_id, connection))
            .await
            .map_err(|_| {
                AcceptError::from(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "Connection forwarding channel closed",
                ))
            })?;
        Ok(())
    }
}

// ============================================================================

/// Application-Level Protocol Negotiation identifier for Indras
pub const ALPN_INDRAS: &[u8] = b"indras/1";

/// Maximum message size (1 MB)
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Wire messages exchanged between peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireMessage {
    /// A packet being transmitted or relayed
    Packet(SerializedPacket),

    /// Delivery confirmation for back-propagation
    Confirmation(SerializedConfirmation),

    /// Presence announcement from a peer
    PresenceAnnounce(PresenceInfo),

    /// Query for peer presence information
    PresenceQuery,

    /// Response to presence query
    PresenceResponse(Vec<PresenceInfo>),

    /// Request to sync state (for CRDT sync) - legacy
    SyncRequest(SyncRequest),

    /// Response with sync data - legacy
    SyncResponse(SyncResponse),

    /// Ping for keepalive
    Ping(u64),

    /// Pong response to ping
    Pong(u64),

    // ========== N-Peer Interface Messages ==========
    /// Interface event broadcast (encrypted with interface key)
    InterfaceEvent(InterfaceEventMessage),

    /// Request sync state for an interface
    InterfaceSyncRequest(InterfaceSyncRequestMessage),

    /// Response with interface sync data
    InterfaceSyncResponse(InterfaceSyncResponseMessage),

    /// Announce joining an interface
    InterfaceJoin(InterfaceJoinMessage),

    /// Announce leaving an interface
    InterfaceLeave(InterfaceLeaveMessage),

    /// Confirm receipt of events (for store-and-forward)
    InterfaceEventAck(InterfaceEventAckMessage),

    // ========== Peer Discovery Messages ==========
    /// Proactive introduction of a new peer to existing members
    PeerIntroduction(PeerIntroductionMessage),

    /// Request to learn about existing realm members
    IntroductionRequest(IntroductionRequestMessage),

    /// Response with known realm members
    IntroductionResponse(IntroductionResponseMessage),

    // ========== Direct Connection Messages ==========
    /// ML-KEM key exchange for establishing DM interface keys
    KeyExchange(KeyExchangeMessage),

    /// Encounter code exchange for in-person peer discovery
    EncounterExchange(EncounterExchangeMessage),
}

/// Serialized packet for wire transmission
///
/// Contains the serialized packet data plus routing metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedPacket {
    /// Unique packet identifier
    pub id: PacketId,
    /// Original source peer
    pub source: IrohIdentity,
    /// Final destination peer
    pub destination: IrohIdentity,
    /// Serialized and encrypted payload
    pub payload: Bytes,
    /// Routing hints (mutual peers who might reach destination)
    pub routing_hints: Vec<IrohIdentity>,
    /// Creation timestamp (Unix millis)
    pub created_at_millis: i64,
    /// Remaining TTL
    pub ttl: u8,
    /// Hashes of peers who have handled this packet
    pub visited: Vec<u64>,
}

/// Serialized delivery confirmation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedConfirmation {
    /// The packet that was delivered
    pub packet_id: PacketId,
    /// Who received the packet
    pub delivered_to: IrohIdentity,
    /// When it was delivered (Unix millis)
    pub delivered_at_millis: i64,
    /// The path the packet took
    pub path: Vec<IrohIdentity>,
}

impl From<DeliveryConfirmation<IrohIdentity>> for SerializedConfirmation {
    fn from(conf: DeliveryConfirmation<IrohIdentity>) -> Self {
        Self {
            packet_id: conf.packet_id,
            delivered_to: conf.delivered_to,
            delivered_at_millis: conf.delivered_at.timestamp_millis(),
            path: conf.path,
        }
    }
}

/// Information about a peer's presence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInfo {
    /// The peer's identity
    pub peer_id: IrohIdentity,
    /// Unix timestamp when this info was generated
    pub timestamp_millis: i64,
    /// List of peer's direct neighbors
    pub neighbors: Vec<IrohIdentity>,
    /// Whether the peer is accepting connections
    pub accepting_connections: bool,
    /// Optional human-readable name
    pub display_name: Option<String>,
    /// Additional metadata (serialized key-value pairs)
    pub metadata: Vec<u8>,
}

impl PresenceInfo {
    /// Create new presence info for a peer
    pub fn new(peer_id: IrohIdentity) -> Self {
        Self {
            peer_id,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            neighbors: vec![],
            accepting_connections: true,
            display_name: None,
            metadata: vec![],
        }
    }

    /// Set the display name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the neighbors
    pub fn with_neighbors(mut self, neighbors: Vec<IrohIdentity>) -> Self {
        self.neighbors = neighbors;
        self
    }
}

/// Request for CRDT sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    /// Document/namespace identifier
    pub namespace: [u8; 32],
    /// Vector of (actor_id, sequence_number) pairs representing local state
    pub heads: Vec<(IrohIdentity, u64)>,
}

/// Response with sync data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    /// Document/namespace identifier
    pub namespace: [u8; 32],
    /// Changes to apply (serialized operations)
    pub changes: Vec<Bytes>,
    /// Whether there are more changes to fetch
    pub has_more: bool,
}

// ============================================================================
// N-Peer Interface Message Types
// ============================================================================

/// Interface event broadcast message
///
/// Events are encrypted with the interface's shared symmetric key.
/// Only members with the key can decrypt the content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceEventMessage {
    /// The interface this event belongs to
    pub interface_id: InterfaceId,
    /// Encrypted event payload (encrypted with interface key)
    pub encrypted_event: Vec<u8>,
    /// Event ID for acknowledgment
    pub event_id: EventId,
    /// Nonce used for encryption (12 bytes for ChaCha20-Poly1305)
    pub nonce: [u8; 12],
}

impl InterfaceEventMessage {
    /// Create a new interface event message
    pub fn new(
        interface_id: InterfaceId,
        encrypted_event: Vec<u8>,
        event_id: EventId,
        nonce: [u8; 12],
    ) -> Self {
        Self {
            interface_id,
            encrypted_event,
            event_id,
            nonce,
        }
    }
}

/// Request sync state for an interface
///
/// Sent when a peer reconnects or needs to catch up on missed events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSyncRequestMessage {
    /// The interface to sync
    pub interface_id: InterfaceId,
    /// Our current Automerge document heads
    pub my_heads: Vec<[u8; 32]>,
    /// Last event ID we've seen (for store-and-forward catchup)
    pub last_event_id: Option<EventId>,
}

impl InterfaceSyncRequestMessage {
    /// Create a new sync request
    pub fn new(interface_id: InterfaceId, my_heads: Vec<[u8; 32]>) -> Self {
        Self {
            interface_id,
            my_heads,
            last_event_id: None,
        }
    }

    /// Set the last event ID we've received
    pub fn with_last_event(mut self, event_id: EventId) -> Self {
        self.last_event_id = Some(event_id);
        self
    }
}

/// Response with interface sync data
///
/// Contains both Automerge document sync data and pending events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSyncResponseMessage {
    /// The interface being synced
    pub interface_id: InterfaceId,
    /// Automerge sync message (changes since their heads)
    pub sync_data: Vec<u8>,
    /// Our current document heads
    pub our_heads: Vec<[u8; 32]>,
    /// Pending events (encrypted) that they missed
    pub pending_events: Vec<PendingEventData>,
}

/// Data for a pending event in sync response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEventData {
    /// Event ID
    pub event_id: EventId,
    /// Encrypted event payload
    pub encrypted_event: Vec<u8>,
    /// Encryption nonce
    pub nonce: [u8; 12],
}

impl InterfaceSyncResponseMessage {
    /// Create a new sync response
    pub fn new(interface_id: InterfaceId, sync_data: Vec<u8>, our_heads: Vec<[u8; 32]>) -> Self {
        Self {
            interface_id,
            sync_data,
            our_heads,
            pending_events: Vec::new(),
        }
    }

    /// Add pending events to the response
    pub fn with_pending_events(mut self, events: Vec<PendingEventData>) -> Self {
        self.pending_events = events;
        self
    }
}

/// Announce joining an interface
///
/// Sent when a peer joins an interface to announce their presence.
/// Includes post-quantum key material for secure peer-to-peer communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceJoinMessage {
    /// The interface being joined
    pub interface_id: InterfaceId,
    /// The peer's presence status
    pub presence_status: PresenceStatus,
    /// Timestamp (Unix millis)
    pub timestamp_millis: i64,
    /// Display name (optional)
    pub display_name: Option<String>,
    /// ML-KEM-768 encapsulation key (1,184 bytes) for receiving encrypted keys
    pub pq_encapsulation_key: Option<Vec<u8>>,
    /// ML-DSA-65 verifying key (1,952 bytes) for signature verification
    pub pq_verifying_key: Option<Vec<u8>>,
}

impl InterfaceJoinMessage {
    /// Create a new join message
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            presence_status: PresenceStatus::Online,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            display_name: None,
            pq_encapsulation_key: None,
            pq_verifying_key: None,
        }
    }

    /// Set the display name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the presence status
    pub fn with_status(mut self, status: PresenceStatus) -> Self {
        self.presence_status = status;
        self
    }

    /// Set the ML-KEM-768 encapsulation key
    pub fn with_pq_encapsulation_key(mut self, key: Vec<u8>) -> Self {
        self.pq_encapsulation_key = Some(key);
        self
    }

    /// Set the ML-DSA-65 verifying key
    pub fn with_pq_verifying_key(mut self, key: Vec<u8>) -> Self {
        self.pq_verifying_key = Some(key);
        self
    }
}

/// Announce leaving an interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceLeaveMessage {
    /// The interface being left
    pub interface_id: InterfaceId,
    /// Timestamp (Unix millis)
    pub timestamp_millis: i64,
}

impl InterfaceLeaveMessage {
    /// Create a new leave message
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Acknowledge receipt of events
///
/// Sent to confirm that events have been received and processed.
/// This allows the sender to clear their store-and-forward buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceEventAckMessage {
    /// The interface
    pub interface_id: InterfaceId,
    /// The last event ID that was successfully received
    pub up_to_event_id: EventId,
}

impl InterfaceEventAckMessage {
    /// Create a new ack message
    pub fn new(interface_id: InterfaceId, up_to_event_id: EventId) -> Self {
        Self {
            interface_id,
            up_to_event_id,
        }
    }
}

// ============================================================================
// Peer Discovery Message Types
// ============================================================================

/// Information about a realm peer for discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealmPeerInfo {
    /// The peer's iroh identity
    pub peer_id: IrohIdentity,
    /// Display name (optional)
    pub display_name: Option<String>,
    /// ML-KEM-768 encapsulation key (for receiving encrypted keys)
    pub pq_encapsulation_key: Option<Vec<u8>>,
    /// ML-DSA-65 verifying key (for signature verification)
    pub pq_verifying_key: Option<Vec<u8>>,
    /// Timestamp when this peer info was created (Unix millis)
    pub timestamp_millis: i64,
}

impl RealmPeerInfo {
    /// Create new peer info
    pub fn new(peer_id: IrohIdentity) -> Self {
        Self {
            peer_id,
            display_name: None,
            pq_encapsulation_key: None,
            pq_verifying_key: None,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Set the display name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the ML-KEM-768 encapsulation key
    pub fn with_pq_encapsulation_key(mut self, key: Vec<u8>) -> Self {
        self.pq_encapsulation_key = Some(key);
        self
    }

    /// Set the ML-DSA-65 verifying key
    pub fn with_pq_verifying_key(mut self, key: Vec<u8>) -> Self {
        self.pq_verifying_key = Some(key);
        self
    }
}

/// Proactive introduction of a peer to realm members
///
/// Sent by existing members when a new peer joins to ensure
/// all members learn about the new peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerIntroductionMessage {
    /// The interface/realm this introduction is for
    pub interface_id: InterfaceId,
    /// Information about the peer being introduced
    pub peer_info: RealmPeerInfo,
    /// Timestamp (Unix millis)
    pub timestamp_millis: i64,
}

impl PeerIntroductionMessage {
    /// Create a new peer introduction message
    pub fn new(interface_id: InterfaceId, peer_info: RealmPeerInfo) -> Self {
        Self {
            interface_id,
            peer_info,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Request to discover existing realm members
///
/// Sent by a peer joining a realm to catch up on members
/// that may have been missed due to gossip unreliability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntroductionRequestMessage {
    /// The interface/realm to request members for
    pub interface_id: InterfaceId,
    /// Peers we already know about (to avoid redundant responses)
    pub known_peers: Vec<IrohIdentity>,
    /// Timestamp (Unix millis)
    pub timestamp_millis: i64,
}

impl IntroductionRequestMessage {
    /// Create a new introduction request
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            known_peers: Vec::new(),
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Set the list of already-known peers
    pub fn with_known_peers(mut self, peers: Vec<IrohIdentity>) -> Self {
        self.known_peers = peers;
        self
    }
}

/// Response with known realm members
///
/// Sent in response to IntroductionRequest with information
/// about realm members the requester doesn't know about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntroductionResponseMessage {
    /// The interface/realm this response is for
    pub interface_id: InterfaceId,
    /// Information about known members
    pub members: Vec<RealmPeerInfo>,
    /// Timestamp (Unix millis)
    pub timestamp_millis: i64,
}

impl IntroductionResponseMessage {
    /// Create a new introduction response
    pub fn new(interface_id: InterfaceId, members: Vec<RealmPeerInfo>) -> Self {
        Self {
            interface_id,
            members,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }
}

// ============================================================================
// Direct Connection Message Types
// ============================================================================

/// ML-KEM key exchange message for establishing DM interface keys.
///
/// Sent on the DM gossip topic when the initiator (lower MemberId)
/// encapsulates a shared secret to the peer's PQ encapsulation key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyExchangeMessage {
    /// The DM interface/realm this key exchange is for.
    pub interface_id: InterfaceId,
    /// The sender's member ID (32-byte public key).
    pub sender_id: [u8; 32],
    /// ML-KEM ciphertext (encapsulated shared secret).
    pub kem_ciphertext: Vec<u8>,
    /// Timestamp (Unix millis).
    pub timestamp_millis: i64,
}

impl KeyExchangeMessage {
    /// Create a new key exchange message.
    pub fn new(interface_id: InterfaceId, sender_id: [u8; 32], kem_ciphertext: Vec<u8>) -> Self {
        Self {
            interface_id,
            sender_id,
            kem_ciphertext,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Encounter code exchange message for in-person peer discovery.
///
/// Sent on an encounter gossip topic (derived from a 6-digit code + time window).
/// Contains the sender's MemberId so the other party can call `connect()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterExchangeMessage {
    /// The encounter topic this exchange is for.
    pub interface_id: InterfaceId,
    /// The sender's member ID (32-byte public key).
    pub member_id: [u8; 32],
    /// Optional display name.
    pub display_name: Option<String>,
    /// Timestamp (Unix millis).
    pub timestamp_millis: i64,
}

impl EncounterExchangeMessage {
    /// Create a new encounter exchange message.
    pub fn new(interface_id: InterfaceId, member_id: [u8; 32]) -> Self {
        Self {
            interface_id,
            member_id,
            display_name: None,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Set the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }
}

/// Frame a message for wire transmission
///
/// Returns the framed message as bytes (length-prefixed).
pub fn frame_message(msg: &WireMessage) -> Result<Bytes, FramingError> {
    let serialized =
        postcard::to_allocvec(msg).map_err(|e| FramingError::Serialization(e.to_string()))?;

    if serialized.len() > MAX_MESSAGE_SIZE {
        return Err(FramingError::MessageTooLarge {
            size: serialized.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }

    // Length-prefix with 4 bytes (big-endian)
    let len = serialized.len() as u32;
    let mut framed = Vec::with_capacity(4 + serialized.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(&serialized);

    Ok(Bytes::from(framed))
}

/// Parse a framed message from bytes
///
/// Expects length-prefixed format.
pub fn parse_framed_message(data: &[u8]) -> Result<WireMessage, FramingError> {
    if data.len() < 4 {
        return Err(FramingError::InsufficientData {
            needed: 4,
            available: data.len(),
        });
    }

    let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;

    if len > MAX_MESSAGE_SIZE {
        return Err(FramingError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    if data.len() < 4 + len {
        return Err(FramingError::InsufficientData {
            needed: 4 + len,
            available: data.len(),
        });
    }

    postcard::from_bytes(&data[4..4 + len])
        .map_err(|e| FramingError::Deserialization(e.to_string()))
}

/// Errors that can occur during message framing
#[derive(Debug, Clone, thiserror::Error)]
pub enum FramingError {
    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Insufficient data: need {needed} bytes, have {available}")]
    InsufficientData { needed: usize, available: usize },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_message_roundtrip() {
        let msg = WireMessage::Ping(42);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::Ping(n) => assert_eq!(n, 42),
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_presence_info() {
        use crate::identity::IrohIdentity;
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let id = IrohIdentity::new(secret.public());

        let presence = PresenceInfo::new(id)
            .with_name("TestPeer")
            .with_neighbors(vec![]);

        let msg = WireMessage::PresenceAnnounce(presence.clone());
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::PresenceAnnounce(p) => {
                assert_eq!(p.display_name, Some("TestPeer".to_string()));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_message_too_large() {
        // Create a message that's too large
        let large_payload = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let msg = WireMessage::SyncResponse(SyncResponse {
            namespace: [0u8; 32],
            changes: vec![Bytes::from(large_payload)],
            has_more: false,
        });

        let result = frame_message(&msg);
        assert!(matches!(result, Err(FramingError::MessageTooLarge { .. })));
    }

    #[test]
    fn test_interface_event_message() {
        use indras_core::InterfaceId;

        let interface_id = InterfaceId::new([0x42; 32]);
        let event_id = indras_core::EventId::new(12345, 1);
        let nonce = [0x11; 12];
        let encrypted = vec![1, 2, 3, 4, 5];

        let msg = WireMessage::InterfaceEvent(InterfaceEventMessage::new(
            interface_id,
            encrypted.clone(),
            event_id,
            nonce,
        ));

        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::InterfaceEvent(e) => {
                assert_eq!(e.interface_id, interface_id);
                assert_eq!(e.event_id, event_id);
                assert_eq!(e.nonce, nonce);
                assert_eq!(e.encrypted_event, encrypted);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_interface_sync_messages() {
        use indras_core::InterfaceId;

        let interface_id = InterfaceId::new([0xAB; 32]);
        let heads = vec![[0x01; 32], [0x02; 32]];

        // Test sync request
        let request = InterfaceSyncRequestMessage::new(interface_id, heads.clone());
        let msg = WireMessage::InterfaceSyncRequest(request);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::InterfaceSyncRequest(r) => {
                assert_eq!(r.interface_id, interface_id);
                assert_eq!(r.my_heads, heads);
                assert!(r.last_event_id.is_none());
            }
            _ => panic!("Wrong message type"),
        }

        // Test sync response
        let sync_data = vec![10, 20, 30];
        let response =
            InterfaceSyncResponseMessage::new(interface_id, sync_data.clone(), heads.clone());
        let msg = WireMessage::InterfaceSyncResponse(response);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::InterfaceSyncResponse(r) => {
                assert_eq!(r.interface_id, interface_id);
                assert_eq!(r.sync_data, sync_data);
                assert_eq!(r.our_heads, heads);
                assert!(r.pending_events.is_empty());
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_interface_join_leave() {
        use indras_core::{InterfaceId, PresenceStatus};

        let interface_id = InterfaceId::new([0xCD; 32]);

        // Test join
        let join = InterfaceJoinMessage::new(interface_id)
            .with_name("TestPeer")
            .with_status(PresenceStatus::Online);
        let msg = WireMessage::InterfaceJoin(join);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::InterfaceJoin(j) => {
                assert_eq!(j.interface_id, interface_id);
                assert_eq!(j.display_name, Some("TestPeer".to_string()));
                assert_eq!(j.presence_status, PresenceStatus::Online);
            }
            _ => panic!("Wrong message type"),
        }

        // Test leave
        let leave = InterfaceLeaveMessage::new(interface_id);
        let msg = WireMessage::InterfaceLeave(leave);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::InterfaceLeave(l) => {
                assert_eq!(l.interface_id, interface_id);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_interface_event_ack() {
        use indras_core::InterfaceId;

        let interface_id = InterfaceId::new([0xEF; 32]);
        let event_id = indras_core::EventId::new(99999, 42);

        let ack = InterfaceEventAckMessage::new(interface_id, event_id);
        let msg = WireMessage::InterfaceEventAck(ack);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::InterfaceEventAck(a) => {
                assert_eq!(a.interface_id, interface_id);
                assert_eq!(a.up_to_event_id, event_id);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_empty_frame_error() {
        let result = parse_framed_message(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_frame_error() {
        // Just 2 bytes (less than length header)
        let result = parse_framed_message(&[0x00, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_length_error() {
        // Length says 1000 bytes but only 4 bytes follow
        let data = [0x00, 0x00, 0x03, 0xE8, 0x01, 0x02, 0x03, 0x04];
        let result = parse_framed_message(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_pong_roundtrip() {
        let msg = WireMessage::Pong(99999);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::Pong(n) => assert_eq!(n, 99999),
            _ => panic!("Expected Pong"),
        }
    }

    #[test]
    fn test_presence_query_roundtrip() {
        let msg = WireMessage::PresenceQuery;
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        assert!(matches!(parsed, WireMessage::PresenceQuery));
    }

    #[test]
    fn test_serialized_packet_roundtrip() {
        use indras_core::packet::PacketId;
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());
        let source = IrohIdentity::new(secret1.public());
        let dest = IrohIdentity::new(secret2.public());

        let packet = SerializedPacket {
            id: PacketId::new(0x12345678, 1),
            source,
            destination: dest,
            payload: Bytes::from(vec![1, 2, 3, 4, 5]),
            routing_hints: vec![],
            created_at_millis: chrono::Utc::now().timestamp_millis(),
            ttl: 10,
            visited: vec![0xABCD, 0xEF01],
        };

        let msg = WireMessage::Packet(packet.clone());
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::Packet(p) => {
                assert_eq!(p.id, packet.id);
                assert_eq!(p.source, packet.source);
                assert_eq!(p.destination, packet.destination);
                assert_eq!(p.payload, packet.payload);
                assert_eq!(p.ttl, packet.ttl);
                assert_eq!(p.visited, packet.visited);
            }
            _ => panic!("Expected Packet"),
        }
    }

    #[test]
    fn test_serialized_confirmation_roundtrip() {
        use indras_core::packet::PacketId;
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let peer = IrohIdentity::new(secret.public());

        let confirmation = SerializedConfirmation {
            packet_id: PacketId::new(0xDEADBEEF, 42),
            delivered_to: peer,
            delivered_at_millis: chrono::Utc::now().timestamp_millis(),
            path: vec![peer],
        };

        let msg = WireMessage::Confirmation(confirmation.clone());
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::Confirmation(c) => {
                assert_eq!(c.packet_id, confirmation.packet_id);
                assert_eq!(c.delivered_to, confirmation.delivered_to);
            }
            _ => panic!("Expected Confirmation"),
        }
    }

    #[test]
    fn test_sync_request_response_roundtrip() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let peer = IrohIdentity::new(secret.public());

        let request = SyncRequest {
            namespace: [0x42; 32],
            heads: vec![(peer, 100)],
        };

        let msg = WireMessage::SyncRequest(request.clone());
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::SyncRequest(r) => {
                assert_eq!(r.namespace, request.namespace);
                assert_eq!(r.heads.len(), 1);
            }
            _ => panic!("Expected SyncRequest"),
        }

        let response = SyncResponse {
            namespace: [0x42; 32],
            changes: vec![Bytes::from(vec![1, 2, 3])],
            has_more: true,
        };

        let msg = WireMessage::SyncResponse(response.clone());
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::SyncResponse(r) => {
                assert_eq!(r.namespace, response.namespace);
                assert_eq!(r.has_more, response.has_more);
            }
            _ => panic!("Expected SyncResponse"),
        }
    }

    #[test]
    fn test_presence_response_roundtrip() {
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());
        let peer1 = IrohIdentity::new(secret1.public());
        let peer2 = IrohIdentity::new(secret2.public());

        let presences = vec![
            PresenceInfo::new(peer1).with_name("Peer1"),
            PresenceInfo::new(peer2).with_name("Peer2"),
        ];

        let msg = WireMessage::PresenceResponse(presences.clone());
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::PresenceResponse(p) => {
                assert_eq!(p.len(), 2);
                assert_eq!(p[0].display_name, Some("Peer1".to_string()));
                assert_eq!(p[1].display_name, Some("Peer2".to_string()));
            }
            _ => panic!("Expected PresenceResponse"),
        }
    }

    #[test]
    fn test_interface_join_with_pq_keys() {
        use indras_core::InterfaceId;

        let interface_id = InterfaceId::new([0xAB; 32]);
        let pq_encap_key = vec![0x42; 1184]; // ML-KEM-768 size
        let pq_verify_key = vec![0x43; 1952]; // ML-DSA-65 size

        let join = InterfaceJoinMessage::new(interface_id)
            .with_name("TestPeer")
            .with_pq_encapsulation_key(pq_encap_key.clone())
            .with_pq_verifying_key(pq_verify_key.clone());

        let msg = WireMessage::InterfaceJoin(join);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::InterfaceJoin(j) => {
                assert_eq!(j.interface_id, interface_id);
                assert_eq!(j.display_name, Some("TestPeer".to_string()));
                assert_eq!(j.pq_encapsulation_key, Some(pq_encap_key));
                assert_eq!(j.pq_verifying_key, Some(pq_verify_key));
            }
            _ => panic!("Expected InterfaceJoin"),
        }
    }

    #[test]
    fn test_peer_introduction_roundtrip() {
        use indras_core::InterfaceId;
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let peer = IrohIdentity::new(secret.public());
        let interface_id = InterfaceId::new([0xCD; 32]);

        let peer_info = RealmPeerInfo::new(peer)
            .with_name("NewPeer")
            .with_pq_encapsulation_key(vec![0x11; 1184])
            .with_pq_verifying_key(vec![0x22; 1952]);

        let intro = PeerIntroductionMessage::new(interface_id, peer_info);
        let msg = WireMessage::PeerIntroduction(intro);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::PeerIntroduction(p) => {
                assert_eq!(p.interface_id, interface_id);
                assert_eq!(p.peer_info.peer_id, peer);
                assert_eq!(p.peer_info.display_name, Some("NewPeer".to_string()));
                assert!(p.peer_info.pq_encapsulation_key.is_some());
                assert!(p.peer_info.pq_verifying_key.is_some());
            }
            _ => panic!("Expected PeerIntroduction"),
        }
    }

    #[test]
    fn test_introduction_request_roundtrip() {
        use indras_core::InterfaceId;
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());
        let known_peer1 = IrohIdentity::new(secret1.public());
        let known_peer2 = IrohIdentity::new(secret2.public());
        let interface_id = InterfaceId::new([0xEF; 32]);

        let request =
            IntroductionRequestMessage::new(interface_id).with_known_peers(vec![known_peer1, known_peer2]);

        let msg = WireMessage::IntroductionRequest(request);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::IntroductionRequest(r) => {
                assert_eq!(r.interface_id, interface_id);
                assert_eq!(r.known_peers.len(), 2);
                assert!(r.known_peers.contains(&known_peer1));
                assert!(r.known_peers.contains(&known_peer2));
            }
            _ => panic!("Expected IntroductionRequest"),
        }
    }

    #[test]
    fn test_introduction_response_roundtrip() {
        use indras_core::InterfaceId;
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());
        let peer1 = IrohIdentity::new(secret1.public());
        let peer2 = IrohIdentity::new(secret2.public());
        let interface_id = InterfaceId::new([0x12; 32]);

        let members = vec![
            RealmPeerInfo::new(peer1).with_name("Alice"),
            RealmPeerInfo::new(peer2).with_name("Bob"),
        ];

        let response = IntroductionResponseMessage::new(interface_id, members);
        let msg = WireMessage::IntroductionResponse(response);
        let framed = frame_message(&msg).unwrap();
        let parsed = parse_framed_message(&framed).unwrap();

        match parsed {
            WireMessage::IntroductionResponse(r) => {
                assert_eq!(r.interface_id, interface_id);
                assert_eq!(r.members.len(), 2);
                assert_eq!(r.members[0].display_name, Some("Alice".to_string()));
                assert_eq!(r.members[1].display_name, Some("Bob".to_string()));
            }
            _ => panic!("Expected IntroductionResponse"),
        }
    }

    #[test]
    fn test_realm_peer_info_creation() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let peer = IrohIdentity::new(secret.public());

        let info = RealmPeerInfo::new(peer)
            .with_name("TestPeer")
            .with_pq_encapsulation_key(vec![1, 2, 3])
            .with_pq_verifying_key(vec![4, 5, 6]);

        assert_eq!(info.peer_id, peer);
        assert_eq!(info.display_name, Some("TestPeer".to_string()));
        assert_eq!(info.pq_encapsulation_key, Some(vec![1, 2, 3]));
        assert_eq!(info.pq_verifying_key, Some(vec![4, 5, 6]));
        assert!(info.timestamp_millis > 0);
    }
}
