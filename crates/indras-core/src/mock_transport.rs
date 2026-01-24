//! Mock transport implementation for testing
//!
//! Provides an in-memory transport for testing sync and routing logic
//! without requiring real network connections.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use indras_core::{MockTransportBuilder, SimulationIdentity, Transport};
//!
//! // Create a network of connected mock transports
//! let builder = MockTransportBuilder::new();
//! let alice = SimulationIdentity::new('A').unwrap();
//! let bob = SimulationIdentity::new('B').unwrap();
//!
//! let (transport_a, transport_b) = builder.create_connected_pair(alice, bob);
//!
//! // Now alice and bob can send/receive messages
//! transport_a.send(&bob, b"Hello Bob!".to_vec()).await.unwrap();
//! let (sender, data) = transport_b.recv().await.unwrap();
//! assert_eq!(sender, alice);
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::{RwLock, mpsc};

use crate::error::TransportError;
use crate::identity::PeerIdentity;
use crate::transport::Transport;

/// Message in the mock transport
#[derive(Debug, Clone)]
pub struct MockMessage<I> {
    /// The sender of the message
    pub sender: I,
    /// The message payload
    pub data: Vec<u8>,
}

/// A mock transport for testing
///
/// Messages are delivered via in-memory channels. Each MockTransport
/// maintains connections to other transports via channel pairs.
pub struct MockTransport<I: PeerIdentity> {
    /// Our identity
    local_id: I,
    /// Outgoing channels to peers (peer -> sender channel)
    outgoing: DashMap<I, mpsc::Sender<MockMessage<I>>>,
    /// Incoming message channel receiver
    inbox_rx: Arc<RwLock<mpsc::Receiver<MockMessage<I>>>>,
    /// Incoming message channel sender (for peers to send to us)
    inbox_tx: mpsc::Sender<MockMessage<I>>,
    /// Channel buffer size (kept for potential future use)
    #[allow(dead_code)]
    buffer_size: usize,
}

impl<I: PeerIdentity> MockTransport<I> {
    /// Create a new mock transport with the given identity
    pub fn new(local_id: I) -> Self {
        Self::with_buffer_size(local_id, 1024)
    }

    /// Create a new mock transport with a specific buffer size
    pub fn with_buffer_size(local_id: I, buffer_size: usize) -> Self {
        let (inbox_tx, inbox_rx) = mpsc::channel(buffer_size);
        Self {
            local_id,
            outgoing: DashMap::new(),
            inbox_rx: Arc::new(RwLock::new(inbox_rx)),
            inbox_tx,
            buffer_size,
        }
    }

    /// Get our local identity
    pub fn local_id(&self) -> &I {
        &self.local_id
    }

    /// Get the inbox sender for this transport
    ///
    /// This is used by other MockTransports to send messages to us.
    pub fn inbox_sender(&self) -> mpsc::Sender<MockMessage<I>> {
        self.inbox_tx.clone()
    }

    /// Connect to another mock transport
    ///
    /// This establishes a one-way channel. For bidirectional communication,
    /// the other transport should also call `connect_to` on this transport.
    pub fn connect_to(&self, peer_id: I, peer_inbox: mpsc::Sender<MockMessage<I>>) {
        self.outgoing.insert(peer_id, peer_inbox);
    }

    /// Disconnect from a peer
    pub fn disconnect_from(&self, peer: &I) {
        self.outgoing.remove(peer);
    }
}

#[async_trait]
impl<I: PeerIdentity> Transport<I> for MockTransport<I> {
    async fn send(&self, peer: &I, data: Vec<u8>) -> Result<(), TransportError> {
        let sender = self
            .outgoing
            .get(peer)
            .ok_or_else(|| TransportError::PeerNotConnected(peer.short_id()))?;

        let msg = MockMessage {
            sender: self.local_id.clone(),
            data,
        };

        sender
            .send(msg)
            .await
            .map_err(|_: mpsc::error::SendError<MockMessage<I>>| {
                TransportError::SendFailed("channel closed".into())
            })?;

        Ok(())
    }

    async fn recv(&self) -> Result<(I, Vec<u8>), TransportError> {
        let mut inbox = self.inbox_rx.write().await;
        let msg = inbox
            .recv()
            .await
            .ok_or_else(|| TransportError::ReceiveFailed("channel closed".into()))?;
        Ok((msg.sender, msg.data))
    }

