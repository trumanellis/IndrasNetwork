//! Background sync task for periodic synchronization with peers
//!
//! Handles:
//! - Periodic sync with all interface members
//! - Delivery of pending events to peers (signed with ML-DSA-65)
//! - Sync state management
//!
//! ## Post-Quantum Signatures
//!
//! All outgoing messages are signed with ML-DSA-65 signatures.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use indras_core::transport::Transport;
use indras_core::{InterfaceId, NInterfaceTrait, PeerIdentity};
use indras_crypto::{InterfaceKey, PQIdentity};
use indras_storage::CompositeStorage;
use indras_transport::{IrohIdentity, IrohNetworkAdapter};

use crate::InterfaceState;
use crate::message_handler::{
    InterfaceEventMessage, InterfaceSyncRequest, NetworkMessage, SIGNED_MESSAGE_VERSION,
    SignedNetworkMessage,
};

/// Background sync task
pub struct SyncTask {
    /// Our transport identity
    local_identity: IrohIdentity,
    /// Our PQ identity for signing messages
    pq_identity: Arc<PQIdentity>,
    /// Transport adapter for sending messages
    transport: Arc<IrohNetworkAdapter>,
    /// Interface keys for encryption
    interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
    /// Loaded interfaces
    interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
    /// Storage (reserved for future use)
    #[allow(dead_code)]
    storage: Arc<CompositeStorage<IrohIdentity>>,
    /// Sync interval
    sync_interval: Duration,
    /// Shutdown signal
    shutdown_rx: broadcast::Receiver<()>,
}

