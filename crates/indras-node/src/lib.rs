//! # Indras Node
//!
//! High-level P2P node coordinator for Indras Network.
//!
//! This crate provides [`IndrasNode`], which ties together:
//! - Transport layer (iroh-based networking)
//! - Storage layer (append-only logs + redb + blobs)
//! - Sync layer (Automerge CRDT sync)
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_node::{IndrasNode, NodeConfig};
//!
//! // Create and start a node
//! let config = NodeConfig::with_data_dir("./my-node");
//! let node = IndrasNode::new(config).await?;
//! node.start().await?;
//!
//! // Create a new interface (shared space)
//! let (interface_id, invite_key) = node.create_interface("My Chat").await?;
//!
//! // Send a message
//! let event_id = node.send_message(&interface_id, b"Hello!".to_vec()).await?;
//!
//! // Subscribe to events
//! let mut events = node.events(&interface_id)?;
//! while let Ok(event) = events.recv().await {
//!     println!("Received: {:?}", event);
//! }
//! ```

mod config;
mod error;

pub use config::NodeConfig;
pub use error::{NodeError, NodeResult};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, instrument};

use indras_core::{EventId, InterfaceEvent, InterfaceId, PeerIdentity, NInterfaceTrait};
use indras_storage::CompositeStorage;
use indras_sync::NInterface;
use indras_transport::IrohIdentity;

/// Invite key for joining an interface
///
/// Contains the interface ID and optional bootstrap peer addresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteKey {
    /// The interface to join
    pub interface_id: InterfaceId,
    /// Bootstrap peer addresses (serialized)
    pub bootstrap_peers: Vec<Vec<u8>>,
}

impl InviteKey {
    /// Create a new invite key
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            bootstrap_peers: Vec::new(),
        }
    }

    /// Add a bootstrap peer
    pub fn with_bootstrap(mut self, peer_addr: Vec<u8>) -> Self {
        self.bootstrap_peers.push(peer_addr);
        self
    }

    /// Serialize to bytes for sharing
    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }

    /// Encode as base64 for easy sharing
    pub fn to_base64(&self) -> Result<String, postcard::Error> {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        let bytes = self.to_bytes()?;
        Ok(URL_SAFE_NO_PAD.encode(&bytes))
    }

    /// Decode from base64
    pub fn from_base64(s: &str) -> Result<Self, NodeError> {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        let bytes = URL_SAFE_NO_PAD
            .decode(s)
            .map_err(|e| NodeError::Serialization(e.to_string()))?;
        Self::from_bytes(&bytes).map_err(|e| NodeError::Serialization(e.to_string()))
    }
}

/// Event received from an interface
#[derive(Debug, Clone)]
pub struct ReceivedEvent {
    /// The interface this event is from
    pub interface_id: InterfaceId,
    /// The event itself
    pub event: InterfaceEvent<IrohIdentity>,
}

/// State for a loaded interface
struct InterfaceState {
    /// The NInterface managing sync and events
    interface: RwLock<NInterface<IrohIdentity>>,
    /// Broadcast channel for events
    event_tx: broadcast::Sender<ReceivedEvent>,
}

/// High-level P2P node coordinator
///
/// IndrasNode provides a unified API for P2P networking, storage, and sync.
/// It manages interfaces (shared spaces where N peers collaborate) and handles
/// message delivery, CRDT synchronization, and persistence.
pub struct IndrasNode {
    /// Node configuration
    config: NodeConfig,
    /// Our identity
    identity: IrohIdentity,
    /// Composite storage (logs + redb + blobs)
    storage: Arc<CompositeStorage<IrohIdentity>>,
    /// Loaded interfaces
    interfaces: DashMap<InterfaceId, InterfaceState>,
    /// Whether the node has been started
    started: AtomicBool,
}

impl IndrasNode {
    /// Create a new node
    ///
    /// This initializes storage but does not start networking.
    /// Call [`start`](Self::start) to begin accepting connections.
    #[instrument(skip(config), fields(data_dir = %config.data_dir.display()))]
    pub async fn new(config: NodeConfig) -> NodeResult<Self> {
        // Ensure data directory exists
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .map_err(|e| NodeError::Io(e.to_string()))?;

        // Initialize storage
        let storage = CompositeStorage::new(config.storage.clone()).await?;
        let storage = Arc::new(storage);

        // Generate identity (in production, load from keystore)
        let secret_key = iroh::SecretKey::generate(&mut rand::rng());
        let identity = IrohIdentity::new(secret_key.public());

        info!(identity = %identity.short_id(), "Node created");

        Ok(Self {
            config,
            identity,
            storage,
            interfaces: DashMap::new(),
            started: AtomicBool::new(false),
        })
    }

    /// Create a node with a specific identity
    ///
    /// Use this when you have an existing secret key.
    #[instrument(skip(config, secret_key))]
    pub async fn with_identity(
        config: NodeConfig,
        secret_key: iroh::SecretKey,
    ) -> NodeResult<Self> {
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .map_err(|e| NodeError::Io(e.to_string()))?;

        let storage = CompositeStorage::new(config.storage.clone()).await?;
        let storage = Arc::new(storage);

        let identity = IrohIdentity::new(secret_key.public());

        info!(identity = %identity.short_id(), "Node created with existing identity");

        Ok(Self {
            config,
            identity,
            storage,
            interfaces: DashMap::new(),
            started: AtomicBool::new(false),
        })
    }

