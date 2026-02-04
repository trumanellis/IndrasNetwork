//! Background message handler for processing incoming network messages
//!
//! Handles:
//! - Incoming interface events (verify signature, decrypt, append, broadcast)
//! - Sync requests (generate sync response)
//! - Sync responses (apply incoming sync)
//! - Event acknowledgments (mark delivered)
//!
//! ## Post-Quantum Signatures
//!
//! All network messages are signed with ML-DSA-65 signatures for
//! quantum-resistant authentication at the application layer.

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use indras_core::transport::Transport;
use indras_core::{EventId, InterfaceEvent, InterfaceId, NInterfaceTrait, PeerIdentity};
use indras_crypto::{InterfaceKey, PQIdentity, PQPublicIdentity, PQSignature};
use indras_storage::CompositeStorage;
use indras_transport::{IrohIdentity, IrohNetworkAdapter};

use crate::{InterfaceState, ReceivedEvent};

/// Message types for the P2P protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// An encrypted interface event
    InterfaceEvent(InterfaceEventMessage),
    /// A sync request
    SyncRequest(InterfaceSyncRequest),
    /// A sync response
    SyncResponse(InterfaceSyncResponse),
    /// Acknowledge receipt of events
    EventAck(EventAckMessage),
}

impl NetworkMessage {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(data)
    }
}

/// Current protocol version for signed messages
pub const SIGNED_MESSAGE_VERSION: u8 = 1;

/// A signed network message with ML-DSA-65 signature
///
/// All messages sent over the network are wrapped with a post-quantum
/// signature for application-layer authentication.
///
/// ## Size Overhead
///
/// - Signature: ~3,309 bytes
/// - Verifying key: ~1,952 bytes
/// - Total overhead: ~5.3 KB per message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedNetworkMessage {
    /// Protocol version (must match SIGNED_MESSAGE_VERSION)
    pub version: u8,
    /// The signed message
    pub message: NetworkMessage,
    /// ML-DSA-65 signature (~3,309 bytes)
    pub signature: Vec<u8>,
    /// Sender's verifying key (~1,952 bytes)
    pub sender_verifying_key: Vec<u8>,
}

impl SignedNetworkMessage {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(data)
    }

    /// Verify the signature on this message
    ///
    /// Also checks that the protocol version is supported.
    pub fn verify(&self) -> Result<bool, MessageError> {
        // Check protocol version
        if self.version != SIGNED_MESSAGE_VERSION {
            return Err(MessageError::UnsupportedVersion(
                self.version,
                SIGNED_MESSAGE_VERSION,
            ));
        }

        let verifying_key = PQPublicIdentity::from_bytes(&self.sender_verifying_key)
            .map_err(|e| MessageError::InvalidSignature(e.to_string()))?;

        let signature = PQSignature::from_bytes(self.signature.clone())
            .map_err(|e| MessageError::InvalidSignature(e.to_string()))?;

        let message_bytes = self
            .message
            .to_bytes()
            .map_err(|e| MessageError::Serialization(e.to_string()))?;

        Ok(verifying_key.verify(&message_bytes, &signature))
    }

    /// Get the sender's public identity
    pub fn sender_identity(&self) -> Result<PQPublicIdentity, MessageError> {
        PQPublicIdentity::from_bytes(&self.sender_verifying_key)
            .map_err(|e| MessageError::InvalidSignature(e.to_string()))
    }
}

/// An encrypted interface event message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceEventMessage {
    /// The interface this event belongs to
    pub interface_id: InterfaceId,
    /// Encrypted event data
    pub ciphertext: Vec<u8>,
    /// Event ID for tracking
    pub event_id: EventId,
    /// Nonce used for encryption
    pub nonce: [u8; 12],
}

impl InterfaceEventMessage {
    /// Create a new interface event message
    pub fn new(
        interface_id: InterfaceId,
        ciphertext: Vec<u8>,
        event_id: EventId,
        nonce: [u8; 12],
    ) -> Self {
        Self {
            interface_id,
            ciphertext,
            event_id,
            nonce,
        }
    }
}

/// A sync request for an interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSyncRequest {
    /// The interface to sync
    pub interface_id: InterfaceId,
    /// Our current heads (for sync protocol)
    pub heads: Vec<[u8; 32]>,
    /// Sync data (Automerge sync message)
    pub sync_data: Vec<u8>,
}

/// A sync response for an interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSyncResponse {
    /// The interface this sync is for
    pub interface_id: InterfaceId,
    /// Response sync data
    pub sync_data: Vec<u8>,
    /// Updated heads after sync
    pub heads: Vec<[u8; 32]>,
}

