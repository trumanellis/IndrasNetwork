//! Connection management for iroh endpoints
//!
//! Handles endpoint lifecycle, connection pooling, and stream management.

use dashmap::DashMap;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, PublicKey, SecretKey};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

use indras_core::identity::PeerIdentity;

use crate::identity::IrohIdentity;
use crate::protocol::ALPN_INDRAS;

/// Configuration for connection management
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Idle connection timeout in milliseconds
    pub idle_timeout_ms: u64,
    /// Whether to accept incoming connections
    pub accept_incoming: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            connect_timeout_ms: 10_000,
            idle_timeout_ms: 60_000,
            accept_incoming: true,
        }
    }
}

/// Errors that can occur in connection management
#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Failed to bind endpoint: {0}")]
    BindError(String),

    #[error("Failed to connect: {0}")]
    ConnectError(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Too many connections: {current}/{max}")]
    TooManyConnections { current: usize, max: usize },

    #[error("Timeout waiting for connection")]
    Timeout,

    #[error("No incoming connections available")]
    NoIncomingConnection,

    #[error("Iroh error: {0}")]
    IrohError(String),
}

/// Manages iroh endpoint and connections
///
/// Handles connection pooling, lifecycle management, and provides
/// a clean API for establishing and accepting connections.
pub struct ConnectionManager {
    /// The iroh endpoint
    endpoint: Endpoint,
    /// Our identity (derived from secret key)
    identity: IrohIdentity,
    /// Active connections indexed by peer identity
    connections: DashMap<IrohIdentity, Connection>,
    /// Configuration
    config: ConnectionConfig,
    /// Shutdown flag
    shutdown: RwLock<bool>,
}

impl ConnectionManager {
    /// Create a new connection manager
    ///
    /// Binds an iroh endpoint with the given secret key and configuration.
    pub async fn new(
        secret_key: SecretKey,
        config: ConnectionConfig,
    ) -> Result<Self, ConnectionError> {
        let builder = Endpoint::builder()
            .secret_key(secret_key.clone())
            .alpns(vec![ALPN_INDRAS.to_vec()]);

        let endpoint = builder
            .bind()
            .await
            .map_err(|e| ConnectionError::BindError(e.to_string()))?;

        let public_key = secret_key.public();
        let identity = IrohIdentity::new(public_key);

        info!(
            identity = %identity.short_id(),
            "Connection manager initialized"
        );

        Ok(Self {
            endpoint,
            identity,
            connections: DashMap::new(),
            config,
            shutdown: RwLock::new(false),
        })
    }

    /// Get our local identity
    pub fn local_identity(&self) -> IrohIdentity {
        self.identity
    }

