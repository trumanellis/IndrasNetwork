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
pub use keystore::{EncryptedKeystore, Keystore, StoryKeystore};
pub use message_handler::{
    EventAckMessage, InterfaceEventMessage, InterfaceSyncRequest, InterfaceSyncResponse,
    NetworkMessage, SIGNED_MESSAGE_VERSION, SignedNetworkMessage,
};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bytes::Bytes;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, warn};

use indras_core::transport::Transport;
use indras_core::{EventId, InterfaceEvent, InterfaceId, NInterfaceTrait, PeerIdentity};
use indras_crypto::{
    InterfaceKey, KeyDistribution, KeyInvite, PQEncapsulationKey, PQIdentity, PQKemKeyPair,
};
use indras_storage::CompositeStorage;
use indras_sync::NInterface;
use indras_transport::{IrohIdentity, IrohNetworkAdapter, PeerEvent};

use message_handler::MessageHandler;
use sync_task::SyncTask;

/// Default sync interval in seconds
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 5;

/// Invite key for joining an interface (post-quantum secure)
///
/// Contains the interface ID, bootstrap peer addresses, and post-quantum
/// key material for secure key distribution.
///
/// ## Size
///
/// - Key invite (ML-KEM): ~1,200 bytes
/// - Inviter's encapsulation key: ~1,184 bytes
/// - Inviter's verifying key: ~1,952 bytes
/// - Total overhead: ~4,400 bytes per invite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteKey {
    /// The interface to join
    pub interface_id: InterfaceId,
    /// Bootstrap peer addresses (serialized)
    pub bootstrap_peers: Vec<Vec<u8>>,
    /// ML-KEM encapsulated interface key (KeyInvite serialized)
    pub key_invite: Option<Vec<u8>>,
    /// Inviter's encapsulation key (for future key exchanges with them)
    pub inviter_encapsulation_key: Option<Vec<u8>>,
    /// Inviter's PQ verifying key (for signature verification)
    pub inviter_pq_verifying_key: Option<Vec<u8>>,
}

impl InviteKey {
    /// Create a new invite key
    pub fn new(interface_id: InterfaceId) -> Self {
        Self {
            interface_id,
            bootstrap_peers: Vec::new(),
            key_invite: None,
            inviter_encapsulation_key: None,
            inviter_pq_verifying_key: None,
        }
    }

    /// Add a bootstrap peer
    pub fn with_bootstrap(mut self, peer_addr: Vec<u8>) -> Self {
        self.bootstrap_peers.push(peer_addr);
        self
    }

    /// Add an ML-KEM encapsulated key invite
    pub fn with_key_invite(mut self, key_invite: Vec<u8>) -> Self {
        self.key_invite = Some(key_invite);
        self
    }

    /// Add inviter's encapsulation key (for future key exchanges)
    pub fn with_inviter_encapsulation_key(mut self, key: Vec<u8>) -> Self {
        self.inviter_encapsulation_key = Some(key);
        self
    }