/// Acknowledge receipt of events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAckMessage {
    /// The interface
    pub interface_id: InterfaceId,
    /// Acknowledge all events up to and including this one
    pub up_to: EventId,
}

/// Background message handler
pub struct MessageHandler {
    /// Our identity (reserved for future use)
    #[allow(dead_code)]
    local_identity: IrohIdentity,
    /// Interface keys for decryption
    interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
    /// Loaded interfaces
    interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
    /// Storage
    storage: Arc<CompositeStorage<IrohIdentity>>,
    /// Transport for sending sync responses
    transport: Arc<IrohNetworkAdapter>,
    /// PQ identity for signing outgoing messages
    pq_identity: Arc<PQIdentity>,
    /// Whether to allow unsigned (legacy) messages
    ///
    /// When false, unsigned messages will be rejected with an error.
    /// Set to false in production to enforce PQ signatures.
    allow_legacy_unsigned: bool,
    /// Shutdown signal
    shutdown_rx: broadcast::Receiver<()>,
}

impl MessageHandler {
    /// Create a new message handler
    ///
    /// # Arguments
    ///
    /// * `allow_legacy_unsigned` - If true, accepts unsigned (legacy) messages with a warning.
    ///   Set to false in production to enforce PQ signatures.
    pub fn new(
        local_identity: IrohIdentity,
        interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
        interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
        storage: Arc<CompositeStorage<IrohIdentity>>,
        transport: Arc<IrohNetworkAdapter>,
        pq_identity: Arc<PQIdentity>,
        allow_legacy_unsigned: bool,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            local_identity,
            interface_keys,
            interfaces,
            storage,
            transport,
            pq_identity,
            allow_legacy_unsigned,
            shutdown_rx,
        }
    }

    /// Spawn the message handler as a background task
    ///
    /// # Arguments
    ///
    /// * `allow_legacy_unsigned` - If true, accepts unsigned (legacy) messages with a warning.
    ///   Set to false in production to enforce PQ signatures.
    pub fn spawn(
        local_identity: IrohIdentity,
        interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
        interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
        storage: Arc<CompositeStorage<IrohIdentity>>,
        transport: Arc<IrohNetworkAdapter>,
        pq_identity: Arc<PQIdentity>,
        allow_legacy_unsigned: bool,
        shutdown_rx: broadcast::Receiver<()>,
        message_rx: tokio::sync::mpsc::Receiver<(IrohIdentity, Vec<u8>)>,
    ) -> JoinHandle<()> {
        let handler = Self::new(
            local_identity,
            interface_keys,
            interfaces,
            storage,
            transport,
            pq_identity,
            allow_legacy_unsigned,
            shutdown_rx,
        );

        tokio::spawn(async move {
            handler.run(message_rx).await;
        })
    }

    /// Run the message handler loop
    async fn run(mut self, mut message_rx: tokio::sync::mpsc::Receiver<(IrohIdentity, Vec<u8>)>) {
        info!("Message handler started");

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Message handler shutting down");
                    break;
                }
                Some((sender, data)) = message_rx.recv() => {
                    if let Err(e) = self.handle_message(sender, data).await {
                        error!(error = %e, "Failed to handle message");
                    }
                }
            }
        }
    }

    /// Handle an incoming message
    ///
    /// Supports both signed (PQ) and unsigned (legacy) messages during transition.
    /// Legacy support can be disabled by setting `allow_legacy_unsigned` to false.
    async fn handle_message(
        &self,
        sender: IrohIdentity,
        data: Vec<u8>,
    ) -> Result<(), MessageError> {
        // Try to parse as signed message first
        if let Ok(signed_msg) = SignedNetworkMessage::from_bytes(&data) {
            return self.handle_signed_message(sender, signed_msg).await;
        }

        // Check if legacy unsigned messages are allowed
        if !self.allow_legacy_unsigned {
            error!(
                sender = %sender.short_id(),
                "Rejected unsigned message (legacy mode disabled)"
            );
            return Err(MessageError::LegacyModeDisabled);
        }

        // Fall back to unsigned message (legacy support during transition)
        let message = NetworkMessage::from_bytes(&data)
            .map_err(|e| MessageError::Deserialization(e.to_string()))?;

        warn!(
            sender = %sender.short_id(),
            "Received unsigned message (legacy mode)"
        );

        self.dispatch_message(sender, message).await
    }

    /// Handle a signed network message
    async fn handle_signed_message(
        &self,
        sender: IrohIdentity,
        signed_msg: SignedNetworkMessage,
    ) -> Result<(), MessageError> {
        // Verify signature
        if !signed_msg.verify()? {
            return Err(MessageError::SignatureVerificationFailed);
        }

        debug!(
            sender = %sender.short_id(),
            pq_sender = %signed_msg.sender_identity()?.short_id(),
            "Verified PQ signature on message"
        );

        self.dispatch_message(sender, signed_msg.message).await
    }

    /// Dispatch a verified message to the appropriate handler
    async fn dispatch_message(
        &self,
        sender: IrohIdentity,
        message: NetworkMessage,
    ) -> Result<(), MessageError> {
        match message {
            NetworkMessage::InterfaceEvent(msg) => self.handle_interface_event(sender, msg).await,
            NetworkMessage::SyncRequest(msg) => self.handle_sync_request(sender, msg).await,
            NetworkMessage::SyncResponse(msg) => self.handle_sync_response(sender, msg).await,
            NetworkMessage::EventAck(msg) => self.handle_event_ack(sender, msg).await,
        }
    }

    /// Handle an incoming interface event
    async fn handle_interface_event(
        &self,
        sender: IrohIdentity,
        msg: InterfaceEventMessage,
    ) -> Result<(), MessageError> {
        // Get the interface key
        let key = self
            .interface_keys
            .get(&msg.interface_id)
            .ok_or(MessageError::UnknownInterface(msg.interface_id))?;

        // Decrypt the event
        let encrypted = indras_crypto::EncryptedData {
            nonce: msg.nonce,
            ciphertext: msg.ciphertext,
        };
        let plaintext = key
            .decrypt(&encrypted)
            .map_err(|e| MessageError::Decryption(e.to_string()))?;

        // Deserialize the event
        let event: InterfaceEvent<IrohIdentity> = postcard::from_bytes(&plaintext)
            .map_err(|e| MessageError::Deserialization(e.to_string()))?;

        // Get the interface state
        let state = self
            .interfaces
            .get(&msg.interface_id)
            .ok_or(MessageError::UnknownInterface(msg.interface_id))?;

        // Append to interface (this updates pending tracking)
        {
            let mut interface = state.interface.write().await;
            interface
                .append(event.clone())
                .await
                .map_err(|e| MessageError::AppendFailed(e.to_string()))?;
        }

        // Broadcast locally
        let received = ReceivedEvent {
            interface_id: msg.interface_id,
            event,
        };
        let _ = state.event_tx.send(received);

        debug!(
            interface = %hex::encode(msg.interface_id.as_bytes()),
            event_id = ?msg.event_id,
            sender = %sender.short_id(),
            "Received and processed interface event"
        );

        Ok(())
    }

    /// Handle an incoming sync request
    async fn handle_sync_request(
        &self,
        sender: IrohIdentity,
        msg: InterfaceSyncRequest,
    ) -> Result<(), MessageError> {
        // Get the interface state
        let state = self
            .interfaces
            .get(&msg.interface_id)
            .ok_or(MessageError::UnknownInterface(msg.interface_id))?;

        // Create sync message to merge
        let sync_msg = indras_core::SyncMessage {
            interface_id: msg.interface_id,
            sync_data: msg.sync_data,
            heads: msg.heads,
            is_request: true,
        };

        // Merge incoming sync and generate immediate response
        let response_sync = {
            let mut interface = state.interface.write().await;
            interface
                .merge_sync(sync_msg)
                .await
                .map_err(|e| MessageError::SyncFailed(e.to_string()))?;

            // Ensure sender is tracked as a member so generate_sync produces correct diff
            let _ = interface.add_member(sender);

            // Generate sync response containing state the sender is missing
            interface.generate_sync(&sender)
        };

        debug!(
            interface = %hex::encode(msg.interface_id.as_bytes()),
            sender = %sender.short_id(),
            "Processed sync request"
        );

        // Send immediate sync response instead of waiting for next sync_task cycle
        if !response_sync.sync_data.is_empty() {
            let response = InterfaceSyncResponse {
                interface_id: msg.interface_id,
                sync_data: response_sync.sync_data,
                heads: response_sync.heads,
            };
            let network_msg = NetworkMessage::SyncResponse(response);
            if let Err(e) = self.sign_and_send(&sender, network_msg).await {
                debug!(
                    sender = %sender.short_id(),
                    error = %e,
                    "Failed to send immediate sync response (non-fatal)"
                );
            }
        }

        Ok(())
    }

    /// Sign a network message and send it to a peer.
    async fn sign_and_send(
        &self,
        peer: &IrohIdentity,
        message: NetworkMessage,
    ) -> Result<(), MessageError> {
        let message_bytes = message
            .to_bytes()
            .map_err(|e| MessageError::Serialization(e.to_string()))?;

        let signature = self.pq_identity.sign(&message_bytes);

        let signed_msg = SignedNetworkMessage {
            version: SIGNED_MESSAGE_VERSION,
            message,
            signature: signature.to_bytes().to_vec(),
            sender_verifying_key: self.pq_identity.verifying_key_bytes(),
        };

        let bytes = signed_msg
            .to_bytes()
            .map_err(|e| MessageError::Serialization(e.to_string()))?;

        self.transport
            .send(peer, bytes)
            .await
            .map_err(|e| MessageError::SyncFailed(e.to_string()))?;

        Ok(())
    }

    /// Handle an incoming sync response
    async fn handle_sync_response(
        &self,
        sender: IrohIdentity,
        msg: InterfaceSyncResponse,
    ) -> Result<(), MessageError> {
        // Get the interface state
        let state = self
            .interfaces
            .get(&msg.interface_id)
            .ok_or(MessageError::UnknownInterface(msg.interface_id))?;

        // Create sync message to merge
        let sync_msg = indras_core::SyncMessage {
            interface_id: msg.interface_id,
            sync_data: msg.sync_data,
            heads: msg.heads,
            is_request: false,
        };

        // Merge the incoming sync
        {
            let mut interface = state.interface.write().await;
            interface
                .merge_sync(sync_msg)
                .await
                .map_err(|e| MessageError::SyncFailed(e.to_string()))?;
        }

        debug!(
            interface = %hex::encode(msg.interface_id.as_bytes()),
            sender = %sender.short_id(),
            "Processed sync response"
        );

        Ok(())
    }

    /// Handle an event acknowledgment
    async fn handle_event_ack(
        &self,
        sender: IrohIdentity,
        msg: EventAckMessage,
    ) -> Result<(), MessageError> {
        // Get the interface state
        let state = self
            .interfaces
            .get(&msg.interface_id)
            .ok_or(MessageError::UnknownInterface(msg.interface_id))?;

        // Mark events as delivered
        {
            let mut interface = state.interface.write().await;
            interface.mark_delivered(&sender, msg.up_to);
        }

        // Also update storage
        self.storage
            .acknowledge_events(&sender, &msg.interface_id, msg.up_to)
            .map_err(|e| MessageError::StorageFailed(e.to_string()))?;

        debug!(
            interface = %hex::encode(msg.interface_id.as_bytes()),
            sender = %sender.short_id(),
            up_to = ?msg.up_to,
            "Processed event acknowledgment"
        );

        Ok(())
    }
}