    /// Start the node
    ///
    /// Begins accepting connections and processing messages.
    /// This is currently a no-op placeholder - full networking
    /// will be enabled when transport integration is complete.
    #[instrument(skip(self))]
    pub async fn start(&self) -> NodeResult<()> {
        if self.started.swap(true, Ordering::SeqCst) {
            return Err(NodeError::AlreadyStarted);
        }

        // TODO: Start transport adapter
        // TODO: Start background sync tasks
        // TODO: Load persisted interfaces

        info!("Node started");
        Ok(())
    }

    /// Stop the node
    #[instrument(skip(self))]
    pub async fn stop(&self) -> NodeResult<()> {
        if !self.started.swap(false, Ordering::SeqCst) {
            return Ok(()); // Already stopped
        }

        // Close storage
        self.storage.close().await?;

        info!("Node stopped");
        Ok(())
    }

    /// Get our identity
    pub fn identity(&self) -> &IrohIdentity {
        &self.identity
    }

    /// Check if the node is started
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// Create a new interface
    ///
    /// Returns the interface ID and an invite key for sharing with peers.
    #[instrument(skip(self))]
    pub async fn create_interface(&self, name: Option<&str>) -> NodeResult<(InterfaceId, InviteKey)> {
        // Create NInterface with us as the creator
        let interface = NInterface::new(self.identity.clone());
        let interface_id = interface.id();

        // Persist to storage
        self.storage.create_interface(interface_id, name.map(|s| s.to_string()))?;
        self.storage.register_peer(&self.identity, None)?;
        self.storage.add_member(&interface_id, &self.identity)?;

        // Create event channel
        let (event_tx, _) = broadcast::channel(self.config.event_channel_capacity);

        // Store in memory
        let state = InterfaceState {
            interface: RwLock::new(interface),
            event_tx,
        };
        self.interfaces.insert(interface_id, state);

        info!(interface_id = %hex::encode(interface_id.as_bytes()), "Interface created");

        let invite = InviteKey::new(interface_id);
        Ok((interface_id, invite))
    }

    /// Join an existing interface using an invite key
    #[instrument(skip(self, invite))]
    pub async fn join_interface(&self, invite: InviteKey) -> NodeResult<InterfaceId> {
        let interface_id = invite.interface_id;

        // Check if we're already in this interface
        if self.interfaces.contains_key(&interface_id) {
            return Ok(interface_id);
        }

        // Create NInterface with known ID
        let interface = NInterface::with_id(interface_id, self.identity.clone());

        // Persist to storage
        self.storage.create_interface(interface_id, None)?;
        self.storage.register_peer(&self.identity, None)?;
        self.storage.add_member(&interface_id, &self.identity)?;

        // Create event channel
        let (event_tx, _) = broadcast::channel(self.config.event_channel_capacity);

        // Store in memory
        let state = InterfaceState {
            interface: RwLock::new(interface),
            event_tx,
        };
        self.interfaces.insert(interface_id, state);

        // TODO: Connect to bootstrap peers and sync

        info!(interface_id = %hex::encode(interface_id.as_bytes()), "Joined interface");
        Ok(interface_id)
    }

    /// Send a message to an interface
    #[instrument(skip(self, content), fields(interface_id = %hex::encode(interface_id.as_bytes())))]
    pub async fn send_message(
        &self,
        interface_id: &InterfaceId,
        content: Vec<u8>,
    ) -> NodeResult<EventId> {
        let state = self.interfaces.get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        // Create the event
        let mut interface = state.interface.write().await;
        let sequence = interface.event_count() as u64 + 1;
        let event = InterfaceEvent::message(self.identity.clone(), sequence, content.clone());

        // Append to NInterface (tracks pending delivery + CRDT)
        let event_id = interface.append(event.clone()).await?;

        // Also persist to storage
        self.storage
            .append_event(interface_id, event_id, Bytes::from(content))
            .await?;

        // Broadcast locally
        let received = ReceivedEvent {
            interface_id: *interface_id,
            event,
        };
        let _ = state.event_tx.send(received);

        debug!(event_id = ?event_id, "Message sent");
        Ok(event_id)
    }

    /// Subscribe to events from an interface
    ///
    /// Returns a broadcast receiver that will receive all events.
    pub fn events(&self, interface_id: &InterfaceId) -> NodeResult<broadcast::Receiver<ReceivedEvent>> {
        let state = self.interfaces.get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        Ok(state.event_tx.subscribe())
    }

    /// Get events since a sequence number
    pub async fn events_since(
        &self,
        interface_id: &InterfaceId,
        since: u64,
    ) -> NodeResult<Vec<InterfaceEvent<IrohIdentity>>> {
        let state = self.interfaces.get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        let interface = state.interface.read().await;
        Ok(interface.events_since(since))
    }

