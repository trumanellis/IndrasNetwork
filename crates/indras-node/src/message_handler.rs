//! Background message handler for processing incoming network messages
//!
//! Handles:
//! - Incoming interface events (decrypt, append, broadcast)
//! - Sync requests (generate sync response)
//! - Sync responses (apply incoming sync)
//! - Event acknowledgments (mark delivered)

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use indras_core::{EventId, InterfaceEvent, InterfaceId, NInterfaceTrait, PeerIdentity};
use indras_crypto::InterfaceKey;
use indras_storage::CompositeStorage;
use indras_transport::IrohIdentity;

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
    /// Shutdown signal
    shutdown_rx: broadcast::Receiver<()>,
}

impl MessageHandler {
    /// Create a new message handler
    pub fn new(
        local_identity: IrohIdentity,
        interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
        interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
        storage: Arc<CompositeStorage<IrohIdentity>>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            local_identity,
            interface_keys,
            interfaces,
            storage,
            shutdown_rx,
        }
    }

    /// Spawn the message handler as a background task
    pub fn spawn(
        local_identity: IrohIdentity,
        interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
        interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
        storage: Arc<CompositeStorage<IrohIdentity>>,
        shutdown_rx: broadcast::Receiver<()>,
        message_rx: tokio::sync::mpsc::Receiver<(IrohIdentity, Vec<u8>)>,
    ) -> JoinHandle<()> {
        let handler = Self::new(
            local_identity,
            interface_keys,
            interfaces,
            storage,
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
    async fn handle_message(&self, sender: IrohIdentity, data: Vec<u8>) -> Result<(), MessageError> {
        let message = NetworkMessage::from_bytes(&data)
            .map_err(|e| MessageError::Deserialization(e.to_string()))?;

        match message {
            NetworkMessage::InterfaceEvent(msg) => {
                self.handle_interface_event(sender, msg).await
            }
            NetworkMessage::SyncRequest(msg) => {
                self.handle_sync_request(sender, msg).await
            }
            NetworkMessage::SyncResponse(msg) => {
                self.handle_sync_response(sender, msg).await
            }
            NetworkMessage::EventAck(msg) => {
                self.handle_event_ack(sender, msg).await
            }
        }
    }

    /// Handle an incoming interface event
    async fn handle_interface_event(
        &self,
        sender: IrohIdentity,
        msg: InterfaceEventMessage,
    ) -> Result<(), MessageError> {
        // Get the interface key
        let key = self.interface_keys.get(&msg.interface_id)
            .ok_or_else(|| MessageError::UnknownInterface(msg.interface_id))?;

        // Decrypt the event
        let encrypted = indras_crypto::EncryptedData {
            nonce: msg.nonce,
            ciphertext: msg.ciphertext,
        };
        let plaintext = key.decrypt(&encrypted)
            .map_err(|e| MessageError::Decryption(e.to_string()))?;

        // Deserialize the event
        let event: InterfaceEvent<IrohIdentity> = postcard::from_bytes(&plaintext)
            .map_err(|e| MessageError::Deserialization(e.to_string()))?;

        // Get the interface state
        let state = self.interfaces.get(&msg.interface_id)
            .ok_or_else(|| MessageError::UnknownInterface(msg.interface_id))?;

        // Append to interface (this updates pending tracking)
        {
            let mut interface = state.interface.write().await;
            interface.append(event.clone()).await
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
        let state = self.interfaces.get(&msg.interface_id)
            .ok_or_else(|| MessageError::UnknownInterface(msg.interface_id))?;

        // Create sync message to merge
        let sync_msg = indras_core::SyncMessage {
            interface_id: msg.interface_id,
            sync_data: msg.sync_data,
            heads: msg.heads,
            is_request: true,
        };

        // Merge the incoming sync
        {
            let mut interface = state.interface.write().await;
            interface.merge_sync(sync_msg).await
                .map_err(|e| MessageError::SyncFailed(e.to_string()))?;
        }

        debug!(
            interface = %hex::encode(msg.interface_id.as_bytes()),
            sender = %sender.short_id(),
            "Processed sync request"
        );

        // Note: Response is generated by sync_task on next sync cycle
        Ok(())
    }

    /// Handle an incoming sync response
    async fn handle_sync_response(
        &self,
        sender: IrohIdentity,
        msg: InterfaceSyncResponse,
    ) -> Result<(), MessageError> {
        // Get the interface state
        let state = self.interfaces.get(&msg.interface_id)
            .ok_or_else(|| MessageError::UnknownInterface(msg.interface_id))?;

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
            interface.merge_sync(sync_msg).await
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
        let state = self.interfaces.get(&msg.interface_id)
            .ok_or_else(|| MessageError::UnknownInterface(msg.interface_id))?;

        // Mark events as delivered
        {
            let mut interface = state.interface.write().await;
            interface.mark_delivered(&sender, msg.up_to);
        }

        // Also update storage
        self.storage.acknowledge_events(&sender, &msg.interface_id, msg.up_to)
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