/// Errors that can occur in message handling
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Unknown interface: {0:?}")]
    UnknownInterface(InterfaceId),

    #[error("Decryption error: {0}")]
    Decryption(String),

    #[error("Append failed: {0}")]
    AppendFailed(String),

    #[error("Sync failed: {0}")]
    SyncFailed(String),

    #[error("Storage error: {0}")]
    StorageFailed(String),

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Unsupported protocol version: got {0}, expected {1}")]
    UnsupportedVersion(u8, u8),

    #[error("Legacy (unsigned) messages are disabled; all messages must be signed")]
    LegacyModeDisabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_message_serialization() {
        let event_msg = InterfaceEventMessage {
            interface_id: InterfaceId::generate(),
            ciphertext: vec![1, 2, 3, 4],
            event_id: EventId::new(1, 1),
            nonce: [0u8; 12],
        };

        let msg = NetworkMessage::InterfaceEvent(event_msg);
        let bytes = msg.to_bytes().unwrap();
        let parsed = NetworkMessage::from_bytes(&bytes).unwrap();

        match parsed {
            NetworkMessage::InterfaceEvent(e) => {
                assert_eq!(e.ciphertext, vec![1, 2, 3, 4]);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_sync_request_serialization() {
        let request = InterfaceSyncRequest {
            interface_id: InterfaceId::generate(),
            heads: vec![[1u8; 32], [2u8; 32]],
            sync_data: vec![10, 20, 30],
        };

        let msg = NetworkMessage::SyncRequest(request);
        let bytes = msg.to_bytes().unwrap();
        let parsed = NetworkMessage::from_bytes(&bytes).unwrap();

        match parsed {
            NetworkMessage::SyncRequest(r) => {
                assert_eq!(r.heads.len(), 2);
                assert_eq!(r.sync_data, vec![10, 20, 30]);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_event_ack_serialization() {
        let ack = EventAckMessage {
            interface_id: InterfaceId::generate(),
            up_to: EventId::new(5, 10),
        };

        let msg = NetworkMessage::EventAck(ack);
        let bytes = msg.to_bytes().unwrap();
        let parsed = NetworkMessage::from_bytes(&bytes).unwrap();

        match parsed {
            NetworkMessage::EventAck(a) => {
                assert_eq!(a.up_to.sequence, 10);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
