//! Transport abstraction for message passing
//!
//! The [`Transport`] trait provides a unified interface for sending and receiving
//! messages between peers. This allows sync and routing logic to work with both
//! real iroh networking and mock channels for testing.
//!
//! ## Implementations
//!
//! - [`MockTransport`]: In-memory transport for testing (in this module)
//! - `IrohNetworkAdapter`: Real iroh transport (in indras-transport crate)

use async_trait::async_trait;

use crate::error::TransportError;
use crate::identity::PeerIdentity;

/// Transport trait for message passing between peers
///
/// This trait abstracts the underlying transport mechanism, allowing
/// the same sync and routing code to work with both real networking
/// and mock implementations for testing.
///
/// # Type Parameters
///
/// * `I` - The peer identity type (e.g., `SimulationIdentity` or `IrohIdentity`)
///
/// # Example
///
/// ```rust,ignore
/// use indras_core::{Transport, SimulationIdentity};
///
/// async fn send_message<T: Transport<SimulationIdentity>>(
///     transport: &T,
///     peer: &SimulationIdentity,
///     data: Vec<u8>,
/// ) -> Result<(), TransportError> {
///     transport.send(peer, data).await
/// }
/// ```
#[async_trait]
pub trait Transport<I: PeerIdentity>: Send + Sync {
    /// Send data to a specific peer
    ///
    /// # Arguments
    ///
    /// * `peer` - The target peer to send data to
    /// * `data` - The data payload to send
    ///
    /// # Errors
    ///
    /// Returns an error if the peer is not connected or if sending fails.
    async fn send(&self, peer: &I, data: Vec<u8>) -> Result<(), TransportError>;

    /// Receive data from any connected peer
    ///
    /// Blocks until data is available or an error occurs.
    ///
    /// # Returns
    ///
    /// A tuple of (sender identity, data payload)
    async fn recv(&self) -> Result<(I, Vec<u8>), TransportError>;

    /// Check if we're currently connected to a peer
    fn is_connected(&self, peer: &I) -> bool;

    /// Get all currently connected peers
    fn connected_peers(&self) -> Vec<I>;

    /// Ensure a connection to a peer exists
    ///
    /// If not connected, attempts to establish a connection.
    /// If already connected, returns immediately.
    ///
    /// # Arguments
    ///
    /// * `peer` - The peer to connect to
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established.
    async fn ensure_connected(&self, peer: &I) -> Result<(), TransportError>;

    /// Try to receive data without blocking
    ///
    /// Returns `Ok(None)` if no data is immediately available.
    /// Default implementation just calls `recv()` with a timeout of 0.
    async fn try_recv(&self) -> Result<Option<(I, Vec<u8>)>, TransportError> {
        // Default implementation: subclasses should override for better performance
        match tokio::time::timeout(std::time::Duration::ZERO, self.recv()).await {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    /// Disconnect from a peer
    ///
    /// Default implementation does nothing (connection cleanup may be optional).
    async fn disconnect(&self, _peer: &I) -> Result<(), TransportError> {
        Ok(())
    }

    /// Get the number of connected peers
    fn connection_count(&self) -> usize {
        self.connected_peers().len()
    }
}

/// Extension trait for transport with broadcast capabilities
#[async_trait]
pub trait BroadcastTransport<I: PeerIdentity>: Transport<I> {
    /// Broadcast data to all connected peers
    ///
    /// Returns a list of peers that successfully received the data.
    async fn broadcast(&self, data: Vec<u8>) -> Result<Vec<I>, TransportError> {
        let peers = self.connected_peers();
        let mut successful = Vec::with_capacity(peers.len());

        for peer in peers {
            if self.send(&peer, data.clone()).await.is_ok() {
                successful.push(peer);
            }
        }

        Ok(successful)
    }

    /// Broadcast data to a subset of connected peers
    async fn broadcast_to(&self, peers: &[I], data: Vec<u8>) -> Result<Vec<I>, TransportError> {
        let mut successful = Vec::with_capacity(peers.len());

        for peer in peers {
            if self.is_connected(peer) && self.send(peer, data.clone()).await.is_ok() {
                successful.push(peer.clone());
            }
        }

        Ok(successful)
    }
}

/// Blanket implementation of BroadcastTransport for all Transport implementations
impl<I: PeerIdentity, T: Transport<I>> BroadcastTransport<I> for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SimulationIdentity;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::{mpsc, RwLock};

    /// Simple mock transport for testing
    struct TestTransport {
        local: SimulationIdentity,
        connections: Arc<RwLock<HashMap<SimulationIdentity, mpsc::Sender<Vec<u8>>>>>,
        inbox: Arc<RwLock<mpsc::Receiver<(SimulationIdentity, Vec<u8>)>>>,
    }

    #[async_trait]
    impl Transport<SimulationIdentity> for TestTransport {
        async fn send(&self, peer: &SimulationIdentity, data: Vec<u8>) -> Result<(), TransportError> {
            let connections = self.connections.read().await;
            if let Some(sender) = connections.get(peer) {
                sender.send(data).await.map_err(|_| TransportError::SendFailed("channel closed".into()))?;
                Ok(())
            } else {
                Err(TransportError::PeerNotConnected(peer.short_id()))
            }
        }

        async fn recv(&self) -> Result<(SimulationIdentity, Vec<u8>), TransportError> {
            let mut inbox = self.inbox.write().await;
            inbox.recv().await.ok_or(TransportError::ReceiveFailed("channel closed".into()))
        }

        fn is_connected(&self, peer: &SimulationIdentity) -> bool {
            // We'd need a sync way to check this; for test just return false
            false
        }

        fn connected_peers(&self) -> Vec<SimulationIdentity> {
            Vec::new()
        }

        async fn ensure_connected(&self, _peer: &SimulationIdentity) -> Result<(), TransportError> {
            Ok(())
        }
    }

    #[test]
    fn test_transport_trait_compiles() {
        // Just verify the trait is well-formed
        fn assert_transport<T: Transport<SimulationIdentity>>(_: &T) {}
        fn assert_broadcast<T: BroadcastTransport<SimulationIdentity>>(_: &T) {}
    }
}