    /// Get the endpoint's address for sharing with peers
    pub fn endpoint_addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// Get the raw endpoint (for advanced usage)
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Connect to a peer by their endpoint address
    #[instrument(skip(self, addr), fields(remote_peer = %IrohIdentity::new(addr.id).short_id()))]
    pub async fn connect(&self, addr: EndpointAddr) -> Result<Connection, ConnectionError> {
        let peer_id = IrohIdentity::new(addr.id);

        // Check if we already have a connection
        if let Some(conn) = self.connections.get(&peer_id) {
            if conn.close_reason().is_none() {
                debug!("Reusing existing connection");
                return Ok(conn.clone());
            }
            // Remove stale connection
            drop(conn);
            self.connections.remove(&peer_id);
        }

        // Check connection limit
        let current = self.connections.len();
        if current >= self.config.max_connections {
            warn!(
                current = current,
                max = self.config.max_connections,
                "Connection limit reached"
            );
            return Err(ConnectionError::TooManyConnections {
                current,
                max: self.config.max_connections,
            });
        }

        debug!("Establishing new connection");

        // Establish connection with timeout
        let connect_future = self.endpoint.connect(addr, ALPN_INDRAS);

        let conn = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.connect_timeout_ms),
            connect_future,
        )
        .await
        .map_err(|_| {
            warn!(
                timeout_ms = self.config.connect_timeout_ms,
                "Connection timeout"
            );
            ConnectionError::Timeout
        })?
        .map_err(|e| {
            warn!(error = %e, "Connection failed");
            ConnectionError::ConnectError(e.to_string())
        })?;

        info!("Connection established");

        // Store the connection
        self.connections.insert(peer_id, conn.clone());

        Ok(conn)
    }

    /// Connect to a peer by their public key
    ///
    /// This requires the peer to be discoverable via relays.
    pub async fn connect_by_key(
        &self,
        public_key: PublicKey,
    ) -> Result<Connection, ConnectionError> {
        let addr = EndpointAddr::new(public_key);
        self.connect(addr).await
    }

    /// Accept an incoming connection
    ///
    /// Returns the peer's identity and the connection.
    #[instrument(skip(self), name = "accept_connection")]
    pub async fn accept(&self) -> Result<(IrohIdentity, Connection), ConnectionError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(ConnectionError::NoIncomingConnection)?;

        let conn = incoming.await.map_err(|e| {
            warn!(error = %e, "Failed to accept connection");
            ConnectionError::ConnectError(e.to_string())
        })?;

        let peer_key = conn.remote_id();
        let peer_id = IrohIdentity::new(peer_key);

        info!(remote_peer = %peer_id.short_id(), "Accepted incoming connection");

        // Store the connection (replacing any existing one from this peer)
        self.connections.insert(peer_id, conn.clone());

        Ok((peer_id, conn))
    }

    /// Get an existing connection to a peer, if any
    pub fn get_connection(&self, peer: &IrohIdentity) -> Option<Connection> {
        self.connections.get(peer).map(|c| c.clone())
    }

    /// Check if we have an active connection to a peer
    pub fn is_connected(&self, peer: &IrohIdentity) -> bool {
        self.connections
            .get(peer)
            .map(|c| c.close_reason().is_none())
            .unwrap_or(false)
    }

    /// Get all connected peer identities
    pub fn connected_peers(&self) -> Vec<IrohIdentity> {
        self.connections
            .iter()
            .filter(|c| c.value().close_reason().is_none())
            .map(|c| *c.key())
            .collect()
    }

    /// Close a specific connection
    pub fn close_connection(&self, peer: &IrohIdentity) {
        if let Some((_, conn)) = self.connections.remove(peer) {
            conn.close(0u32.into(), b"closing");
            debug!(peer = %peer.short_id(), "Closed connection");
        }
    }

    /// Close all connections and shut down the endpoint
    pub async fn close(&self) {
        let mut shutdown = self.shutdown.write().await;
        if *shutdown {
            return;
        }
        *shutdown = true;

        info!("Shutting down connection manager");

        // Close all connections
        for entry in self.connections.iter() {
            entry.value().close(0u32.into(), b"shutdown");
        }
        self.connections.clear();

        // Close the endpoint
        self.endpoint.close().await;
    }

    /// Clean up closed/stale connections
    pub fn cleanup_stale(&self) {
        let stale: Vec<_> = self
            .connections
            .iter()
            .filter(|c| c.value().close_reason().is_some())
            .map(|c| *c.key())
            .collect();

        for peer in stale {
            self.connections.remove(&peer);
            debug!(peer = %peer.short_id(), "Removed stale connection");
        }
    }

    /// Get connection statistics
    pub fn stats(&self) -> ConnectionStats {
        let active = self
            .connections
            .iter()
            .filter(|c| c.value().close_reason().is_none())
            .count();
        let total = self.connections.len();

        ConnectionStats {
            active_connections: active,
            total_connections: total,
            max_connections: self.config.max_connections,
        }
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    /// Number of active (not closed) connections
    pub active_connections: usize,
    /// Total connections in the pool (including closed)
    pub total_connections: usize,
    /// Maximum allowed connections
    pub max_connections: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_manager_creation() {
        let secret = SecretKey::generate(&mut rand::rng());
        let config = ConnectionConfig::default();

        let manager = ConnectionManager::new(secret, config).await.unwrap();
        assert_eq!(manager.connected_peers().len(), 0);
    }

    #[tokio::test]
    async fn test_local_identity() {
        let secret = SecretKey::generate(&mut rand::rng());
        let expected_id = IrohIdentity::new(secret.public());

        let manager = ConnectionManager::new(secret, ConnectionConfig::default())
            .await
            .unwrap();

        assert_eq!(manager.local_identity(), expected_id);
    }

    #[test]
    fn test_connection_config_default() {
        let config = ConnectionConfig::default();

        assert_eq!(config.max_connections, 100);
        assert_eq!(config.connect_timeout_ms, 10_000);
        assert_eq!(config.idle_timeout_ms, 60_000);
        assert!(config.accept_incoming);
    }

    #[test]
    fn test_connection_config_custom() {
        let config = ConnectionConfig {
            max_connections: 50,
            connect_timeout_ms: 5000,
            idle_timeout_ms: 30_000,
            accept_incoming: false,
        };

        assert_eq!(config.max_connections, 50);
        assert_eq!(config.connect_timeout_ms, 5000);
        assert!(!config.accept_incoming);
    }

    #[tokio::test]
    async fn test_connection_stats_initial() {
        let secret = SecretKey::generate(&mut rand::rng());
        let config = ConnectionConfig::default();

        let manager = ConnectionManager::new(secret, config).await.unwrap();
        let stats = manager.stats();

        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.max_connections, 100);
    }

    #[tokio::test]
    async fn test_connection_manager_endpoint() {
        let secret = SecretKey::generate(&mut rand::rng());
        let config = ConnectionConfig::default();

        let manager = ConnectionManager::new(secret.clone(), config)
            .await
            .unwrap();

        // Endpoint should be accessible and have our public key
        let addr = manager.endpoint_addr();
        assert_eq!(addr.id, secret.public());
    }

    #[tokio::test]
    async fn test_connection_manager_endpoint_addr() {
        let secret = SecretKey::generate(&mut rand::rng());
        let config = ConnectionConfig::default();

        let manager = ConnectionManager::new(secret.clone(), config)
            .await
            .unwrap();
        let addr = manager.endpoint_addr();

        // Address should have correct node ID
        assert_eq!(addr.id, secret.public());
    }

    #[tokio::test]
    async fn test_connection_manager_close() {
        let secret = SecretKey::generate(&mut rand::rng());
        let config = ConnectionConfig::default();

        let manager = ConnectionManager::new(secret, config).await.unwrap();

        // Close should not panic
        manager.close().await;
    }

    #[tokio::test]
    async fn test_connected_peers_empty_initially() {
        let secret = SecretKey::generate(&mut rand::rng());
        let config = ConnectionConfig::default();

        let manager = ConnectionManager::new(secret, config).await.unwrap();
        let peers = manager.connected_peers();

        assert!(peers.is_empty());
    }
}