    fn is_connected(&self, peer: &I) -> bool {
        self.outgoing.contains_key(peer)
    }

    fn connected_peers(&self) -> Vec<I> {
        self.outgoing
            .iter()
            .map(
                |entry: dashmap::mapref::multiple::RefMulti<
                    '_,
                    I,
                    mpsc::Sender<MockMessage<I>>,
                >| { entry.key().clone() },
            )
            .collect()
    }

    async fn ensure_connected(&self, peer: &I) -> Result<(), TransportError> {
        if self.is_connected(peer) {
            Ok(())
        } else {
            Err(TransportError::PeerNotConnected(peer.short_id()))
        }
    }

    async fn try_recv(&self) -> Result<Option<(I, Vec<u8>)>, TransportError> {
        let mut inbox = self.inbox_rx.write().await;
        match inbox.try_recv() {
            Ok(msg) => Ok(Some((msg.sender, msg.data))),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                Err(TransportError::ReceiveFailed("channel disconnected".into()))
            }
        }
    }

    async fn disconnect(&self, peer: &I) -> Result<(), TransportError> {
        self.outgoing.remove(peer);
        Ok(())
    }
}

/// Builder for creating interconnected mock transports
///
/// This provides convenient methods for creating networks of mock transports
/// for testing purposes.
pub struct MockTransportBuilder {
    buffer_size: usize,
}

impl Default for MockTransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTransportBuilder {
    /// Create a new builder with default settings
    pub fn new() -> Self {
        Self { buffer_size: 1024 }
    }

    /// Set the buffer size for channels
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Create a pair of connected mock transports
    ///
    /// Both transports can send and receive messages from each other.
    pub fn create_connected_pair<I: PeerIdentity>(
        &self,
        id_a: I,
        id_b: I,
    ) -> (MockTransport<I>, MockTransport<I>) {
        let transport_a = MockTransport::with_buffer_size(id_a.clone(), self.buffer_size);
        let transport_b = MockTransport::with_buffer_size(id_b.clone(), self.buffer_size);

        // Connect A -> B
        transport_a.connect_to(id_b, transport_b.inbox_sender());
        // Connect B -> A
        transport_b.connect_to(id_a, transport_a.inbox_sender());

        (transport_a, transport_b)
    }

    /// Create a network of fully connected mock transports
    ///
    /// Returns a map from identity to transport, where each transport
    /// can send to every other transport.
    pub fn create_full_mesh<I: PeerIdentity>(&self, ids: Vec<I>) -> HashMap<I, MockTransport<I>> {
        // First, create all transports
        let transports: HashMap<I, MockTransport<I>> = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    MockTransport::with_buffer_size(id.clone(), self.buffer_size),
                )
            })
            .collect();

        // Then, connect each pair
        for (id_a, transport_a) in &transports {
            for (id_b, transport_b) in &transports {
                if id_a != id_b {
                    transport_a.connect_to(id_b.clone(), transport_b.inbox_sender());
                }
            }
        }

        transports
    }

    /// Create a linear chain of connected transports
    ///
    /// A -> B -> C -> D (each connected only to neighbors)
    pub fn create_chain<I: PeerIdentity>(&self, ids: Vec<I>) -> HashMap<I, MockTransport<I>> {
        let transports: HashMap<I, MockTransport<I>> = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    MockTransport::with_buffer_size(id.clone(), self.buffer_size),
                )
            })
            .collect();

        for i in 0..ids.len().saturating_sub(1) {
            let id_a = &ids[i];
            let id_b = &ids[i + 1];

            if let (Some(transport_a), Some(transport_b)) =
                (transports.get(id_a), transports.get(id_b))
            {
                // Bidirectional connection
                transport_a.connect_to(id_b.clone(), transport_b.inbox_sender());
                transport_b.connect_to(id_a.clone(), transport_a.inbox_sender());
            }
        }

        transports
    }

    /// Create a star topology with a central hub
    ///
    /// All spokes are connected only to the hub.
    pub fn create_star<I: PeerIdentity>(
        &self,
        hub: I,
        spokes: Vec<I>,
    ) -> HashMap<I, MockTransport<I>> {
        let hub_transport = MockTransport::with_buffer_size(hub.clone(), self.buffer_size);

        let mut transports = HashMap::new();

        for spoke_id in spokes {
            let spoke_transport =
                MockTransport::with_buffer_size(spoke_id.clone(), self.buffer_size);

            // Connect hub <-> spoke
            hub_transport.connect_to(spoke_id.clone(), spoke_transport.inbox_sender());
            spoke_transport.connect_to(hub.clone(), hub_transport.inbox_sender());

            transports.insert(spoke_id, spoke_transport);
        }

        transports.insert(hub, hub_transport);
        transports
    }
}

