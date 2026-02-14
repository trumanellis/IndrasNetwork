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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// Maximum number of events to batch in a single delivery cycle per peer.
const EVENT_BATCH_SIZE: usize = 50;

/// Duration after which a peer is considered potentially offline.
const PEER_TIMEOUT_WARN: Duration = Duration::from_secs(300); // 5 minutes

/// Duration after which a peer is considered offline (logged but not removed).
const PEER_TIMEOUT_OFFLINE: Duration = Duration::from_secs(900); // 15 minutes

/// Tracks delivery retry state for a peer.
struct PeerDeliveryState {
    consecutive_failures: u32,
    last_attempt: Instant,
    last_success: Option<Instant>,
}

impl PeerDeliveryState {
    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            last_attempt: Instant::now(),
            last_success: None,
        }
    }

    /// Exponential backoff: 2^failures seconds, capped at 64s (2^6).
    fn backoff_duration(&self) -> Duration {
        Duration::from_secs(2u64.pow(self.consecutive_failures.min(6))) // max 64s
    }

    /// Whether enough time has elapsed since last attempt to retry.
    /// Always returns true if no failures have occurred yet.
    fn should_retry(&self) -> bool {
        // Always allow first attempt (no failures yet)
        if self.consecutive_failures == 0 {
            return true;
        }
        self.last_attempt.elapsed() >= self.backoff_duration()
    }

    /// Record a successful delivery, resetting the failure counter.
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.last_success = Some(Instant::now());
        self.last_attempt = Instant::now();
    }

    /// Record a failed delivery attempt, incrementing the backoff.
    fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_attempt = Instant::now();
    }

    /// True if no successful delivery in PEER_TIMEOUT_WARN (5 minutes).
    fn is_potentially_offline(&self) -> bool {
        match self.last_success {
            Some(t) => t.elapsed() > PEER_TIMEOUT_WARN,
            None => self.last_attempt.elapsed() > PEER_TIMEOUT_WARN,
        }
    }

    /// True if no successful delivery in PEER_TIMEOUT_OFFLINE (15 minutes).
    fn is_offline(&self) -> bool {
        match self.last_success {
            Some(t) => t.elapsed() > PEER_TIMEOUT_OFFLINE,
            None => self.last_attempt.elapsed() > PEER_TIMEOUT_OFFLINE,
        }
    }
}

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
    /// Per-peer delivery retry state
    delivery_states: HashMap<IrohIdentity, PeerDeliveryState>,
    /// Sync cycle counter (for periodic maintenance)
    cycle_count: u64,
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
            delivery_states: HashMap::new(),
            cycle_count: 0,
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
                    self.cycle_count += 1;
                    if let Err(e) = self.sync_all_interfaces().await {
                        error!(error = %e, "Sync cycle failed");
                    }
                }
            }
        }
    }

    /// Sync all interfaces with their members
    async fn sync_all_interfaces(&mut self) -> Result<(), SyncError> {
        // Clone the Arc so DashMap borrows don't go through `self`,
        // allowing `&mut self` in sync_interface_inner.
        let interfaces = Arc::clone(&self.interfaces);

        let interface_ids: Vec<InterfaceId> = interfaces.iter()
            .map(|entry| *entry.key())
            .collect();

        for interface_id in interface_ids {
            let state = match interfaces.get(&interface_id) {
                Some(s) => s,
                None => continue,
            };

            let interface = state.interface.read().await;

            if let Err(e) = self.sync_interface_inner(interface_id, &interface).await {
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
    async fn sync_interface_inner(
        &mut self,
        interface_id: InterfaceId,
        interface: &tokio::sync::RwLockReadGuard<'_, indras_sync::NInterface<IrohIdentity>>,
    ) -> Result<(), SyncError> {
        let members = interface.members();
        let key = self.interface_keys.get(&interface_id);

        for member in members {
            if member == self.local_identity {
                continue;
            }

            let delivery_state = self.delivery_states
                .entry(member.clone())
                .or_insert_with(PeerDeliveryState::new);

            if delivery_state.is_offline() {
                debug!(
                    peer = %member.short_id(),
                    "Peer appears offline (no success in 15m), reducing sync frequency"
                );
                if self.cycle_count % 4 != 0 {
                    continue;
                }
            } else if delivery_state.is_potentially_offline() {
                debug!(
                    peer = %member.short_id(),
                    "Peer may be offline (no success in 5m)"
                );
            }

            if !delivery_state.should_retry() {
                debug!(
                    peer = %member.short_id(),
                    failures = delivery_state.consecutive_failures,
                    "Skipping peer due to backoff"
                );
                continue;
            }

            if !self.transport.is_connected(&member) {
                debug!(
                    peer = %member.short_id(),
                    "Skipping sync - peer not connected"
                );
                delivery_state.record_failure();
                continue;
            }

            let mut sync_ok = true;
            if let Err(e) = self.send_sync_request(interface, &member).await {
                debug!(
                    peer = %member.short_id(),
                    error = %e,
                    "Failed to send sync request"
                );
                sync_ok = false;
            }

            if let Some(ref key) = key {
                match self.deliver_pending_events_batched(interface, &member, key.value()).await {
                    Ok(()) => {}
                    Err(e) => {
                        debug!(
                            peer = %member.short_id(),
                            error = %e,
                            "Failed to deliver pending events"
                        );
                        sync_ok = false;
                    }
                }
            }

            let delivery_state = self.delivery_states.get_mut(&member).unwrap();
            if sync_ok {
                delivery_state.record_success();
            } else {
                delivery_state.record_failure();
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
            state_vector: sync_msg.state_vector,
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

    /// Deliver pending events to a peer in batches of EVENT_BATCH_SIZE (50).
    ///
    /// Each event is serialized, encrypted with the interface key, and wrapped
    /// in an ML-DSA-65 signed network message before sending.
    async fn deliver_pending_events_batched(
        &self,
        interface: &tokio::sync::RwLockReadGuard<'_, indras_sync::NInterface<IrohIdentity>>,
        peer: &IrohIdentity,
        key: &InterfaceKey,
    ) -> Result<(), SyncError> {
        let pending = interface.pending_for(peer);

        if pending.is_empty() {
            return Ok(());
        }

        let total = pending.len();
        let batch_count = (total + EVENT_BATCH_SIZE - 1) / EVENT_BATCH_SIZE;

        debug!(
            peer = %peer.short_id(),
            total,
            batches = batch_count,
            "Delivering pending events in batches (signed)"
        );

        for (batch_idx, batch) in pending.chunks(EVENT_BATCH_SIZE).enumerate() {
            for event in batch {
                let plaintext = postcard::to_allocvec(&event)
                    .map_err(|e| SyncError::Serialization(e.to_string()))?;

                let encrypted = key
                    .encrypt(&plaintext)
                    .map_err(|e| SyncError::Encryption(e.to_string()))?;

                let event_id = match event.event_id() {
                    Some(id) => id,
                    None => continue,
                };

                let msg = InterfaceEventMessage::new(
                    interface.id(),
                    encrypted.ciphertext,
                    event_id,
                    encrypted.nonce,
                );

                let network_msg = NetworkMessage::InterfaceEvent(msg);
                let bytes = self.sign_message(network_msg)?;

                self.transport
                    .send(peer, bytes)
                    .await
                    .map_err(|e| SyncError::Transport(e.to_string()))?;
            }

            if batch_count > 1 {
                debug!(
                    peer = %peer.short_id(),
                    batch = batch_idx + 1,
                    of = batch_count,
                    "Batch delivered"
                );
            }
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

    #[test]
    fn test_peer_delivery_state_new() {
        let state = PeerDeliveryState::new();
        assert_eq!(state.consecutive_failures, 0);
        assert!(state.last_success.is_none());
        assert!(state.should_retry());
    }

    #[test]
    fn test_peer_delivery_state_backoff() {
        let mut state = PeerDeliveryState::new();
        // Initial backoff: 2^0 = 1 second
        assert_eq!(state.backoff_duration(), Duration::from_secs(1));

        state.record_failure();
        assert_eq!(state.consecutive_failures, 1);
        // After 1 failure: 2^1 = 2 seconds
        assert_eq!(state.backoff_duration(), Duration::from_secs(2));

        state.record_failure();
        assert_eq!(state.consecutive_failures, 2);
        // After 2 failures: 2^2 = 4 seconds
        assert_eq!(state.backoff_duration(), Duration::from_secs(4));
    }

    #[test]
    fn test_peer_delivery_state_max_backoff() {
        let mut state = PeerDeliveryState::new();
        // Max backoff caps at 2^6 = 64 seconds
        for _ in 0..10 {
            state.record_failure();
        }
        assert_eq!(state.backoff_duration(), Duration::from_secs(64));
    }

    #[test]
    fn test_peer_delivery_state_success_resets() {
        let mut state = PeerDeliveryState::new();
        state.record_failure();
        state.record_failure();
        state.record_failure();
        assert_eq!(state.consecutive_failures, 3);

        state.record_success();
        assert_eq!(state.consecutive_failures, 0);
        assert!(state.last_success.is_some());
    }
}
