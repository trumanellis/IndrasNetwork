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
mod keystore;
pub mod message_handler;
pub mod sync_task;

pub use config::NodeConfig;
pub use error::{NodeError, NodeResult};
pub use keystore::Keystore;
pub use message_handler::{NetworkMessage, InterfaceEventMessage, InterfaceSyncRequest, InterfaceSyncResponse, EventAckMessage};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, warn};

use indras_core::{EventId, InterfaceEvent, InterfaceId, PeerIdentity, NInterfaceTrait};
use indras_core::transport::Transport;
use indras_crypto::{InterfaceKey, KeyDistribution, KeyInvite};
use indras_storage::CompositeStorage;
use indras_sync::NInterface;
use indras_transport::{IrohIdentity, IrohNetworkAdapter};

use message_handler::MessageHandler;
use sync_task::SyncTask;

/// Default sync interval in seconds
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 5;

/// Invite key for joining an interface
///
/// Contains the interface ID, optional bootstrap peer addresses, and optional encrypted key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteKey {
    /// The interface to join
    pub interface_id: InterfaceId,
    /// Bootstrap peer addresses (serialized)
    pub bootstrap_peers: Vec<Vec<u8>>,
    /// Encrypted interface key (KeyInvite serialized) for the invitee
    pub key_invite: Option<Vec<u8>>,
}

