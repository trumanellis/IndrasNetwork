//! Iroh network adapter implementing Transport and NetworkTopology traits
//!
//! This module provides [`IrohNetworkAdapter`], which wraps the low-level
//! [`ConnectionManager`] and [`DiscoveryService`] to implement the high-level
//! [`Transport`] and [`NetworkTopology`] traits from indras-core.
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_transport::{IrohNetworkAdapter, AdapterConfig};
//! use indras_core::Transport;
//! use iroh::SecretKey;
//!
//! let secret = SecretKey::generate(&mut rand::thread_rng());
//! let adapter = IrohNetworkAdapter::new(secret, AdapterConfig::default()).await?;
//!
//! // Start discovery
//! adapter.start().await?;
//!
//! // Send a message to a peer
//! let data = b"Hello, peer!".to_vec();
//! adapter.send(&peer_id, data).await?;
//!
//! // Receive messages
//! let (sender, data) = adapter.recv().await?;
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use iroh::{EndpointAddr, PublicKey, SecretKey};
use iroh::endpoint::Connection;
use iroh_gossip::Gossip;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, instrument, warn};

use indras_core::error::TransportError;
use indras_core::identity::PeerIdentity;
use indras_core::traits::NetworkTopology;
use indras_core::transport::Transport;

use crate::connection::{ConnectionConfig, ConnectionError, ConnectionManager};
use crate::discovery::{DiscoveryConfig, DiscoveryError, DiscoveryService, PeerEvent, PeerInfo};
use crate::identity::IrohIdentity;

/// Configuration for the network adapter
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    /// Connection manager configuration
    pub connection: ConnectionConfig,
    /// Discovery service configuration
    pub discovery: DiscoveryConfig,
    /// Size of the incoming message buffer
    pub message_buffer_size: usize,
    /// Whether to auto-connect to discovered peers
    pub auto_connect: bool,
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            discovery: DiscoveryConfig::default(),
            message_buffer_size: 1024,
            auto_connect: false,
            max_reconnect_attempts: 3,
        }
    }
}

/// Incoming message from a peer
#[derive(Debug, Clone)]
struct IncomingMessage {
    sender: IrohIdentity,
    data: Vec<u8>,
}

/// Network adapter that bridges iroh networking to the Transport trait
///
/// This adapter provides a unified interface for sending and receiving messages
/// while handling connection management and peer discovery automatically.
pub struct IrohNetworkAdapter {
    /// Connection manager for QUIC connections
    connection_manager: Arc<ConnectionManager>,
    /// Discovery service for peer discovery
    discovery_service: Arc<DiscoveryService>,
    /// Gossip handle for topic-based messaging
    gossip: Arc<Gossip>,
    /// Our local identity
    local_identity: IrohIdentity,
    /// Configuration
    config: AdapterConfig,
    /// Peer addresses (for connecting without discovery)
    peer_addresses: DashMap<IrohIdentity, EndpointAddr>,
    /// Incoming message channel
    message_tx: mpsc::Sender<IncomingMessage>,
    /// Incoming message receiver (wrapped in RwLock for async access)
    message_rx: Arc<RwLock<mpsc::Receiver<IncomingMessage>>>,
    /// Shutdown signal
    shutdown: broadcast::Sender<()>,
    /// Running state
    running: RwLock<bool>,
}

impl IrohNetworkAdapter {
    /// Create a new network adapter
    ///
    /// This initializes the iroh endpoint, connection manager, gossip, and
    /// discovery service.
    pub async fn new(
        secret_key: SecretKey,
        config: AdapterConfig,
    ) -> Result<Self, AdapterError> {
        // Create connection manager
        let connection_manager = ConnectionManager::new(secret_key.clone(), config.connection.clone())
            .await
            .map_err(|e| AdapterError::ConnectionManager(e.to_string()))?;

        let local_identity = connection_manager.local_identity();
        let endpoint = connection_manager.endpoint().clone();

        // Create gossip service using the builder pattern
        let gossip = Gossip::builder().spawn(endpoint);

        // Create discovery service
        let discovery_service = DiscoveryService::new(
            gossip.clone(),
            local_identity,
            config.discovery.clone(),
        );

        // Create message channel
        let (message_tx, message_rx) = mpsc::channel(config.message_buffer_size);
        let (shutdown, _) = broadcast::channel(1);

        info!(
            identity = %local_identity.short_id(),
            "IrohNetworkAdapter created"
        );

        Ok(Self {
            connection_manager: Arc::new(connection_manager),
            discovery_service: Arc::new(discovery_service),
            gossip: Arc::new(gossip),
            local_identity,
            config,
            peer_addresses: DashMap::new(),
            message_tx,
            message_rx: Arc::new(RwLock::new(message_rx)),
            shutdown,
            running: RwLock::new(false),
        })
    }