impl SyncTask {
    /// Create a new sync task
    #[allow(clippy::too_many_arguments)] // Constructor with many dependencies
    pub fn new(
        local_identity: IrohIdentity,
        pq_identity: Arc<PQIdentity>,
        transport: Arc<IrohNetworkAdapter>,
        interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
        interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
        storage: Arc<CompositeStorage<IrohIdentity>>,
        sync_interval: Duration,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            local_identity,
            pq_identity,
            transport,
            interface_keys,
            interfaces,
            storage,
            sync_interval,
            shutdown_rx,
        }
    }

    /// Spawn the sync task as a background task
    #[allow(clippy::too_many_arguments)] // Constructor with many dependencies
    pub fn spawn(
        local_identity: IrohIdentity,
        pq_identity: Arc<PQIdentity>,
        transport: Arc<IrohNetworkAdapter>,
        interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
        interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
        storage: Arc<CompositeStorage<IrohIdentity>>,
        sync_interval: Duration,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> JoinHandle<()> {
        let task = Self::new(
            local_identity,
            pq_identity,
            transport,
            interface_keys,
            interfaces,
            storage,
            sync_interval,
            shutdown_rx,
        );

        tokio::spawn(async move {
            task.run().await;
        })
    }

    /// Run the sync task loop
    async fn run(mut self) {
        info!(
            interval_secs = self.sync_interval.as_secs(),
            "Sync task started"
        );

        let mut interval = tokio::time::interval(self.sync_interval);

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Sync task shutting down");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = self.sync_all_interfaces().await {
                        error!(error = %e, "Sync cycle failed");
                    }
                }
            }
        }
    }

    /// Sync all interfaces with their members
    async fn sync_all_interfaces(&self) -> Result<(), SyncError> {
        for entry in self.interfaces.iter() {
            let interface_id = *entry.key();
            let state = entry.value();

            if let Err(e) = self.sync_interface(interface_id, state).await {
                warn!(
                    interface = %hex::encode(interface_id.as_bytes()),
                    error = %e,
                    "Failed to sync interface"
                );
            }
        }

        Ok(())
    }

    /// Sync a single interface with its members
    async fn sync_interface(
        &self,
        interface_id: InterfaceId,
        state: &InterfaceState,
    ) -> Result<(), SyncError> {
        let interface = state.interface.read().await;
        let members = interface.members();

        // Get interface key for encryption
        let key = self.interface_keys.get(&interface_id);

        for member in members {
            // Skip ourselves
            if member == self.local_identity {
                continue;
            }

            // Skip if not connected
            if !self.transport.is_connected(&member) {
                debug!(
                    peer = %member.short_id(),
                    "Skipping sync - peer not connected"
                );
                continue;
            }

            // Send sync request
            if let Err(e) = self.send_sync_request(&interface, &member).await {
                debug!(
                    peer = %member.short_id(),
                    error = %e,
                    "Failed to send sync request"
                );
            }

            // Deliver pending events
            if let Some(ref key) = key
                && let Err(e) = self
                    .deliver_pending_events(&interface, &member, key.value())
                    .await
            {
                debug!(
                    peer = %member.short_id(),
                    error = %e,
                    "Failed to deliver pending events"
                );
            }
        }

        Ok(())
    }

    /// Sign and serialize a network message
    fn sign_message(&self, message: NetworkMessage) -> Result<Vec<u8>, SyncError> {
        let message_bytes = message
            .to_bytes()
            .map_err(|e| SyncError::Serialization(e.to_string()))?;

        let signature = self.pq_identity.sign(&message_bytes);

        let signed_msg = SignedNetworkMessage {
            version: SIGNED_MESSAGE_VERSION,
            message,
            signature: signature.to_bytes().to_vec(),
            sender_verifying_key: self.pq_identity.verifying_key_bytes(),
        };

        signed_msg
            .to_bytes()
            .map_err(|e| SyncError::Serialization(e.to_string()))
    }

    /// Send a sync request to a peer
    async fn send_sync_request(
        &self,
        interface: &tokio::sync::RwLockReadGuard<'_, indras_sync::NInterface<IrohIdentity>>,
        peer: &IrohIdentity,
    ) -> Result<(), SyncError> {
        // Generate sync message
        let sync_msg = interface.generate_sync(peer);

        // Create network message
        let request = InterfaceSyncRequest {
            interface_id: sync_msg.interface_id,
            heads: sync_msg.heads,
            sync_data: sync_msg.sync_data,
        };

        let msg = NetworkMessage::SyncRequest(request);

        // Sign and serialize
        let bytes = self.sign_message(msg)?;

        // Send to peer
        self.transport
            .send(peer, bytes)
            .await
            .map_err(|e| SyncError::Transport(e.to_string()))?;

        debug!(
            peer = %peer.short_id(),
            interface = %hex::encode(sync_msg.interface_id.as_bytes()),
            "Sent signed sync request"
        );

        Ok(())
    }

    /// Deliver pending events to a peer
    async fn deliver_pending_events(
        &self,
        interface: &tokio::sync::RwLockReadGuard<'_, indras_sync::NInterface<IrohIdentity>>,
        peer: &IrohIdentity,
        key: &InterfaceKey,
    ) -> Result<(), SyncError> {
        let pending = interface.pending_for(peer);

        if pending.is_empty() {
            return Ok(());
        }

        debug!(
            peer = %peer.short_id(),
            count = pending.len(),
            "Delivering pending events (signed)"
        );

        for event in pending {
            // Serialize the event
            let plaintext = postcard::to_allocvec(&event)
                .map_err(|e| SyncError::Serialization(e.to_string()))?;

            // Encrypt
            let encrypted = key
                .encrypt(&plaintext)
                .map_err(|e| SyncError::Encryption(e.to_string()))?;

            // Get event ID from the event (skip events without IDs like Presence, SyncMarker)
            let event_id = match event.event_id() {
                Some(id) => id,
                None => continue, // Skip events without IDs
            };

            // Create message
            let msg = InterfaceEventMessage::new(
                interface.id(),
                encrypted.ciphertext,
                event_id,
                encrypted.nonce,
            );

            let network_msg = NetworkMessage::InterfaceEvent(msg);

            // Sign and serialize
            let bytes = self.sign_message(network_msg)?;

            // Send
            self.transport
                .send(peer, bytes)
                .await
                .map_err(|e| SyncError::Transport(e.to_string()))?;
        }

        Ok(())
    }
}

/// Errors that can occur in sync operations
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Storage error: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic unit tests for sync task components
    // Full integration tests require network setup

    #[test]
    fn test_sync_error_display() {
        let err = SyncError::Transport("connection refused".to_string());
        assert!(err.to_string().contains("Transport error"));
    }
}