/// A mock transport network for managing multiple interconnected transports
pub struct MockNetwork<I: PeerIdentity> {
    transports: HashMap<I, Arc<MockTransport<I>>>,
}

impl<I: PeerIdentity> MockNetwork<I> {
    /// Create a new mock network from a full mesh
    pub fn full_mesh(ids: Vec<I>) -> Self {
        let builder = MockTransportBuilder::new();
        let transports = builder
            .create_full_mesh(ids)
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect();
        Self { transports }
    }

    /// Get a transport by identity
    pub fn get(&self, id: &I) -> Option<Arc<MockTransport<I>>> {
        self.transports.get(id).cloned()
    }

    /// Get all transports
    pub fn all(&self) -> Vec<Arc<MockTransport<I>>> {
        self.transports.values().cloned().collect()
    }

    /// Get all identities
    pub fn identities(&self) -> Vec<I> {
        self.transports.keys().cloned().collect()
    }

    /// Partition the network (disconnect nodes between partitions)
    pub fn partition(&self, partition_a: &[I], partition_b: &[I]) {
        for id_a in partition_a {
            if let Some(transport_a) = self.transports.get(id_a) {
                for id_b in partition_b {
                    transport_a.disconnect_from(id_b);
                }
            }
        }

        for id_b in partition_b {
            if let Some(transport_b) = self.transports.get(id_b) {
                for id_a in partition_a {
                    transport_b.disconnect_from(id_a);
                }
            }
        }
    }