    /// Start the adapter
    ///
    /// Starts the discovery service and begins accepting connections.
    #[instrument(skip(self, bootstrap_peers))]
    pub async fn start(&self, bootstrap_peers: Vec<PublicKey>) -> Result<(), AdapterError> {
        let mut running = self.running.write().await;
        if *running {
            debug!("Adapter already running");
            return Ok(());
        }

        info!("Starting IrohNetworkAdapter");

        // Start discovery
        self.discovery_service
            .start(bootstrap_peers)
            .await
            .map_err(|e| AdapterError::Discovery(e.to_string()))?;

        *running = true;

        // Start background tasks
        self.spawn_accept_loop();
        self.spawn_discovery_handler();

        Ok(())
    }

    /// Stop the adapter
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        if !*running {
            return;
        }

        info!("Stopping IrohNetworkAdapter");

        // Signal shutdown
        let _ = self.shutdown.send(());

        // Stop discovery
        self.discovery_service.stop().await;

        // Close all connections
        self.connection_manager.close().await;

        *running = false;
    }

    /// Get our local identity
    pub fn local_identity(&self) -> IrohIdentity {
        self.local_identity
    }

    /// Get the endpoint address for sharing with peers
    pub fn endpoint_addr(&self) -> EndpointAddr {
        self.connection_manager.endpoint_addr()
    }

    /// Add a known peer address
    ///
    /// This allows connecting to peers without discovery.
    pub fn add_peer_address(&self, peer: IrohIdentity, addr: EndpointAddr) {
        self.peer_addresses.insert(peer, addr);
    }

    /// Get known peer info from discovery
    pub fn known_peers(&self) -> Vec<PeerInfo> {
        self.discovery_service.known_peers()
    }

    /// Subscribe to peer events
    pub fn subscribe_peer_events(&self) -> broadcast::Receiver<PeerEvent> {
        self.discovery_service.subscribe()
    }

    /// Get the connection manager
    pub fn connection_manager(&self) -> &ConnectionManager {
        &self.connection_manager
    }

    /// Get the discovery service
    pub fn discovery_service(&self) -> &DiscoveryService {
        &self.discovery_service
    }

    /// Get the gossip handle
    pub fn gossip(&self) -> &Gossip {
        &self.gossip
    }

    /// Spawn the connection accept loop
    fn spawn_accept_loop(&self) {
        let connection_manager = self.connection_manager.clone();
        let message_tx = self.message_tx.clone();
        let mut shutdown_rx = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Accept loop shutting down");
                        break;
                    }
                    result = connection_manager.accept() => {
                        match result {
                            Ok((peer_id, conn)) => {
                                debug!(peer = %peer_id.short_id(), "Accepted connection");
                                Self::spawn_connection_handler(
                                    peer_id,
                                    conn,
                                    message_tx.clone(),
                                );
                            }
                            Err(ConnectionError::NoIncomingConnection) => {
                                // Endpoint closed, exit loop
                                break;
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to accept connection");
                            }
                        }
                    }
                }
            }
        });
    }

    /// Spawn a handler for an established connection
    fn spawn_connection_handler(
        peer_id: IrohIdentity,
        conn: Connection,
        message_tx: mpsc::Sender<IncomingMessage>,
    ) {
        tokio::spawn(async move {
            loop {
                match conn.accept_uni().await {
                    Ok(mut recv) => {
                        // Read the message
                        match recv.read_to_end(1024 * 1024).await {
                            Ok(data) => {
                                if let Err(e) = message_tx.send(IncomingMessage {
                                    sender: peer_id,
                                    data,
                                }).await {
                                    error!(error = %e, "Failed to queue incoming message");
                                    break;
                                }
                            }
                            Err(e) => {
                                debug!(error = %e, "Stream read error");
                            }
                        }
                    }
                    Err(e) => {
                        if conn.close_reason().is_some() {
                            debug!(peer = %peer_id.short_id(), "Connection closed");
                        } else {
                            debug!(error = %e, "Failed to accept stream");
                        }
                        break;
                    }
                }
            }
        });
    }

    /// Spawn the discovery event handler
    fn spawn_discovery_handler(&self) {
        if !self.config.auto_connect {
            return;
        }

        let discovery_service = self.discovery_service.clone();
        let connection_manager = self.connection_manager.clone();
        let peer_addresses = self.peer_addresses.clone();
        let mut shutdown_rx = self.shutdown.subscribe();

        tokio::spawn(async move {
            let mut events = discovery_service.subscribe();

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Discovery handler shutting down");
                        break;
                    }
                    Ok(event) = events.recv() => {
                        match event {
                            PeerEvent::Discovered(info) => {
                                // Auto-connect to discovered peers
                                let peer_id = info.identity;
                                if !connection_manager.is_connected(&peer_id) {
                                    // Try to get address from our cache
                                    if let Some(addr) = peer_addresses.get(&peer_id) {
                                        if let Err(e) = connection_manager.connect(addr.clone()).await {
                                            debug!(
                                                peer = %peer_id.short_id(),
                                                error = %e,
                                                "Failed to auto-connect"
                                            );
                                        }
                                    }
                                }
                            }
                            PeerEvent::Lost(peer_id) => {
                                // Clean up connection
                                connection_manager.close_connection(&peer_id);
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }

    /// Send raw bytes to a peer over QUIC
    async fn send_bytes(&self, peer: &IrohIdentity, data: Vec<u8>) -> Result<(), TransportError> {
        // Get or establish connection
        let conn = if let Some(conn) = self.connection_manager.get_connection(peer) {
            conn
        } else {
            // Try to connect using known address
            if let Some(addr) = self.peer_addresses.get(peer) {
                self.connection_manager
                    .connect(addr.clone())
                    .await
                    .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?
            } else {
                // Try connecting by key alone (relies on relay discovery)
                self.connection_manager
                    .connect_by_key(*peer.public_key())
                    .await
                    .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?
            }
        };

        // Open a unidirectional stream and send data
        let mut send = conn
            .open_uni()
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;

        send.write_all(&data)
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;

        send.finish()
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl Transport<IrohIdentity> for IrohNetworkAdapter {
    async fn send(&self, peer: &IrohIdentity, data: Vec<u8>) -> Result<(), TransportError> {
        self.send_bytes(peer, data).await
    }

    async fn recv(&self) -> Result<(IrohIdentity, Vec<u8>), TransportError> {
        let mut rx = self.message_rx.write().await;
        let msg = rx
            .recv()
            .await
            .ok_or_else(|| TransportError::ReceiveFailed("channel closed".into()))?;
        Ok((msg.sender, msg.data))
    }

    fn is_connected(&self, peer: &IrohIdentity) -> bool {
        self.connection_manager.is_connected(peer)
    }

    fn connected_peers(&self) -> Vec<IrohIdentity> {
        self.connection_manager.connected_peers()
    }

    async fn ensure_connected(&self, peer: &IrohIdentity) -> Result<(), TransportError> {
        if self.is_connected(peer) {
            return Ok(());
        }

        // Try to connect
        if let Some(addr) = self.peer_addresses.get(peer) {
            self.connection_manager
                .connect(addr.clone())
                .await
                .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
            Ok(())
        } else {
            // Try by key
            self.connection_manager
                .connect_by_key(*peer.public_key())
                .await
                .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
            Ok(())
        }
    }

    async fn try_recv(&self) -> Result<Option<(IrohIdentity, Vec<u8>)>, TransportError> {
        let mut rx = self.message_rx.write().await;
        match rx.try_recv() {
            Ok(msg) => Ok(Some((msg.sender, msg.data))),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                Err(TransportError::ReceiveFailed("channel disconnected".into()))
            }
        }
    }

    async fn disconnect(&self, peer: &IrohIdentity) -> Result<(), TransportError> {
        self.connection_manager.close_connection(peer);
        Ok(())
    }
}

impl NetworkTopology<IrohIdentity> for IrohNetworkAdapter {
    fn peers(&self) -> Vec<IrohIdentity> {
        // Return all known peers (from discovery + connected)
        let mut peers: Vec<_> = self.discovery_service
            .known_peers()
            .into_iter()
            .map(|p| p.identity)
            .collect();

        // Add connected peers that might not be in discovery
        for peer in self.connection_manager.connected_peers() {
            if !peers.contains(&peer) {
                peers.push(peer);
            }
        }

        peers
    }

    fn neighbors(&self, peer: &IrohIdentity) -> Vec<IrohIdentity> {
        // In iroh, "neighbors" are peers that this peer is directly connected to
        // We get this from presence info if available
        if let Some(info) = self.discovery_service.get_peer(peer) {
            info.presence.neighbors
        } else {
            Vec::new()
        }
    }

    fn are_connected(&self, a: &IrohIdentity, b: &IrohIdentity) -> bool {
        // We can only definitively know if WE are connected to one of them
        if *a == self.local_identity {
            self.connection_manager.is_connected(b)
        } else if *b == self.local_identity {
            self.connection_manager.is_connected(a)
        } else {
            // Check if a's neighbors include b (from presence info)
            self.neighbors(a).contains(b)
        }
    }

    fn is_online(&self, peer: &IrohIdentity) -> bool {
        // A peer is online if we've seen them recently via discovery
        // or if we're actively connected to them
        self.connection_manager.is_connected(peer)
            || self.discovery_service.get_peer(peer).is_some()
    }
}

/// Errors that can occur in the network adapter
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("Connection manager error: {0}")]
    ConnectionManager(String),

    #[error("Discovery error: {0}")]
    Discovery(String),

    #[error("Gossip error: {0}")]
    Gossip(String),

    #[error("Not running")]
    NotRunning,
}

impl From<ConnectionError> for AdapterError {
    fn from(e: ConnectionError) -> Self {
        Self::ConnectionManager(e.to_string())
    }
}

impl From<DiscoveryError> for AdapterError {
    fn from(e: DiscoveryError) -> Self {
        Self::Discovery(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_adapter_creation() {
        let secret = SecretKey::generate(&mut rand::thread_rng());
        let config = AdapterConfig::default();

        let adapter = IrohNetworkAdapter::new(secret.clone(), config).await.unwrap();

        // Identity should be derived from secret key
        let expected_id = IrohIdentity::new(secret.public());
        assert_eq!(adapter.local_identity(), expected_id);

        // Should have no connected peers initially
        assert!(adapter.connected_peers().is_empty());
    }

    #[tokio::test]
    async fn test_peer_address_management() {
        let secret = SecretKey::generate(&mut rand::thread_rng());
        let adapter = IrohNetworkAdapter::new(secret.clone(), AdapterConfig::default())
            .await
            .unwrap();

        let peer_secret = SecretKey::generate(&mut rand::thread_rng());
        let peer_id = IrohIdentity::new(peer_secret.public());
        let peer_addr = EndpointAddr::new(peer_secret.public());

        // Add peer address
        adapter.add_peer_address(peer_id, peer_addr.clone());

        // Should be retrievable
        assert!(adapter.peer_addresses.contains_key(&peer_id));
    }

    #[tokio::test]
    async fn test_topology_implementation() {
        let secret = SecretKey::generate(&mut rand::thread_rng());
        let adapter = IrohNetworkAdapter::new(secret.clone(), AdapterConfig::default())
            .await
            .unwrap();

        // Initially no peers
        assert!(adapter.peers().is_empty());

        // Local identity should be online (we're always online to ourselves)
        // But actually we're not in the peer list
        assert!(!adapter.is_online(&adapter.local_identity()));
    }
}