    /// Add inviter's PQ verifying key (for signature verification)
    pub fn with_inviter_pq_verifying_key(mut self, key: Vec<u8>) -> Self {
        self.inviter_pq_verifying_key = Some(key);
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
///
/// ## Post-Quantum Security
///
/// The node uses post-quantum cryptography for application-layer security:
/// - ML-DSA-65 for digital signatures (message authentication)
/// - ML-KEM-768 for key encapsulation (key distribution)
///
/// Iroh's Ed25519 is used for transport-layer identity only.
pub struct IndrasNode {
    /// Node configuration
    config: NodeConfig,
    /// Our transport identity (iroh Ed25519)
    identity: IrohIdentity,
    /// Our iroh secret key (for transport layer)
    secret_key: iroh::SecretKey,
    /// Our post-quantum identity (ML-DSA-65 for signatures)
    pq_identity: PQIdentity,
    /// Our post-quantum KEM key pair (ML-KEM-768 for key encapsulation)
    pq_kem_keypair: PQKemKeyPair,
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

        // Load or generate all keys from keystore
        // Use encrypted keystore when passphrase is provided
        let (secret_key, pq_identity, pq_kem_keypair) = if let Some(ref passphrase) = config.passphrase {
            let mut keystore = EncryptedKeystore::new(&config.data_dir);
            keystore.unlock(passphrase)?;
            let sk = keystore.load_or_generate_iroh()?;
            let pq = keystore.load_or_generate_pq_identity()?;
            let kem = keystore.load_or_generate_pq_kem()?;
            (sk, pq, kem)
        } else {
            let keystore = Keystore::new(&config.data_dir);
            let sk = keystore.load_or_generate_iroh()?;
            let pq = keystore.load_or_generate_pq_identity()?;
            let kem = keystore.load_or_generate_pq_kem()?;
            (sk, pq, kem)
        };

        let identity = IrohIdentity::new(secret_key.public());

        let (shutdown_tx, _) = broadcast::channel(1);

        info!(
            identity = %identity.short_id(),
            pq_identity = %pq_identity.verifying_key().short_id(),
            pq_kem = %pq_kem_keypair.encapsulation_key().short_id(),
            "Node created with post-quantum keys"
        );

        Ok(Self {
            config,
            identity,
            secret_key,
            pq_identity,
            pq_kem_keypair,
            storage,
            interfaces: Arc::new(DashMap::new()),
            interface_keys: Arc::new(DashMap::new()),
            transport: RwLock::new(None),
            shutdown_tx,
            background_tasks: RwLock::new(Vec::new()),
            started: AtomicBool::new(false),
        })
    }

    /// Create a node with a specific iroh identity
    ///
    /// Use this when you have an existing iroh secret key.
    /// PQ keys will be generated if not present.
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

        // Save iroh key and load/generate PQ keys
        let (pq_identity, pq_kem_keypair) = if let Some(ref passphrase) = config.passphrase {
            let mut keystore = EncryptedKeystore::new(&config.data_dir);
            keystore.unlock(passphrase)?;
            keystore.save_iroh(&secret_key)?;
            let pq = keystore.load_or_generate_pq_identity()?;
            let kem = keystore.load_or_generate_pq_kem()?;
            (pq, kem)
        } else {
            let keystore = Keystore::new(&config.data_dir);
            keystore.save_iroh(&secret_key)?;
            let pq = keystore.load_or_generate_pq_identity()?;
            let kem = keystore.load_or_generate_pq_kem()?;
            (pq, kem)
        };

        let identity = IrohIdentity::new(secret_key.public());

        let (shutdown_tx, _) = broadcast::channel(1);

        info!(
            identity = %identity.short_id(),
            pq_identity = %pq_identity.verifying_key().short_id(),
            "Node created with existing iroh identity"
        );

        Ok(Self {
            config,
            identity,
            secret_key,
            pq_identity,
            pq_kem_keypair,
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
        let adapter =
            IrohNetworkAdapter::new(self.secret_key.clone(), self.config.transport.clone()).await?;
        adapter.start(vec![]).await?;
        let adapter = Arc::new(adapter);

        // Configure discovery service with our PQ keys for realm discovery
        let discovery = adapter.discovery_service();
        discovery
            .set_pq_keys(
                self.pq_kem_keypair.encapsulation_key_bytes(),
                self.pq_identity.verifying_key_bytes(),
            )
            .await;
        if let Some(name) = &self.config.display_name {
            discovery.set_display_name(name.clone()).await;
        }

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

        // Spawn message handler and sync task (both need PQ identity for signing)
        let pq_identity_arc = Arc::new(self.pq_identity.clone());
        let handler_task = MessageHandler::spawn(
            self.identity,
            self.interface_keys.clone(),
            self.interfaces.clone(),
            self.storage.clone(),
            adapter.clone(),
            pq_identity_arc.clone(),
            self.config.allow_legacy_unsigned,
            self.shutdown_tx.subscribe(),
            message_rx,
        );

        // Spawn sync task
        let sync_task = SyncTask::spawn(
            self.identity,
            pq_identity_arc,
            adapter.clone(),
            self.interface_keys.clone(),
            self.interfaces.clone(),
            self.storage.clone(),
            Duration::from_secs(DEFAULT_SYNC_INTERVAL_SECS),
            self.shutdown_tx.subscribe(),
        );

        // Spawn realm discovery event handler
        let realm_discovery_task = Self::spawn_realm_discovery_handler(
            self.identity,
            adapter.clone(),
            self.interfaces.clone(),
            self.storage.clone(),
            self.shutdown_tx.subscribe(),
        );

        // Store task handles
        {
            let mut tasks = self.background_tasks.write().await;
            tasks.push(receiver_task);
            tasks.push(handler_task);
            tasks.push(sync_task);
            tasks.push(realm_discovery_task);
        }

        info!("Node started");
        Ok(())
    }

    /// Spawn the realm discovery event handler task
    fn spawn_realm_discovery_handler(
        local_identity: IrohIdentity,
        transport: Arc<IrohNetworkAdapter>,
        interfaces: Arc<DashMap<InterfaceId, InterfaceState>>,
        storage: Arc<CompositeStorage<IrohIdentity>>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let discovery = transport.discovery_service();
            let mut events = discovery.subscribe();

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Realm discovery handler shutting down");
                        break;
                    }
                    Ok(event) = events.recv() => {
                        match event {
                            PeerEvent::RealmPeerJoined { interface_id, peer_info } => {
                                // Skip ourselves
                                if peer_info.peer_id == local_identity {
                                    continue;
                                }

                                // Check if we're in this interface
                                if let Some(state) = interfaces.get(&interface_id) {
                                    let mut interface = state.interface.write().await;

                                    // Add peer as member if not already
                                    if interface.add_member(peer_info.peer_id).is_ok() {
                                        info!(
                                            peer = %peer_info.peer_id.short_id(),
                                            realm = %hex::encode(&interface_id.as_bytes()[..8]),
                                            "Discovered new realm member via gossip"
                                        );

                                        // Persist to storage
                                        if let Err(e) = storage.register_peer(&peer_info.peer_id, peer_info.display_name.clone()) {
                                            warn!(error = %e, "Failed to register discovered peer");
                                        }
                                        if let Err(e) = storage.add_member(&interface_id, &peer_info.peer_id) {
                                            warn!(error = %e, "Failed to add discovered peer as member");
                                        }

                                        // Store PQ keys for future direct encrypted communication
                                        if peer_info.pq_encapsulation_key.is_some() || peer_info.pq_verifying_key.is_some() {
                                            if let Ok(Some(mut record)) = storage.peer_registry().get(&peer_info.peer_id) {
                                                record.pq_encapsulation_key = peer_info.pq_encapsulation_key.clone();
                                                record.pq_verifying_key = peer_info.pq_verifying_key.clone();
                                                if let Err(e) = storage.peer_registry().upsert(&peer_info.peer_id, &record) {
                                                    warn!(error = %e, "Failed to store PQ keys for discovered peer");
                                                }
                                            }
                                        }

                                        // Broadcast a PeerIntroduction so others learn about this peer
                                        if let Err(e) = discovery.broadcast_peer_introduction(interface_id, peer_info.clone()).await {
                                            debug!(error = %e, "Failed to broadcast peer introduction");
                                        }
                                    }
                                }
                            }
                            PeerEvent::RealmPeerLeft { interface_id, peer_id } => {
                                // Skip ourselves
                                if peer_id == local_identity {
                                    continue;
                                }

                                // Check if we're in this interface
                                if let Some(state) = interfaces.get(&interface_id) {
                                    let mut interface = state.interface.write().await;
                                    if interface.remove_member(&peer_id).is_ok() {
                                        info!(
                                            peer = %peer_id.short_id(),
                                            realm = %hex::encode(&interface_id.as_bytes()[..8]),
                                            "Realm member left"
                                        );
                                    }
                                }
                            }
                            PeerEvent::IntroductionRequested { interface_id, requester, known_peers } => {
                                // Skip our own requests
                                if requester == local_identity {
                                    continue;
                                }

                                // Respond with members we know (discovery service handles rate limiting)
                                if interfaces.contains_key(&interface_id) {
                                    if let Err(e) = discovery.send_introduction_response(
                                        interface_id,
                                        requester,
                                        &known_peers,
                                    ).await {
                                        debug!(error = %e, "Failed to send introduction response");
                                    }
                                }
                            }
                            _ => {
                                // Ignore other events (global presence)
                            }
                        }
                    }
                }
            }
        })
    }

    /// Load persisted interfaces from storage
    async fn load_persisted_interfaces(&self) -> NodeResult<()> {
        let interface_records = self
            .storage
            .interface_store()
            .all()
            .map_err(NodeError::Storage)?;

        for record in interface_records {
            let interface_id = InterfaceId::new(record.interface_id);

            // Skip if already loaded
            if self.interfaces.contains_key(&interface_id) {
                continue;
            }

            // Create NInterface with known ID
            let mut interface = NInterface::with_id(interface_id, self.identity);

            // Load members from storage
            let members = self
                .storage
                .interface_store()
                .get_members(&interface_id)
                .map_err(NodeError::Storage)?;

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

        // Leave all realm topics before shutdown
        if let Some(transport) = self.transport.read().await.as_ref() {
            let discovery = transport.discovery_service();
            for realm in discovery.active_realms() {
                if let Err(e) = discovery.leave_realm_topic(realm).await {
                    warn!(error = %e, "Failed to leave realm topic during shutdown");
                }
            }
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

    /// Get our iroh secret key
    pub fn secret_key(&self) -> &iroh::SecretKey {
        &self.secret_key
    }

    /// Get our post-quantum identity (ML-DSA-65)
    pub fn pq_identity(&self) -> &PQIdentity {
        &self.pq_identity
    }

    /// Get our post-quantum KEM key pair (ML-KEM-768)
    pub fn pq_kem_keypair(&self) -> &PQKemKeyPair {
        &self.pq_kem_keypair
    }

    /// Get our public encapsulation key (for others to send us keys)
    pub fn encapsulation_key(&self) -> PQEncapsulationKey {
        self.pq_kem_keypair.encapsulation_key()
    }

    /// Check if the node is started
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// Get the transport adapter (if started)
    pub async fn transport(&self) -> Option<Arc<IrohNetworkAdapter>> {
        self.transport.read().await.clone()
    }

    /// Connect to a peer using their serialized endpoint address.
    ///
    /// Establishes transport-level connectivity, enabling gossip discovery
    /// and CRDT sync for all shared interfaces. This is useful when you
    /// know a peer's address (e.g., from an invite code) but aren't
    /// joining a new interface.
    pub async fn connect_to_bootstrap(&self, bootstrap_bytes: &[u8]) -> NodeResult<()> {
        let addr: iroh::EndpointAddr = postcard::from_bytes(bootstrap_bytes)?;
        let guard = self.transport.read().await;
        let transport = guard.as_ref().ok_or(NodeError::NotStarted)?;
        transport
            .connection_manager()
            .connect(addr)
            .await
            .map_err(|e| NodeError::Transport(e.to_string()))?;
        Ok(())
    }

    /// Connect to a peer by their raw 32-byte public key.
    ///
    /// Creates an `EndpointAddr` from the key and connects via iroh relay.
    /// This establishes transport-level connectivity so that gossip messages
    /// and CRDT sync can flow between the two nodes.
    pub async fn connect_to_peer(&self, peer_key_bytes: &[u8; 32]) -> NodeResult<()> {
        let public_key = iroh::PublicKey::from_bytes(peer_key_bytes)
            .map_err(|e| NodeError::Crypto(e.to_string()))?;
        let guard = self.transport.read().await;
        let transport = guard.as_ref().ok_or(NodeError::NotStarted)?;
        transport
            .connection_manager()
            .connect_by_key(public_key)
            .await
            .map_err(|e| NodeError::Transport(e.to_string()))?;
        Ok(())
    }

    /// Get our endpoint address for sharing with peers
    pub async fn endpoint_addr(&self) -> Option<iroh::EndpointAddr> {
        self.transport
            .read()
            .await
            .as_ref()
            .map(|t| t.endpoint_addr())
    }

    /// Create a new interface
    ///
    /// Returns the interface ID and an invite key for sharing with peers.
    #[instrument(skip(self))]
    pub async fn create_interface(
        &self,
        name: Option<&str>,
    ) -> NodeResult<(InterfaceId, InviteKey)> {
        // Create NInterface with us as the creator
        let interface = NInterface::new(self.identity);
        let interface_id = interface.id();
        self.setup_interface(interface_id, interface, name, None, vec![]).await
    }

    /// Create a new interface with a specific ID (for deterministic peer-set realms).
    ///
    /// This is used internally when the interface ID is pre-computed (e.g., from a peer set hash).
    /// Returns the interface ID and an invite key for sharing with peers.
    #[instrument(skip(self))]
    pub async fn create_interface_with_id(
        &self,
        interface_id: InterfaceId,
        name: Option<&str>,
    ) -> NodeResult<(InterfaceId, InviteKey)> {
        // Check if we're already in this interface
        if self.interfaces.contains_key(&interface_id) {
            // Already exists, return existing invite
            let mut invite = InviteKey::new(interface_id)
                .with_inviter_encapsulation_key(self.pq_kem_keypair.encapsulation_key_bytes())
                .with_inviter_pq_verifying_key(self.pq_identity.verifying_key_bytes());

            if let Some(transport) = self.transport.read().await.as_ref() {
                let addr = transport.endpoint_addr();
                if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                    invite = invite.with_bootstrap(addr_bytes);
                }
            }

            return Ok((interface_id, invite));
        }

        // Create NInterface with the specified ID
        let interface = NInterface::with_id(interface_id, self.identity);
        self.setup_interface(interface_id, interface, name, None, vec![]).await
    }

    /// Create a new interface with a specific ID and a deterministic key seed.
    ///
    /// Instead of generating a random `InterfaceKey`, the key is derived from
    /// `key_seed` via `InterfaceKey::from_seed()`. Both peers who know the
    /// seed will independently derive the same key.
    ///
    /// `bootstrap_peers` are iroh `PublicKey`s to pass to the gossip discovery
    /// layer so that the two sides of a deterministic realm can find each other.
    /// Pass an empty vec when no peer is known (e.g. own inbox, home realm).
    ///
    /// **Idempotent:** If the interface already exists, returns the existing invite.
    #[instrument(skip(self, key_seed, bootstrap_peers))]
    pub async fn create_interface_with_seed(
        &self,
        interface_id: InterfaceId,
        key_seed: &[u8; 32],
        name: Option<&str>,
        bootstrap_peers: Vec<iroh::PublicKey>,
    ) -> NodeResult<(InterfaceId, InviteKey)> {
        // Check if we're already in this interface
        if self.interfaces.contains_key(&interface_id) {
            // Already exists, return existing invite
            let mut invite = InviteKey::new(interface_id)
                .with_inviter_encapsulation_key(self.pq_kem_keypair.encapsulation_key_bytes())
                .with_inviter_pq_verifying_key(self.pq_identity.verifying_key_bytes());

            if let Some(transport) = self.transport.read().await.as_ref() {
                let addr = transport.endpoint_addr();
                if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                    invite = invite.with_bootstrap(addr_bytes);
                }
            }

            return Ok((interface_id, invite));
        }

        // Create NInterface with the specified ID
        let interface = NInterface::with_id(interface_id, self.identity);
        self.setup_interface(interface_id, interface, name, Some(key_seed), bootstrap_peers)
            .await
    }

    /// Internal helper to set up an interface after creation.
    async fn setup_interface(
        &self,
        interface_id: InterfaceId,
        interface: NInterface<IrohIdentity>,
        name: Option<&str>,
        key_seed: Option<&[u8; 32]>,
        bootstrap_peers: Vec<iroh::PublicKey>,
    ) -> NodeResult<(InterfaceId, InviteKey)> {

        // Generate or derive interface encryption key
        let interface_key = match key_seed {
            Some(seed) => InterfaceKey::from_seed(seed, interface_id),
            None => InterfaceKey::generate(interface_id),
        };
        self.interface_keys
            .insert(interface_id, interface_key.clone());

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

        // Create invite with our endpoint address and PQ keys
        let mut invite = InviteKey::new(interface_id)
            .with_inviter_encapsulation_key(self.pq_kem_keypair.encapsulation_key_bytes())
            .with_inviter_pq_verifying_key(self.pq_identity.verifying_key_bytes());

        if let Some(transport) = self.transport.read().await.as_ref() {
            let addr = transport.endpoint_addr();
            // Serialize endpoint address using postcard
            if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                invite = invite.with_bootstrap(addr_bytes);
            }

            // Join the realm's gossip topic for peer discovery
            let discovery = transport.discovery_service();
            if let Err(e) = discovery.join_realm_topic(interface_id, bootstrap_peers).await {
                warn!(error = %e, "Failed to join realm gossip topic");
            }
        }

        info!(interface_id = %hex::encode(interface_id.as_bytes()), "Interface created");
        Ok((interface_id, invite))
    }

    /// Create an invite for a specific peer using ML-KEM
    ///
    /// Includes the ML-KEM encapsulated interface key for the invitee,
    /// plus our PQ keys for future communication.
    pub async fn create_invite_for(
        &self,
        interface_id: &InterfaceId,
        invitee_encapsulation_key: &PQEncapsulationKey,
    ) -> NodeResult<InviteKey> {
        // Get interface key
        let interface_key = self
            .interface_keys
            .get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        // Create ML-KEM encapsulated key invite
        let key_invite =
            KeyDistribution::create_invite(interface_key.value(), invitee_encapsulation_key)
                .map_err(|e| NodeError::Crypto(e.to_string()))?;

        let key_invite_bytes = key_invite
            .to_bytes()
            .map_err(|e| NodeError::Crypto(e.to_string()))?;

        // Build invite with PQ components
        let mut invite = InviteKey::new(*interface_id)
            .with_key_invite(key_invite_bytes)
            .with_inviter_encapsulation_key(self.pq_kem_keypair.encapsulation_key_bytes())
            .with_inviter_pq_verifying_key(self.pq_identity.verifying_key_bytes());

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

        // Decapsulate interface key if provided (using ML-KEM)
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

            // Decapsulate using our ML-KEM key pair
            let interface_key = KeyDistribution::accept_invite(&key_invite, &self.pq_kem_keypair)
                .map_err(|e| NodeError::Crypto(e.to_string()))?;

            self.interface_keys.insert(interface_id, interface_key);
        }

        // Create NInterface with known ID
        let interface = NInterface::with_id(interface_id, self.identity);

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
        let mut bootstrap_public_keys = Vec::new();
        if let Some(transport) = self.transport.read().await.as_ref() {
            for peer_bytes in &invite.bootstrap_peers {
                // Deserialize endpoint address using postcard
                if let Ok(addr) = postcard::from_bytes::<iroh::EndpointAddr>(peer_bytes) {
                    let peer_id = IrohIdentity::new(addr.id);
                    debug!(peer = %peer_id.short_id(), "Connecting to bootstrap peer");
                    bootstrap_public_keys.push(addr.id);
                    if let Err(e) = transport.connection_manager().connect(addr).await {
                        warn!(error = %e, "Failed to connect to bootstrap peer");
                    } else {
                        bootstrap_peer_ids.push(peer_id);
                    }
                }
            }

            // Join the realm's gossip topic for peer discovery
            // This broadcasts our InterfaceJoin with PQ keys and sends IntroductionRequest
            let discovery = transport.discovery_service();
            if let Err(e) = discovery
                .join_realm_topic(interface_id, bootstrap_public_keys)
                .await
            {
                warn!(error = %e, "Failed to join realm gossip topic");
            }

            // Send initial sync request ONLY to bootstrap peers we connected to (signed)
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

                    // Sign the message
                    if let Ok(msg_bytes) = msg.to_bytes() {
                        let signature = self.pq_identity.sign(&msg_bytes);
                        let signed_msg = SignedNetworkMessage {
                            version: SIGNED_MESSAGE_VERSION,
                            message: msg,
                            signature: signature.to_bytes().to_vec(),
                            sender_verifying_key: self.pq_identity.verifying_key_bytes(),
                        };
                        if let Ok(bytes) = signed_msg.to_bytes() {
                            let _ = transport.send(peer, bytes).await;
                        }
                    }
                }
            }
        }

        info!(interface_id = %hex::encode(interface_id.as_bytes()), "Joined interface");
        Ok(interface_id)
    }

    /// Leave an interface
    ///
    /// Broadcasts a leave message and cleans up all state for this interface.
    #[instrument(skip_all)]
    pub async fn leave_interface(&self, interface_id: &InterfaceId) -> NodeResult<()> {
        // Check if we're in this interface
        if !self.interfaces.contains_key(interface_id) {
            return Err(NodeError::InterfaceNotFound(hex::encode(
                interface_id.as_bytes(),
            )));
        }

        // Leave the realm's gossip topic (broadcasts leave message)
        if let Some(transport) = self.transport.read().await.as_ref() {
            let discovery = transport.discovery_service();
            if let Err(e) = discovery.leave_realm_topic(*interface_id).await {
                warn!(error = %e, "Failed to leave realm gossip topic");
            }
        }

        // Remove from memory
        self.interfaces.remove(interface_id);
        self.interface_keys.remove(interface_id);

        // Note: We don't remove from storage to allow rejoining later
        // The storage can be cleaned up separately if needed

        info!("Left interface");
        Ok(())
    }

    /// Send a message to an interface
    #[instrument(skip_all)]
    pub async fn send_message(
        &self,
        interface_id: &InterfaceId,
        content: Vec<u8>,
    ) -> NodeResult<EventId> {
        let state = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        // Create the event
        let mut interface = state.interface.write().await;
        let sequence = interface.event_count() as u64 + 1;
        let event = InterfaceEvent::message(self.identity, sequence, content.clone());

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

        // Send encrypted and signed message to connected peers
        if let Some(transport) = self.transport.read().await.as_ref()
            && let Some(key) = self.interface_keys.get(interface_id)
        {
            // Serialize and encrypt
            let plaintext = postcard::to_allocvec(&event)
                .map_err(|e| NodeError::Serialization(e.to_string()))?;
            let encrypted = key
                .encrypt(&plaintext)
                .map_err(|e| NodeError::Crypto(e.to_string()))?;

            let msg = InterfaceEventMessage::new(
                *interface_id,
                encrypted.ciphertext,
                event_id,
                encrypted.nonce,
            );
            let network_msg = NetworkMessage::InterfaceEvent(msg);

            // Sign the message with PQ identity
            let msg_bytes = network_msg
                .to_bytes()
                .map_err(|e| NodeError::Serialization(e.to_string()))?;
            let signature = self.pq_identity.sign(&msg_bytes);

            let signed_msg = SignedNetworkMessage {
                version: SIGNED_MESSAGE_VERSION,
                message: network_msg,
                signature: signature.to_bytes().to_vec(),
                sender_verifying_key: self.pq_identity.verifying_key_bytes(),
            };

            let bytes = signed_msg
                .to_bytes()
                .map_err(|e| NodeError::Serialization(e.to_string()))?;

            // Send to all members
            for member in interface.members() {
                if member != self.identity && transport.is_connected(&member) {
                    let _ = transport.send(&member, bytes.clone()).await;
                }
            }
        }

        debug!(event_id = ?event_id, "Message sent");
        Ok(event_id)
    }

    /// Subscribe to events from an interface
    ///
    /// Returns a broadcast receiver that will receive all events.
    pub fn events(
        &self,
        interface_id: &InterfaceId,
    ) -> NodeResult<broadcast::Receiver<ReceivedEvent>> {
        let state = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        Ok(state.event_tx.subscribe())
    }

    /// Get events since a sequence number
    pub async fn events_since(
        &self,
        interface_id: &InterfaceId,
        since: u64,
    ) -> NodeResult<Vec<InterfaceEvent<IrohIdentity>>> {
        let state = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        let interface = state.interface.read().await;
        Ok(interface.events_since(since))
    }

    /// Get all events from the Automerge document for an interface.
    ///
    /// Unlike `events_since()` which reads from the EventStore (local events only),
    /// this reads from the Automerge document which includes events received via
    /// CRDT sync from remote peers.
    pub async fn document_events(
        &self,
        interface_id: &InterfaceId,
    ) -> NodeResult<Vec<InterfaceEvent<IrohIdentity>>> {
        let state = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        let interface = state.interface.read().await;
        let doc = interface
            .document()
            .map_err(|e| NodeError::Sync(format!("Document lock: {}", e)))?;
        Ok(doc.events())
    }

    /// Get all members of an interface
    ///
    /// Returns members from both the CRDT state and discovered peers via gossip.
    pub async fn members(&self, interface_id: &InterfaceId) -> NodeResult<Vec<IrohIdentity>> {
        let state = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        let interface = state.interface.read().await;
        let mut members: Vec<IrohIdentity> = interface.members().into_iter().collect();

        // Also include peers discovered via gossip that may not be in CRDT yet
        if let Some(transport) = self.transport.read().await.as_ref() {
            let discovery = transport.discovery_service();
            for peer_info in discovery.realm_members(interface_id) {
                if !members.contains(&peer_info.peer_id) {
                    members.push(peer_info.peer_id);
                }
            }
        }

        Ok(members)
    }

    /// Get realm members with full info including PQ keys
    ///
    /// Returns detailed member info including PQ keys for secure communication.
    pub async fn members_with_info(
        &self,
        interface_id: &InterfaceId,
    ) -> NodeResult<Vec<indras_transport::RealmPeerInfo>> {
        if !self.interfaces.contains_key(interface_id) {
            return Err(NodeError::InterfaceNotFound(hex::encode(
                interface_id.as_bytes(),
            )));
        }

        if let Some(transport) = self.transport.read().await.as_ref() {
            Ok(transport.discovery_service().realm_members(interface_id))
        } else {
            Ok(Vec::new())
        }
    }

    /// Subscribe to realm peer discovery events.
    ///
    /// Returns a receiver for peer events (joins, leaves).
    /// Returns `None` if the node has not been started yet.
    pub async fn subscribe_peer_events(&self) -> Option<broadcast::Receiver<PeerEvent>> {
        let guard = self.transport.read().await;
        guard.as_ref().map(|t| t.discovery_service().subscribe())
    }

    /// Add a member to an interface
    pub async fn add_member(
        &self,
        interface_id: &InterfaceId,
        peer: IrohIdentity,
    ) -> NodeResult<()> {
        let state = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| NodeError::InterfaceNotFound(hex::encode(interface_id.as_bytes())))?;

        let mut interface = state.interface.write().await;
        interface
            .add_member(peer)
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
        self.interface_keys
            .get(interface_id)
            .map(|k| k.value().clone())
    }

    /// Set an interface key (used when accepting contact invites that include the key).
    pub fn set_interface_key(&self, interface_id: InterfaceId, key: InterfaceKey) {
        self.interface_keys.insert(interface_id, key);
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
        node.send_message(&interface_id, b"First".to_vec())
            .await
            .unwrap();
        node.send_message(&interface_id, b"Second".to_vec())
            .await
            .unwrap();
        node.send_message(&interface_id, b"Third".to_vec())
            .await
            .unwrap();

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
        node1
            .send_message(&interface_id, b"Test message".to_vec())
            .await
            .unwrap();
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