    /// Heal a network partition (reconnect nodes)
    pub fn heal_partition(&self, partition_a: &[I], partition_b: &[I]) {
        for id_a in partition_a {
            if let Some(transport_a) = self.transports.get(id_a) {
                for id_b in partition_b {
                    if let Some(transport_b) = self.transports.get(id_b) {
                        transport_a.connect_to(id_b.clone(), transport_b.inbox_sender());
                    }
                }
            }
        }

        for id_b in partition_b {
            if let Some(transport_b) = self.transports.get(id_b) {
                for id_a in partition_a {
                    if let Some(transport_a) = self.transports.get(id_a) {
                        transport_b.connect_to(id_a.clone(), transport_a.inbox_sender());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SimulationIdentity;

    #[tokio::test]
    async fn test_mock_transport_send_recv() {
        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();

        let builder = MockTransportBuilder::new();
        let (transport_a, transport_b) = builder.create_connected_pair(alice, bob);

        // Alice sends to Bob
        let message = b"Hello Bob!".to_vec();
        transport_a.send(&bob, message.clone()).await.unwrap();

        // Bob receives
        let (sender, data) = transport_b.recv().await.unwrap();
        assert_eq!(sender, alice);
        assert_eq!(data, message);
    }

    #[tokio::test]
    async fn test_mock_transport_bidirectional() {
        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();

        let builder = MockTransportBuilder::new();
        let (transport_a, transport_b) = builder.create_connected_pair(alice, bob);

        // Alice sends to Bob
        transport_a
            .send(&bob, b"Hello Bob!".to_vec())
            .await
            .unwrap();

        // Bob receives and replies
        let (sender, _) = transport_b.recv().await.unwrap();
        assert_eq!(sender, alice);

        transport_b
            .send(&alice, b"Hello Alice!".to_vec())
            .await
            .unwrap();

        // Alice receives reply
        let (sender, data) = transport_a.recv().await.unwrap();
        assert_eq!(sender, bob);
        assert_eq!(data, b"Hello Alice!".to_vec());
    }

    #[tokio::test]
    async fn test_mock_transport_full_mesh() {
        let ids: Vec<_> = ('A'..='C').filter_map(SimulationIdentity::new).collect();

        let builder = MockTransportBuilder::new();
        let transports = builder.create_full_mesh(ids.clone());

        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();
        let charlie = SimulationIdentity::new('C').unwrap();

        let transport_a = transports.get(&alice).unwrap();
        let transport_b = transports.get(&bob).unwrap();
        let transport_c = transports.get(&charlie).unwrap();

        // Alice broadcasts to all
        transport_a.send(&bob, b"Hi Bob".to_vec()).await.unwrap();
        transport_a
            .send(&charlie, b"Hi Charlie".to_vec())
            .await
            .unwrap();

        // Both receive
        let (_, data_b) = transport_b.recv().await.unwrap();
        let (_, data_c) = transport_c.recv().await.unwrap();

        assert_eq!(data_b, b"Hi Bob".to_vec());
        assert_eq!(data_c, b"Hi Charlie".to_vec());
    }

    #[tokio::test]
    async fn test_mock_transport_not_connected() {
        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();

        let transport_a = MockTransport::new(alice);

        // Try to send to unconnected peer
        let result = transport_a.send(&bob, b"Hello".to_vec()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_transport_connected_peers() {
        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();
        let charlie = SimulationIdentity::new('C').unwrap();

        let builder = MockTransportBuilder::new();
        let (transport_a, _transport_b) = builder.create_connected_pair(alice, bob);

        // Alice is connected to Bob
        assert!(transport_a.is_connected(&bob));
        assert!(!transport_a.is_connected(&charlie));

        let peers = transport_a.connected_peers();
        assert_eq!(peers.len(), 1);
        assert!(peers.contains(&bob));
    }

    #[tokio::test]
    async fn test_mock_transport_try_recv() {
        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();

        let builder = MockTransportBuilder::new();
        let (transport_a, transport_b) = builder.create_connected_pair(alice, bob);

        // No messages yet
        let result = transport_b.try_recv().await.unwrap();
        assert!(result.is_none());

        // Send a message
        transport_a.send(&bob, b"Hello".to_vec()).await.unwrap();

        // Now there's a message
        let result = transport_b.try_recv().await.unwrap();
        assert!(result.is_some());
        let (sender, data) = result.unwrap();
        assert_eq!(sender, alice);
        assert_eq!(data, b"Hello".to_vec());
    }

    #[tokio::test]
    async fn test_mock_network_partition() {
        let ids: Vec<_> = ('A'..='D').filter_map(SimulationIdentity::new).collect();

        let network = MockNetwork::full_mesh(ids);

        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();
        let charlie = SimulationIdentity::new('C').unwrap();
        let dan = SimulationIdentity::new('D').unwrap();

        let transport_a = network.get(&alice).unwrap();
        let transport_c = network.get(&charlie).unwrap();

        // Before partition: A can send to C
        transport_a.send(&charlie, b"Hi".to_vec()).await.unwrap();
        let _ = transport_c.recv().await.unwrap();

        // Partition: {A, B} and {C, D}
        network.partition(&[alice, bob], &[charlie, dan]);

        // After partition: A cannot send to C
        let result = transport_a.send(&charlie, b"Hi".to_vec()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_transport_chain() {
        let ids: Vec<_> = ('A'..='C').filter_map(SimulationIdentity::new).collect();

        let builder = MockTransportBuilder::new();
        let transports = builder.create_chain(ids);

        let alice = SimulationIdentity::new('A').unwrap();
        let bob = SimulationIdentity::new('B').unwrap();
        let charlie = SimulationIdentity::new('C').unwrap();

        let transport_a = transports.get(&alice).unwrap();
        let transport_b = transports.get(&bob).unwrap();

        // A is connected to B
        assert!(transport_a.is_connected(&bob));
        // A is NOT directly connected to C
        assert!(!transport_a.is_connected(&charlie));
        // B is connected to both A and C
        assert!(transport_b.is_connected(&alice));
        assert!(transport_b.is_connected(&charlie));
    }

    #[tokio::test]
    async fn test_mock_transport_star() {
        let hub = SimulationIdentity::new('H').unwrap();
        let spokes: Vec<_> = ('A'..='C').filter_map(SimulationIdentity::new).collect();

        let builder = MockTransportBuilder::new();
        let transports = builder.create_star(hub, spokes.clone());

        let transport_hub = transports.get(&hub).unwrap();
        let spoke_a = spokes[0];
        let spoke_b = spokes[1];
        let transport_a = transports.get(&spoke_a).unwrap();

        // Hub is connected to all spokes
        for spoke in &spokes {
            assert!(transport_hub.is_connected(spoke));
        }

        // Spokes are connected to hub only
        assert!(transport_a.is_connected(&hub));
        assert!(!transport_a.is_connected(&spoke_b));
    }
}