impl InviteKey {
    /// Create a new invite key
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            bootstrap_peers: Vec::new(),
            key_invite: None,
        }
    }

    /// Add a bootstrap peer
    pub fn with_bootstrap(mut self, peer_addr: Vec<u8>) -> Self {
        self.bootstrap_peers.push(peer_addr);
        self
    }

    /// Add an encrypted key invite
    pub fn with_key_invite(mut self, key_invite: Vec<u8>) -> Self {
        self.key_invite = Some(key_invite);
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
pub struct InterfaceState {
    /// The NInterface managing sync and events
    pub interface: RwLock<NInterface<IrohIdentity>>,
    /// Broadcast channel for events
    pub event_tx: broadcast::Sender<ReceivedEvent>,
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
    /// Our secret key (for crypto operations)
    secret_key: iroh::SecretKey,
    /// Composite storage (logs + redb + blobs)
    storage: Arc<CompositeStorage<IrohIdentity>>,
    /// Loaded interfaces
    interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
    /// Interface encryption keys
    interface_keys: Arc<DashMap<InterfaceId, InterfaceKey>>,
    /// Transport adapter (None until started)
    transport: RwLock<Option<Arc<IrohNetworkAdapter>>>,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
    /// Background task handles
    background_tasks: RwLock<Vec<JoinHandle<()>>>,
    /// Whether the node has been started
    started: AtomicBool,
}

impl IndrasNode {
    /// Create a new node
    ///
    /// This initializes storage and loads identity from keystore.
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

        // Load or generate identity from keystore
        let keystore = Keystore::new(&config.data_dir);
        let secret_key = keystore.load_or_generate()?;
        let identity = IrohIdentity::new(secret_key.public());

        let (shutdown_tx, _) = broadcast::channel(1);

        info!(identity = %identity.short_id(), "Node created");

        Ok(Self {
            config,
            identity,
            secret_key,
            storage,
            interfaces: Arc::new(DashMap::new()),
            interface_keys: Arc::new(DashMap::new()),
            transport: RwLock::new(None),
            shutdown_tx,
            background_tasks: RwLock::new(Vec::new()),
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

        // Save the provided key to keystore
        let keystore = Keystore::new(&config.data_dir);
        keystore.save(&secret_key)?;

        let identity = IrohIdentity::new(secret_key.public());
        let (shutdown_tx, _) = broadcast::channel(1);

        info!(identity = %identity.short_id(), "Node created with existing identity");

        Ok(Self {
            config,
            identity,
            secret_key,
            storage,
            interfaces: Arc::new(DashMap::new()),
            interface_keys: Arc::new(DashMap::new()),
            transport: RwLock::new(None),
            shutdown_tx,
            background_tasks: RwLock::new(Vec::new()),
            started: AtomicBool::new(false),
        })
    }

    /// Start the node
    ///
    /// Begins accepting connections and processing messages.
    #[instrument(skip(self))]
    pub async fn start(&self) -> NodeResult<()> {
        if self.started.swap(true, Ordering::SeqCst) {
            return Err(NodeError::AlreadyStarted);
        }

        // Start transport adapter
        let adapter = IrohNetworkAdapter::new(self.secret_key.clone(), self.config.transport.clone()).await?;
        adapter.start(vec![]).await?;
        let adapter = Arc::new(adapter);
        *self.transport.write().await = Some(adapter.clone());

        // Load persisted interfaces
        self.load_persisted_interfaces().await?;

        // Create message channel for incoming messages
        let (message_tx, message_rx) = mpsc::channel(1024);

        // Spawn message receiver task
        let transport_clone = adapter.clone();
        let message_tx_clone = message_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let receiver_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    result = transport_clone.recv() => {
                        match result {
                            Ok((sender, data)) => {
                                if message_tx_clone.send((sender, data)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "Transport receive error");
                            }
                        }
                    }
                }
            }
        });

        // Spawn message handler
        let handler_task = MessageHandler::spawn(
            self.identity,
            self.interface_keys.clone(),
            self.interfaces.clone(),
            self.storage.clone(),
            self.shutdown_tx.subscribe(),
            message_rx,
        );

        // Spawn sync task
        let sync_task = SyncTask::spawn(
            self.identity,
            adapter.clone(),
            self.interface_keys.clone(),
            self.interfaces.clone(),
            self.storage.clone(),
            Duration::from_secs(DEFAULT_SYNC_INTERVAL_SECS),
            self.shutdown_tx.subscribe(),
        );

        // Store task handles
        {
            let mut tasks = self.background_tasks.write().await;
            tasks.push(receiver_task);
            tasks.push(handler_task);
            tasks.push(sync_task);
        }

        info!("Node started");
        Ok(())
    }

    /// Load persisted interfaces from storage
    async fn load_persisted_interfaces(&self) -> NodeResult<()> {
        let interface_records = self.storage.interface_store().all()
            .map_err(|e| NodeError::Storage(e))?;

        for record in interface_records {
            let interface_id = InterfaceId::new(record.interface_id);

            // Skip if already loaded
            if self.interfaces.contains_key(&interface_id) {
                continue;
            }

            // Create NInterface with known ID
            let mut interface = NInterface::with_id(interface_id, self.identity.clone());

            // Load members from storage
            let members = self.storage.interface_store().get_members(&interface_id)
                .map_err(|e| NodeError::Storage(e))?;

            for member_record in members {
                // Reconstruct peer identity from bytes
                if member_record.peer_id.len() == 32 {
                    let mut key_bytes = [0u8; 32];
                    key_bytes.copy_from_slice(&member_record.peer_id);
                    let public_key = iroh::PublicKey::from_bytes(&key_bytes)
                        .map_err(|e| NodeError::Crypto(e.to_string()))?;
                    let peer_identity = IrohIdentity::new(public_key);

                    // Don't add ourselves twice
                    if peer_identity != self.identity {
                        let _ = interface.add_member(peer_identity);
                    }
                }
            }

            // Load interface key if stored
            if let Some(encrypted_key_bytes) = &record.encrypted_key {
                // Decrypt and restore interface key
                // For now, we store the raw key bytes (in production, use proper encryption)
                if encrypted_key_bytes.len() == 32 {
                    let mut key_bytes = [0u8; 32];
                    key_bytes.copy_from_slice(encrypted_key_bytes);
                    let interface_key = InterfaceKey::from_bytes(key_bytes, interface_id);
                    self.interface_keys.insert(interface_id, interface_key);
                }
            }

            // Create event channel
            let (event_tx, _) = broadcast::channel(self.config.event_channel_capacity);

            // Store in memory
            let state = InterfaceState {
                interface: RwLock::new(interface),
                event_tx,
            };
            self.interfaces.insert(interface_id, state);

            debug!(
                interface = %hex::encode(interface_id.as_bytes()),
                name = ?record.name,
                "Loaded persisted interface"
            );
        }

        info!(count = self.interfaces.len(), "Loaded persisted interfaces");
        Ok(())
    }

    /// Stop the node
    #[instrument(skip(self))]
    pub async fn stop(&self) -> NodeResult<()> {
        if !self.started.swap(false, Ordering::SeqCst) {
            return Ok(()); // Already stopped
        }

        // Signal shutdown
        let _ = self.shutdown_tx.send(());

        // Stop transport
        if let Some(transport) = self.transport.write().await.take() {
            transport.stop().await;
        }

        // Wait for background tasks
        let mut tasks = self.background_tasks.write().await;
        for task in tasks.drain(..) {
            let _ = task.await;
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

    /// Get our secret key
    pub fn secret_key(&self) -> &iroh::SecretKey {
        &self.secret_key
    }

    /// Check if the node is started
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// Get the transport adapter (if started)
    pub async fn transport(&self) -> Option<Arc<IrohNetworkAdapter>> {
        self.transport.read().await.clone()
    }

    /// Get our endpoint address for sharing with peers
    pub async fn endpoint_addr(&self) -> Option<iroh::EndpointAddr> {
        self.transport.read().await.as_ref().map(|t| t.endpoint_addr())
    }

    /// Create a new interface
    ///
    /// Returns the interface ID and an invite key for sharing with peers.
    #[instrument(skip(self))]
    pub async fn create_interface(&self, name: Option<&str>) -> NodeResult<(InterfaceId, InviteKey)> {
        // Create NInterface with us as the creator
        let interface = NInterface::new(self.identity.clone());
        let interface_id = interface.id();

        // Generate interface encryption key
        let interface_key = InterfaceKey::generate(interface_id);
        self.interface_keys.insert(interface_id, interface_key.clone());

        // Persist to storage (including the key)
        let mut record = indras_storage::structured::InterfaceRecord::new(interface_id);
        if let Some(n) = name {
            record = record.with_name(n);
        }
        record.encrypted = true;
        record.encrypted_key = Some(interface_key.as_bytes().to_vec());
        self.storage.interface_store().upsert(&record)?;

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

        // Create invite with our endpoint address
        let mut invite = InviteKey::new(interface_id);
        if let Some(transport) = self.transport.read().await.as_ref() {
            let addr = transport.endpoint_addr();
            // Serialize endpoint address using postcard
            if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                invite = invite.with_bootstrap(addr_bytes);
            }
        }

        Ok((interface_id, invite))
    }

    /// Create an invite for a specific peer
    ///
    /// Includes the encrypted interface key for the invitee.
    pub async fn create_invite_for(
        &self,
        interface_id: &InterfaceId,
        invitee_public: &indras_crypto::PublicKey,
    ) -> NodeResult<InviteKey> {
        // Get interface key
        let interface_key = self.interface_keys.get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        // Create X25519 key from our iroh key
        let our_x25519_secret = indras_crypto::StaticSecret::from(self.secret_key.to_bytes());

        // Create encrypted key invite
        let key_invite = KeyDistribution::create_invite(
            interface_key.value(),
            &our_x25519_secret,
            invitee_public,
        ).map_err(|e| NodeError::Crypto(e.to_string()))?;

        let key_invite_bytes = key_invite.to_bytes()
            .map_err(|e| NodeError::Crypto(e.to_string()))?;

        // Build invite
        let mut invite = InviteKey::new(*interface_id)
            .with_key_invite(key_invite_bytes);

        // Add our endpoint address
        if let Some(transport) = self.transport.read().await.as_ref() {
            let addr = transport.endpoint_addr();
            // Serialize endpoint address using postcard
            if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                invite = invite.with_bootstrap(addr_bytes);
            }
        }

        Ok(invite)
    }

    /// Join an existing interface using an invite key
    #[instrument(skip(self, invite))]
    pub async fn join_interface(&self, invite: InviteKey) -> NodeResult<InterfaceId> {
        let interface_id = invite.interface_id;

        // Check if we're already in this interface
        if self.interfaces.contains_key(&interface_id) {
            return Ok(interface_id);
        }

        // Decrypt interface key if provided
        if let Some(key_invite_bytes) = &invite.key_invite {
            let key_invite = KeyInvite::from_bytes(key_invite_bytes)
                .map_err(|e| NodeError::Crypto(e.to_string()))?;

            // Validate that the key invite is for the correct interface
            if key_invite.interface_id != interface_id {
                return Err(NodeError::Crypto(format!(
                    "Key invite interface_id mismatch: expected {}, got {}",
                    hex::encode(interface_id.as_bytes()),
                    hex::encode(key_invite.interface_id.as_bytes())
                )));
            }

            let our_x25519_secret = indras_crypto::StaticSecret::from(self.secret_key.to_bytes());
            let interface_key = KeyDistribution::accept_invite(&key_invite, &our_x25519_secret)
                .map_err(|e| NodeError::Crypto(e.to_string()))?;

            self.interface_keys.insert(interface_id, interface_key);
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

        // Connect to bootstrap peers and track which peers we connected to
        let mut bootstrap_peer_ids = Vec::new();
        if let Some(transport) = self.transport.read().await.as_ref() {
            for peer_bytes in &invite.bootstrap_peers {
                // Deserialize endpoint address using postcard
                if let Ok(addr) = postcard::from_bytes::<iroh::EndpointAddr>(peer_bytes) {
                    let peer_id = IrohIdentity::new(addr.id);
                    debug!(peer = %peer_id.short_id(), "Connecting to bootstrap peer");
                    if let Err(e) = transport.connection_manager().connect(addr).await {
                        warn!(error = %e, "Failed to connect to bootstrap peer");
                    } else {
                        bootstrap_peer_ids.push(peer_id);
                    }
                }
            }

            // Send initial sync request ONLY to bootstrap peers we connected to
            let state = self.interfaces.get(&interface_id).unwrap();
            let interface = state.interface.read().await;
            for peer in &bootstrap_peer_ids {
                if *peer != self.identity && transport.is_connected(peer) {
                    let sync_msg = interface.generate_sync(peer);

                    let request = InterfaceSyncRequest {
                        interface_id: sync_msg.interface_id,
                        heads: sync_msg.heads,
                        sync_data: sync_msg.sync_data,
                    };
                    let msg = NetworkMessage::SyncRequest(request);
                    if let Ok(bytes) = msg.to_bytes() {
                        let _ = transport.send(peer, bytes).await;
                    }
                }
            }
        }

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
            .append_event(interface_id, event_id, Bytes::from(content.clone()))
            .await?;

        // Broadcast locally
        let received = ReceivedEvent {
            interface_id: *interface_id,
            event: event.clone(),
        };
        let _ = state.event_tx.send(received);

        // Send encrypted message to connected peers
        if let Some(transport) = self.transport.read().await.as_ref() {
            if let Some(key) = self.interface_keys.get(interface_id) {
                // Serialize and encrypt
                let plaintext = postcard::to_allocvec(&event)
                    .map_err(|e| NodeError::Serialization(e.to_string()))?;
                let encrypted = key.encrypt(&plaintext)
                    .map_err(|e| NodeError::Crypto(e.to_string()))?;

                let msg = InterfaceEventMessage::new(
                    *interface_id,
                    encrypted.ciphertext,
                    event_id,
                    encrypted.nonce,
                );
                let network_msg = NetworkMessage::InterfaceEvent(msg);
                let bytes = network_msg.to_bytes()
                    .map_err(|e| NodeError::Serialization(e.to_string()))?;

                // Send to all members
                for member in interface.members() {
                    if member != self.identity && transport.is_connected(&member) {
                        let _ = transport.send(&member, bytes.clone()).await;
                    }
                }
            }
        }

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

    /// Get interface key (for testing/advanced use)
    pub fn interface_key(&self, interface_id: &InterfaceId) -> Option<InterfaceKey> {
        self.interface_keys.get(interface_id).map(|k| k.value().clone())
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
    async fn test_identity_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Create first node
        let config1 = NodeConfig::with_data_dir(temp_dir.path());
        let node1 = IndrasNode::new(config1).await.unwrap();
        let identity1 = *node1.identity();
        drop(node1);

        // Create second node in same directory
        let config2 = NodeConfig::with_data_dir(temp_dir.path());
        let node2 = IndrasNode::new(config2).await.unwrap();
        let identity2 = *node2.identity();

        // Should have same identity
        assert_eq!(identity1, identity2);
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

        // Should have an interface key
        assert!(node.interface_key(&interface_id).is_some());
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
            .with_bootstrap(vec![1, 2, 3])
            .with_key_invite(vec![4, 5, 6]);

        // Bytes roundtrip
        let bytes = invite.to_bytes().unwrap();
        let restored = InviteKey::from_bytes(&bytes).unwrap();
        assert_eq!(restored.interface_id, interface_id);
        assert_eq!(restored.bootstrap_peers.len(), 1);
        assert_eq!(restored.key_invite, Some(vec![4, 5, 6]));

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

    #[tokio::test]
    async fn test_interface_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Create node and interface
        let config1 = NodeConfig::with_data_dir(temp_dir.path());
        let node1 = IndrasNode::new(config1).await.unwrap();
        let (interface_id, _) = node1.create_interface(Some("Persistent")).await.unwrap();

        // Send a message
        node1.send_message(&interface_id, b"Test message".to_vec()).await.unwrap();
        drop(node1);

        // Create new node in same directory
        let config2 = NodeConfig::with_data_dir(temp_dir.path());
        let node2 = IndrasNode::new(config2).await.unwrap();

        // Start to trigger interface loading
        node2.start().await.unwrap();

        // Interface should be loaded
        assert!(node2.list_interfaces().contains(&interface_id));

        // Should have the interface key
        assert!(node2.interface_key(&interface_id).is_some());

        node2.stop().await.unwrap();
    }
}