    /// Get all members of an interface
    pub async fn members(
        &self,
        interface_id: &InterfaceId,
    ) -> NodeResult<Vec<IrohIdentity>> {
        let state = self.interfaces.get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        let interface = state.interface.read().await;
        Ok(interface.members().into_iter().collect())
    }

    /// Add a member to an interface
    pub async fn add_member(
        &self,
        interface_id: &InterfaceId,
        peer: IrohIdentity,
    ) -> NodeResult<()> {
        let state = self.interfaces.get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        let mut interface = state.interface.write().await;
        interface.add_member(peer.clone())
            .map_err(|e| NodeError::Sync(e.to_string()))?;

        // Persist
        self.storage.register_peer(&peer, None)?;
        self.storage.add_member(interface_id, &peer)?;

        info!(peer = %peer.short_id(), "Member added");
        Ok(())
    }

    /// Get the storage backend (for advanced operations)
    pub fn storage(&self) -> &CompositeStorage<IrohIdentity> {
        &self.storage
    }

    /// List all loaded interfaces
    pub fn list_interfaces(&self) -> Vec<InterfaceId> {
        self.interfaces.iter().map(|entry| *entry.key()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_node() -> (IndrasNode, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = NodeConfig::with_data_dir(temp_dir.path());
        let node = IndrasNode::new(config).await.unwrap();
        (node, temp_dir)
    }

    #[tokio::test]
    async fn test_node_creation() {
        let (node, _temp) = create_test_node().await;

        assert!(!node.is_started());
        assert!(!node.identity().short_id().is_empty());
    }

    #[tokio::test]
    async fn test_start_stop() {
        let (node, _temp) = create_test_node().await;

        // Start
        node.start().await.unwrap();
        assert!(node.is_started());

        // Can't start twice
        assert!(matches!(node.start().await, Err(NodeError::AlreadyStarted)));

        // Stop
        node.stop().await.unwrap();
        assert!(!node.is_started());
    }

    #[tokio::test]
    async fn test_create_interface() {
        let (node, _temp) = create_test_node().await;

        let (interface_id, invite) = node.create_interface(Some("Test")).await.unwrap();

        assert_eq!(invite.interface_id, interface_id);
        assert!(node.list_interfaces().contains(&interface_id));
    }

    #[tokio::test]
    async fn test_send_message() {
        let (node, _temp) = create_test_node().await;
        let (interface_id, _) = node.create_interface(None).await.unwrap();

        // Subscribe to events
        let mut events = node.events(&interface_id).unwrap();

        // Send a message
        let event_id = node
            .send_message(&interface_id, b"Hello!".to_vec())
            .await
            .unwrap();

        assert_eq!(event_id.sequence, 1);

        // Check we received it
        let received = events.try_recv().unwrap();
        assert_eq!(received.interface_id, interface_id);
        match received.event {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Hello!");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[tokio::test]
    async fn test_events_since() {
        let (node, _temp) = create_test_node().await;
        let (interface_id, _) = node.create_interface(None).await.unwrap();

        // Send multiple messages
        node.send_message(&interface_id, b"First".to_vec()).await.unwrap();
        node.send_message(&interface_id, b"Second".to_vec()).await.unwrap();
        node.send_message(&interface_id, b"Third".to_vec()).await.unwrap();

        // Get events since sequence 1
        let events = node.events_since(&interface_id, 1).await.unwrap();
        assert_eq!(events.len(), 2); // Second and Third
    }

    #[tokio::test]
    async fn test_members() {
        let (node, _temp) = create_test_node().await;
        let (interface_id, _) = node.create_interface(None).await.unwrap();

        let members = node.members(&interface_id).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0], *node.identity());
    }

    #[tokio::test]
    async fn test_join_interface() {
        let (node, _temp) = create_test_node().await;
        let (interface_id, invite) = node.create_interface(None).await.unwrap();

        // Join again (should be idempotent)
        let joined_id = node.join_interface(invite.clone()).await.unwrap();
        assert_eq!(joined_id, interface_id);
    }

    #[tokio::test]
    async fn test_invite_key_serialization() {
        let interface_id = InterfaceId::generate();
        let invite = InviteKey::new(interface_id)
            .with_bootstrap(vec![1, 2, 3]);

        // Bytes roundtrip
        let bytes = invite.to_bytes().unwrap();
        let restored = InviteKey::from_bytes(&bytes).unwrap();
        assert_eq!(restored.interface_id, interface_id);
        assert_eq!(restored.bootstrap_peers.len(), 1);

        // Base64 roundtrip
        let b64 = invite.to_base64().unwrap();
        let restored = InviteKey::from_base64(&b64).unwrap();
        assert_eq!(restored.interface_id, interface_id);
    }

    #[tokio::test]
    async fn test_interface_not_found() {
        let (node, _temp) = create_test_node().await;
        let fake_id = InterfaceId::generate();

        assert!(matches!(
            node.events(&fake_id),
            Err(NodeError::InterfaceNotFound(_))
        ));

        assert!(matches!(
            node.send_message(&fake_id, vec![]).await,
            Err(NodeError::InterfaceNotFound(_))
        ));
    }
}
