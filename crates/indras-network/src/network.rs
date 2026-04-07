//! IndrasNetwork - the main SyncEngine entry point.
//!
//! Provides a high-level API for building P2P applications on Indra's Network.

use crate::config::{NetworkBuilder, NetworkConfig, Preset};
use crate::contacts::ContactsRealm;
use crate::direct_connect::{inbox_key_seed, inbox_realm_id, is_initiator, ConnectionNotify};
use crate::encounter;
use crate::error::{IndraError, Result};
use crate::home_realm::{home_key_seed, home_realm_id, HomeRealm};
use crate::artifact_sync::{artifact_interface_id, artifact_key_seed};
use crate::identity_code::IdentityCode;
use crate::invite::InviteCode;
use crate::member::{Member, MemberId};
use crate::realm::Realm;
use crate::artifact::{generate_tree_id, dm_story_id};

use dashmap::DashMap;
use indras_core::{InterfaceId, PeerIdentity};
use indras_node::{IndrasNode, ReceivedEvent};
use indras_storage::CompositeStorage;
use indras_transport::IrohIdentity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tokio::sync::{broadcast, watch, Mutex, Notify, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::peering::{PeerEvent, PeerInfo};

/// Unique identifier for a realm.
pub type RealmId = InterfaceId;

/// An event from any realm, tagged with the source realm ID.
///
/// Used by `IndrasNetwork::events()` to provide a global event stream
/// that aggregates events across all loaded realms.
#[derive(Debug, Clone)]
pub struct GlobalEvent {
    /// The realm this event originated from.
    pub realm_id: RealmId,
    /// The underlying event.
    pub event: ReceivedEvent,
}

/// Serializable identity backup containing all cryptographic keys.
///
/// Used by `export_identity()` and `import_identity()` for backing up
/// and restoring node identity across devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityBackup {
    /// Ed25519 transport key (iroh identity).
    pub iroh_key: Vec<u8>,
    /// ML-DSA-65 post-quantum signing identity.
    pub pq_identity: Vec<u8>,
    /// ML-KEM-768 post-quantum key exchange keypair.
    pub pq_kem: Vec<u8>,
}

/// User profile persisted to disk.
///
/// Stores user preferences that should survive across restarts
/// without needing to be re-entered.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UserProfile {
    /// Display name for this user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

/// Filename for the persisted user profile.
const PROFILE_FILENAME: &str = "profile.json";

/// The main entry point for the Indra SyncEngine.
///
/// `IndrasNetwork` manages your identity, realms, and network connections.
/// It wraps the underlying infrastructure and provides a simple, unified API.
///
/// # Example
///
/// ```ignore
/// use indras_network::prelude::*;
///
/// // Create a new network instance
/// let network = IndrasNetwork::new("~/.myapp").await?;
///
/// // Create a realm for collaboration
/// let realm = network.create_realm("My Project").await?;
///
/// // Share the invite code with others
/// println!("Invite: {}", realm.invite_code());
///
/// // Send a message
/// realm.send("Hello, world!").await?;
/// ```
pub struct IndrasNetwork {
    /// The underlying node.
    inner: Arc<IndrasNode>,
    /// Cached realm wrappers.
    realms: Arc<DashMap<RealmId, RealmState>>,
    /// Mapping from peer sets to realm IDs (for peer-based realm lookup).
    peer_realms: Arc<DashMap<Vec<MemberId>, RealmId>>,
    /// The contacts realm (lazily initialized).
    contacts_realm: Arc<RwLock<Option<ContactsRealm>>>,
    /// The home realm (lazily initialized).
    home_realm: RwLock<Option<HomeRealm>>,
    /// Configuration.
    config: NetworkConfig,
    /// Runtime display name override (set via `set_display_name`).
    display_name_override: std::sync::RwLock<Option<String>>,
    /// Our identity.
    identity: Member,

    // ── Peering state ────────────────────────────────────────────
    /// Watch channel sender for the current peer list.
    peers_tx: watch::Sender<Vec<PeerInfo>>,
    /// Watch channel receiver for the current peer list.
    peers_rx: watch::Receiver<Vec<PeerInfo>>,
    /// Broadcast channel for peer events.
    peer_event_tx: broadcast::Sender<PeerEvent>,
    /// Cancellation token for peering background tasks.
    peering_cancel: CancellationToken,
    /// Handles for peering background tasks (joined on stop).
    peering_tasks: Mutex<Vec<JoinHandle<()>>>,
    /// Notify to trigger an immediate contact poll cycle.
    poll_notify: Arc<Notify>,
    /// Shared relay blob endpoint for vault sync (lazily initialized).
    /// Uses a fresh transport key so it can connect to any relay including
    /// nodes running in the same process.
    relay_blob_endpoint: OnceCell<(
        indras_transport::relay_client::RelayClient,
        iroh::Endpoint,
    )>,
    /// Guard against double-shutdown of peering tasks.
    shutdown_called: AtomicBool,
    /// Time-throttled map of peers we've re-notified (avoids spam).
    /// Value is the last re-notification attempt time.
    re_notified_peers: Arc<DashMap<MemberId, std::time::Instant>>,
}

/// Internal realm state.
struct RealmState {
    name: Option<String>,
    artifact_id: Option<crate::artifact::ArtifactId>,
    /// Shared chat document handle — all Realm instances for the same realm
    /// share this OnceCell so they use the same broadcast channel.
    chat_doc: Arc<OnceCell<crate::document::Document<crate::chat_message::RealmChatDocument>>>,
}

impl IndrasNetwork {
    /// Create a new network instance with default configuration.
    ///
    /// # Arguments
    ///
    /// * `data_dir` - Directory for persistent storage (keys, messages, etc.)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let network = IndrasNetwork::new("~/.myapp").await?;
    /// ```
    pub async fn new(data_dir: impl AsRef<Path>) -> Result<Arc<Self>> {
        let config = NetworkConfig::new(data_dir.as_ref());
        Self::with_config(config).await
    }

    /// Create a new network instance with a preset configuration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let network = IndrasNetwork::preset(Preset::Chat)
    ///     .data_dir("~/.mychat")
    ///     .build()
    ///     .await?;
    /// ```
    pub fn preset(preset: Preset) -> NetworkBuilder {
        NetworkBuilder::with_preset(preset)
    }

    /// Create a builder for custom configuration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let network = IndrasNetwork::builder()
    ///     .data_dir("~/.myapp")
    ///     .display_name("Alice")
    ///     .enforce_pq_signatures()
    ///     .build()
    ///     .await?;
    /// ```
    pub fn builder() -> NetworkBuilder {
        NetworkBuilder::new()
    }

    /// Create a network instance with the given configuration.
    pub async fn with_config(mut config: NetworkConfig) -> Result<Arc<Self>> {
        // Load persisted profile (display name, etc.) if it exists
        if config.display_name.is_none() {
            if let Some(profile) = Self::load_profile(&config.data_dir) {
                if profile.display_name.is_some() {
                    config.display_name = profile.display_name;
                }
            }
        }

        // Persist display name if one was provided
        if config.display_name.is_some() {
            let profile = UserProfile {
                display_name: config.display_name.clone(),
            };
            // Best-effort persistence — don't fail network creation if this fails
            let _ = Self::save_profile(&config.data_dir, &profile);
        }

        let node_config = config.to_node_config();
        let node = IndrasNode::new(node_config).await?;

        let identity = Member::new(*node.identity());

        let (peers_tx, peers_rx) = watch::channel(Vec::new());
        let (peer_event_tx, _) = broadcast::channel(256);

        Ok(Arc::new(Self {
            inner: Arc::new(node),
            realms: Arc::new(DashMap::new()),
            peer_realms: Arc::new(DashMap::new()),
            contacts_realm: Arc::new(RwLock::new(None)),
            home_realm: RwLock::new(None),
            config,
            display_name_override: std::sync::RwLock::new(None),
            identity,
            peers_tx,
            peers_rx,
            peer_event_tx,
            peering_cancel: CancellationToken::new(),
            peering_tasks: Mutex::new(Vec::new()),
            poll_notify: Arc::new(Notify::new()),
            relay_blob_endpoint: OnceCell::new(),
            shutdown_called: AtomicBool::new(false),
            re_notified_peers: Arc::new(DashMap::new()),
        }))
    }

    // ============================================================
    // Identity
    // ============================================================

    /// Get the unique identifier for this network instance.
    pub fn id(&self) -> MemberId {
        self.identity.id()
    }

    /// Get our identity as a Member.
    pub fn identity(&self) -> &Member {
        &self.identity
    }

    /// Get the display name for this network instance.
    pub fn display_name(&self) -> Option<String> {
        if let Ok(guard) = self.display_name_override.read() {
            if let Some(ref name) = *guard {
                return Some(name.clone());
            }
        }
        self.config.display_name.clone()
    }

    /// Set the display name for this network instance.
    pub async fn set_display_name(&self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        if let Ok(mut guard) = self.display_name_override.write() {
            *guard = Some(name.clone());
        }

        // Persist to disk
        let profile = UserProfile {
            display_name: Some(name.clone()),
        };
        Self::save_profile(&self.config.data_dir, &profile)?;

        // Broadcast to peers via discovery service if running
        if let Some(transport) = self.inner.transport().await {
            transport.discovery_service().set_display_name(name).await;
        }

        Ok(())
    }

    /// Check if the given data directory contains an existing identity.
    ///
    /// Returns `true` if no identity keys exist yet (first run).
    /// Returns `false` if keys already exist (returning user).
    ///
    /// Use this to decide whether to show a setup/genesis flow in your app.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if IndrasNetwork::is_first_run("~/.myapp") {
    ///     // Show setup screen, collect display name and passphrase
    ///     let network = IndrasNetwork::builder()
    ///         .data_dir("~/.myapp")
    ///         .display_name("Zephyr")
    ///         .passphrase("secure-passphrase")
    ///         .build()
    ///         .await?;
    /// } else {
    ///     // Returning user - just open
    ///     let network = IndrasNetwork::new("~/.myapp").await?;
    /// }
    /// ```
    pub fn is_first_run(data_dir: impl AsRef<Path>) -> bool {
        let keystore = indras_node::Keystore::new(data_dir.as_ref());
        !keystore.exists()
    }

    /// Load the user profile from disk, if it exists.
    fn load_profile(data_dir: &Path) -> Option<UserProfile> {
        let profile_path = data_dir.join(PROFILE_FILENAME);
        if profile_path.exists() {
            match std::fs::read_to_string(&profile_path) {
                Ok(json) => serde_json::from_str(&json).ok(),
                Err(_) => None,
            }
        } else {
            None
        }
    }

    /// Save the user profile to disk.
    fn save_profile(data_dir: &Path, profile: &UserProfile) -> Result<()> {
        let profile_path = data_dir.join(PROFILE_FILENAME);
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| IndraError::InvalidOperation(format!("Failed to serialize profile: {}", e)))?;
        std::fs::write(&profile_path, json)
            .map_err(|e| IndraError::InvalidOperation(format!("Failed to write profile: {}", e)))?;
        Ok(())
    }

    // ============================================================
    // Lifecycle
    // ============================================================

    /// Start the network and peering lifecycle.
    ///
    /// This begins accepting connections, synchronizing with peers,
    /// and spawns background tasks for contact polling, event forwarding,
    /// and periodic world-view saves.
    ///
    /// Must be called before creating or joining realms.
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        // IndrasNode::start is idempotent, so this is safe to call multiple times
        self.inner.start().await?;

        // Join our inbox realm to receive connection notifications
        self.join_inbox().await?;

        // Restore realms from persisted node interfaces.
        // The node already loaded them in load_persisted_interfaces(),
        // but IndrasNetwork.realms needs to be populated too.
        self.restore_realms().await;

        // Join contacts realm (idempotent)
        self.join_contacts_realm().await?;

        // Spawn peering background tasks
        let h1 = crate::peering::tasks::spawn_contact_poller(
            Arc::clone(self),
            self.peers_tx.clone(),
            self.peer_event_tx.clone(),
            self.peering_cancel.clone(),
            self.config.poll_interval,
            Arc::clone(&self.poll_notify),
        );
        let h2 = crate::peering::tasks::spawn_event_forwarder(
            Arc::clone(self),
            self.peer_event_tx.clone(),
            self.peering_cancel.clone(),
        );
        let h3 = crate::peering::tasks::spawn_periodic_saver(
            Arc::clone(self),
            self.peer_event_tx.clone(),
            self.peering_cancel.clone(),
            self.config.save_interval,
        );
        let h4 = crate::peering::tasks::spawn_task_supervisor(
            self.peer_event_tx.clone(),
            self.peering_cancel.clone(),
        );

        // Spawn inbox gossip listener: detect peers joining our inbox via gossip
        // discovery and auto-connect (creates DM realm + adds contact).
        // This is the primary connection discovery mechanism — it works through
        // iroh relay even without QUIC transport connections, unlike send_message
        // which requires established QUIC connections.
        let h5 = if let Some(peer_events) = self.inner.subscribe_peer_events().await {
            let my_id = self.id();
            let inbox_id = inbox_realm_id(my_id);
            let network_weak = Arc::downgrade(self);
            let cancel = self.peering_cancel.clone();
            Some(tokio::spawn(async move {
                Self::inbox_gossip_listener(my_id, inbox_id, peer_events, network_weak, cancel).await;
            }))
        } else {
            None
        };

        let mut handles = self.peering_tasks.lock().await;
        handles.extend([h1, h2, h3, h4]);
        if let Some(h5) = h5 {
            handles.push(h5);
        }

        Ok(())
    }

    /// Background task that detects peers joining our inbox via gossip discovery.
    ///
    /// When a peer joins our inbox gossip topic (by calling `create_interface_with_seed`
    /// with our inbox ID), iroh's gossip layer detects them even without QUIC connections.
    /// We then auto-call `connect()` to create the DM realm and add them as a contact.
    ///
    /// This is more reliable than the `send_message`-based notification because:
    /// - Gossip discovery works through the iroh relay (no QUIC needed)
    /// - `send_message` only delivers to peers with active QUIC connections
    async fn inbox_gossip_listener(
        my_id: MemberId,
        inbox_id: RealmId,
        mut peer_events: tokio::sync::broadcast::Receiver<indras_transport::PeerEvent>,
        network_weak: std::sync::Weak<IndrasNetwork>,
        cancel: CancellationToken,
    ) {
        use indras_core::identity::PeerIdentity;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::debug!("Inbox gossip listener shutting down");
                    break;
                }
                result = peer_events.recv() => {
                    match result {
                        Ok(indras_transport::PeerEvent::RealmPeerJoined { interface_id, peer_info }) => {
                            // Only care about our inbox realm
                            if interface_id != inbox_id {
                                continue;
                            }

                            // Extract peer's MemberId from their IrohIdentity
                            let peer_key_bytes = peer_info.peer_id.as_bytes();
                            let mut peer_member_id = [0u8; 32];
                            let len = peer_key_bytes.len().min(32);
                            peer_member_id[..len].copy_from_slice(&peer_key_bytes[..len]);

                            // Skip ourselves
                            if peer_member_id == my_id {
                                continue;
                            }

                            // Upgrade weak reference
                            let Some(network) = network_weak.upgrade() else {
                                tracing::debug!("Inbox gossip: network dropped, stopping");
                                break;
                            };

                            let peer_name = peer_info.display_name.clone();
                            tracing::info!(
                                peer = %hex::encode(&peer_member_id[..8]),
                                name = ?peer_name,
                                "Inbox: peer discovered via gossip, auto-connecting"
                            );

                            // Auto-connect: creates DM realm + adds to contacts
                            match network.connect(peer_member_id).await {
                                Ok(_) => {
                                    // Store the display name from gossip discovery
                                    if let Some(name) = peer_name {
                                        if let Ok(contacts) = network.join_contacts_realm().await {
                                            let _ = contacts.add_contact_with_name(
                                                peer_member_id, Some(name)
                                            ).await;
                                        }
                                    }
                                    tracing::info!(
                                        peer = %hex::encode(&peer_member_id[..8]),
                                        "Inbox: auto-connected via gossip discovery"
                                    );
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        peer = %hex::encode(&peer_member_id[..8]),
                                        error = %e,
                                        "Inbox: gossip-triggered auto-connect failed"
                                    );
                                }
                            }
                        }
                        Ok(_) => {} // Ignore other peer events
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!(missed = n, "Inbox gossip listener lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::debug!("Inbox gossip channel closed");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Populate `self.realms` and `self.peer_realms` from persisted node interfaces.
    ///
    /// Called during `start()` to restore DM and group realms that were created
    /// in previous sessions. Without this, the sidebar shows empty after restart.
    async fn restore_realms(&self) {
        let my_id = self.id();
        let home_id = home_realm_id(my_id);
        let inbox_id = inbox_realm_id(my_id);

        for interface_id in self.inner.list_interfaces() {
            // Skip home and inbox (managed separately)
            if interface_id == home_id || interface_id == inbox_id {
                continue;
            }

            // Skip if already tracked
            if self.realms.contains_key(&interface_id) {
                continue;
            }

            // Get the interface name from storage
            let name = self.inner.storage()
                .interface_store()
                .all()
                .ok()
                .and_then(|records| {
                    records.into_iter()
                        .find(|r| r.interface_id == *interface_id.as_bytes())
                        .and_then(|r| r.name)
                });

            // Insert into realms
            self.realms.insert(
                interface_id,
                RealmState {
                    name: name.clone(),
                    artifact_id: None,
                    chat_doc: Arc::new(OnceCell::new()),
                },
            );

            // For DM realms, restore peer_realms mapping from interface members
            if name.as_deref() == Some("DM") {
                if let Ok(members) = self.inner.members(&interface_id).await {
                    let mut peers: Vec<MemberId> = members
                        .into_iter()
                        .map(|id| {
                            let id_bytes = id.as_bytes();
                            let mut bytes = [0u8; 32];
                            bytes.copy_from_slice(&id_bytes[..32.min(id_bytes.len())]);
                            bytes
                        })
                        .collect();
                    peers.sort();
                    self.peer_realms.insert(peers, interface_id);
                }
            }

            tracing::debug!(
                realm = %hex::encode(&interface_id.as_bytes()[..8]),
                name = ?name,
                "Restored realm from persisted interface"
            );
        }
    }

    /// Join our own inbox realm and spawn a listener for connection notifications.
    ///
    /// The inbox realm is a deterministic gossip topic derived from our MemberId.
    /// When another peer calls `connect(our_id)`, they send a `ConnectionNotify`
    /// message to our inbox. The listener auto-reciprocates by calling `connect()`.
    async fn join_inbox(&self) -> Result<()> {
        let my_id = self.id();
        let realm_id = inbox_realm_id(my_id);

        // Check if already joined (idempotent)
        if self.realms.contains_key(&realm_id) {
            return Ok(());
        }

        // Create the inbox interface with deterministic ID and seed
        // No bootstrap peers needed — this is our own inbox.
        let seed = inbox_key_seed(&my_id);
        let (_interface_id, _invite_key) = self
            .inner
            .create_interface_with_seed(realm_id, &seed, Some("Inbox"), vec![])
            .await?;

        // Cache the realm state
        self.realms.insert(
            realm_id,
            RealmState {
                name: Some("Inbox".to_string()),
                artifact_id: None,
                chat_doc: Arc::new(OnceCell::new()),
            },
        );

        // Subscribe to events on our inbox realm.
        // Use Weak references so the listener doesn't prevent node cleanup on drop.
        let event_rx = self.inner.events(&realm_id)?;
        let inner_weak = Arc::downgrade(&self.inner);
        let realms = Arc::clone(&self.realms);
        let peer_realms = Arc::clone(&self.peer_realms);
        let contacts_realm = Arc::clone(&self.contacts_realm);

        tokio::spawn(async move {
            Self::inbox_listener(my_id, event_rx, inner_weak, realms, peer_realms, contacts_realm).await;
        });

        tracing::info!(
            inbox = %hex::encode(&realm_id.as_bytes()[..8]),
            "Joined inbox realm"
        );

        Ok(())
    }

    /// Background task that listens for ConnectionNotify messages on our inbox.
    ///
    /// Creates DM realms, caches peer mappings, adds contacts, and sends
    /// reciprocal notifications. Uses `Weak<IndrasNode>` so this task doesn't
    /// prevent node cleanup on drop.
    async fn inbox_listener(
        my_id: MemberId,
        mut event_rx: tokio::sync::broadcast::Receiver<indras_node::ReceivedEvent>,
        inner_weak: std::sync::Weak<IndrasNode>,
        realms: Arc<DashMap<RealmId, RealmState>>,
        peer_realms: Arc<DashMap<Vec<MemberId>, RealmId>>,
        contacts_realm: Arc<RwLock<Option<ContactsRealm>>>,
    ) {
        use indras_core::InterfaceEvent;

        loop {
            match event_rx.recv().await {
                Ok(received) => {
                    // Extract the payload from the event
                    let payload = match &received.event {
                        InterfaceEvent::Message { content, .. } => content.clone(),
                        _ => continue,
                    };

                    // Try to deserialize as ConnectionNotify
                    let notify = match ConnectionNotify::from_bytes(&payload) {
                        Ok(n) => n,
                        Err(_) => continue, // Not a connection notify, skip
                    };

                    // Don't process notifications from ourselves
                    if notify.sender_id == my_id {
                        continue;
                    }

                    // Upgrade the weak reference — if the node is gone, exit
                    let inner = match inner_weak.upgrade() {
                        Some(arc) => arc,
                        None => {
                            tracing::debug!("Inbox: node dropped, listener stopping");
                            break;
                        }
                    };

                    let peer_id = notify.sender_id;
                    let dm_artifact_id = dm_story_id(my_id, peer_id);
                    let dm_realm_id = crate::artifact_sync::artifact_interface_id(&dm_artifact_id);

                    // Check if we already have this DM realm (idempotent)
                    if realms.contains_key(&dm_realm_id) {
                        // DM realm already exists (restored from previous session),
                        // but still ensure the peer is in our contacts
                        if let Some(contacts) = contacts_realm.read().await.as_ref() {
                            if !contacts.is_contact(&peer_id).await {
                                let _ = contacts.add_contact_with_name(
                                    peer_id, notify.display_name.clone()
                                ).await;
                            }
                            let _ = contacts.confirm_contact(&peer_id).await;
                        }
                        tracing::debug!(
                            peer = %hex::encode(&peer_id[..8]),
                            "Inbox: DM realm already exists, ensured contact"
                        );
                        continue;
                    }

                    tracing::info!(
                        peer = %hex::encode(&peer_id[..8]),
                        name = ?notify.display_name,
                        "Inbox: received connection notification, reciprocating"
                    );

                    // Convert peer MemberId → PublicKey for transport + bootstrap
                    let peer_public_key = match iroh::PublicKey::from_bytes(&peer_id) {
                        Ok(pk) => pk,
                        Err(e) => {
                            tracing::warn!(error = %e, "Inbox: invalid peer key, skipping");
                            continue;
                        }
                    };

                    // Establish transport connectivity to peer
                    if notify.endpoint_addr.is_some() {
                        tracing::debug!(
                            peer = %hex::encode(&peer_id[..8]),
                            "Inbox: notification includes endpoint address"
                        );
                    }
                    if let Err(e) = inner.connect_to_peer(&peer_id).await {
                        tracing::debug!(
                            peer = %hex::encode(&peer_id[..8]),
                            error = %e,
                            "Inbox: transport connect failed (may still work via gossip)"
                        );
                    }

                    // Create the DM realm (reciprocate the connection) with deterministic key + bootstrap
                    let dm_seed = crate::artifact_sync::artifact_key_seed(&dm_artifact_id);
                    match inner
                        .create_interface_with_seed(dm_realm_id, &dm_seed, Some("DM"), vec![peer_public_key])
                        .await
                    {
                        Ok((interface_id, _invite_key)) => {
                            // Add peer as member so send_message can reach them
                            let peer_identity = IrohIdentity::from(peer_public_key);
                            let _ = inner.add_member(&dm_realm_id, peer_identity).await;

                            // Cache realm state
                            realms.insert(
                                interface_id,
                                RealmState {
                                    name: Some("DM".to_string()),
                                    artifact_id: Some(dm_artifact_id),
                                    chat_doc: Arc::new(OnceCell::new()),
                                },
                            );

                            // Cache peer mapping
                            let mut peers = vec![my_id, peer_id];
                            peers.sort();
                            peer_realms.insert(peers, interface_id);

                            // Add peer as contact so UI shows the connection
                            if let Some(contacts) = contacts_realm.read().await.as_ref() {
                                if !contacts.is_contact(&peer_id).await {
                                    let _ = contacts.add_contact_with_name(peer_id, notify.display_name.clone()).await;
                                }
                                let _ = contacts.confirm_contact(&peer_id).await;
                            }

                            tracing::info!(
                                peer = %hex::encode(&peer_id[..8]),
                                realm = %hex::encode(&dm_realm_id.as_bytes()[..8]),
                                "Inbox: reciprocated connection successfully"
                            );

                            // Send reciprocal notification to sender's inbox
                            let sender_inbox_id = inbox_realm_id(peer_id);
                            let sender_inbox_seed = inbox_key_seed(&peer_id);
                            if let Ok(_) = inner
                                .create_interface_with_seed(sender_inbox_id, &sender_inbox_seed, Some("PeerInbox"), vec![peer_public_key])
                                .await
                            {
                                // Add peer as member so send_message delivers to them
                                let _ = inner.add_member(&sender_inbox_id, peer_identity).await;

                                let mut reply = ConnectionNotify::new(my_id, dm_realm_id);
                                // Include our endpoint address in reciprocal notification
                                if let Some(addr) = inner.endpoint_addr().await {
                                    if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                                        reply = reply.with_endpoint_addr(addr_bytes);
                                    }
                                }
                                if let Ok(payload) = reply.to_bytes() {
                                    let _ = inner.send_message(&sender_inbox_id, payload).await;
                                }
                                // Cleanup: leave peer inbox after short delay
                                let inner_cleanup = inner.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                    let _ = inner_cleanup.leave_interface(&sender_inbox_id).await;
                                });
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                peer = %hex::encode(&peer_id[..8]),
                                error = %e,
                                "Inbox: failed to reciprocate connection"
                            );
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(missed = n, "Inbox listener lagged, some notifications may be missed");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("Inbox event channel closed, listener stopping");
                    break;
                }
            }
        }
    }

    /// Stop the network.
    ///
    /// Gracefully disconnects from peers and stops background tasks.
    pub async fn stop(&self) -> Result<()> {
        // Guard against double-shutdown of peering tasks
        if !self.shutdown_called.swap(true, Ordering::SeqCst) {
            // Cancel all peering background tasks
            self.peering_cancel.cancel();

            // Wait for all background tasks to finish
            let mut handles = self.peering_tasks.lock().await;
            for handle in handles.drain(..) {
                let _ = handle.await;
            }
            drop(handles);

            // Best-effort save world view
            if let Err(e) = self.save_world_view().await {
                tracing::warn!(error = %e, "failed to save world view on shutdown");
            }
        }

        self.inner.stop().await?;
        Ok(())
    }

    /// Check if the network is running.
    pub fn is_running(&self) -> bool {
        self.inner.is_started()
    }

    // ============================================================
    // Realm operations
    // ============================================================

    /// Create a new realm.
    ///
    /// Creates a new collaborative space that others can join via invite code.
    ///
    /// # Arguments
    ///
    /// * `name` - Human-readable name for the realm
    ///
    /// # Example
    ///
    /// ```ignore
    /// let realm = network.create_realm("Project Alpha").await?;
    /// println!("Invite code: {}", realm.invite_code());
    /// ```
    pub async fn create_realm(&self, name: &str) -> Result<Realm> {
        // Ensure network is started
        if !self.is_running() {
            self.inner.start().await?;
        }

        // Generate random Tree artifact ID
        let artifact_id = generate_tree_id();

        // Derive interface ID from artifact ID
        let interface_id = artifact_interface_id(&artifact_id);

        // Derive key seed from artifact ID
        let key_seed = artifact_key_seed(&artifact_id);

        // Create interface with deterministic ID and key
        let (_interface_id, invite_key) = self.inner.create_interface_with_seed(
            interface_id,
            &key_seed,
            Some(name),
            vec![]
        ).await?;

        // Register artifact in HomeRealm
        if let Ok(home) = self.home_realm().await {
            if let Err(e) = home.ensure_realm_artifact(&artifact_id, name).await {
                tracing::debug!(error = %e, "Failed to register realm artifact (non-fatal)");
            }
        }

        // Cache the realm state
        self.realms.insert(
            interface_id,
            RealmState {
                name: Some(name.to_string()),
                artifact_id: Some(artifact_id),
                chat_doc: Arc::new(OnceCell::new()),
            },
        );

        // Create invite WITH artifact_id
        let invite_code = InviteCode::new_with_artifact(invite_key, artifact_id);

        Ok(Realm::new(
            interface_id,
            Some(name.to_string()),
            Some(artifact_id),
            invite_code,
            Arc::clone(&self.inner),
        ))
    }

    /// Join an existing realm using an invite code.
    ///
    /// # Arguments
    ///
    /// * `invite` - The invite code (can be a string or InviteCode)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let realm = network.join("indra:realm:abc123...").await?;
    /// ```
    pub async fn join(&self, invite: impl AsRef<str>) -> Result<Realm> {
        // Ensure network is started
        if !self.is_running() {
            self.inner.start().await?;
        }

        let invite_code = InviteCode::parse(invite.as_ref())?;
        let interface_id = self.inner.join_interface(invite_code.invite_key().clone()).await?;

        // Derive interface encryption key from artifact seed.
        // Realm invites use deterministic keys derived from the artifact ID
        // (not ML-KEM key exchange, which requires knowing the invitee's key).
        // Without this, the joiner cannot encrypt outbound or decrypt inbound
        // messages, breaking real-time delivery (only periodic sync works).
        if let Some(artifact_id) = invite_code.artifact_id() {
            let key_seed = artifact_key_seed(artifact_id);
            let interface_key = indras_crypto::InterfaceKey::from_seed(&key_seed, interface_id);
            self.inner.set_interface_key(interface_id, interface_key.clone());

            // Persist key to storage so it survives restart
            if let Ok(Some(mut record)) = self.inner.storage().interface_store().get(&interface_id) {
                record.encrypted = true;
                record.encrypted_key = Some(interface_key.as_bytes().to_vec());
                let _ = self.inner.storage().interface_store().upsert(&record);
            }
        }

        // If the invite carries an artifact ID, register it in our HomeRealm
        if let Some(artifact_id) = invite_code.artifact_id() {
            if let Ok(home) = self.home_realm().await {
                if let Err(e) = home.ensure_realm_artifact(artifact_id, "Realm").await {
                    tracing::debug!(error = %e, "Failed to register realm artifact (non-fatal)");
                }
            }
        }

        // Cache the realm state
        self.realms.insert(interface_id, RealmState { name: None, artifact_id: invite_code.artifact_id().cloned(), chat_doc: Arc::new(OnceCell::new()) });

        Ok(Realm::new(
            interface_id,
            None,
            invite_code.artifact_id().cloned(),
            invite_code,
            Arc::clone(&self.inner),
        ))
    }

    /// Get a realm by ID.
    ///
    /// Returns None if the realm is not loaded.
    pub fn get_realm_by_id(&self, id: &RealmId) -> Option<Realm> {
        self.realms.get(id).map(|state| {
            Realm::from_id_with_chat_doc(
                *id,
                state.name.clone(),
                state.artifact_id.clone(),
                Arc::clone(&self.inner),
                Arc::clone(&state.chat_doc),
            )
        })
    }

    /// List all loaded realms.
    pub fn realms(&self) -> Vec<RealmId> {
        self.realms.iter().map(|r| *r.key()).collect()
    }

    /// List only conversation realms (DMs + shared realms).
    ///
    /// Filters out infrastructure realms (Home, Inbox) that should not
    /// appear in the chat sidebar.
    pub fn conversation_realms(&self) -> Vec<RealmId> {
        let my_id = self.id();
        let home_id = home_realm_id(my_id);
        let inbox_id = inbox_realm_id(my_id);

        self.realms
            .iter()
            .filter(|r| {
                let id = *r.key();
                if id == home_id || id == inbox_id {
                    return false;
                }
                // Filter out internal realms (peer inboxes, artifact-sync)
                match r.value().name.as_deref() {
                    Some("PeerInbox") | Some("artifact-sync") => false,
                    _ => true,
                }
            })
            .map(|r| *r.key())
            .collect()
    }

    /// Get the DM peer for a realm, if it's a DM realm.
    ///
    /// Searches the peer-realm mapping for an entry matching this realm ID
    /// and returns the other member (not self).
    pub fn dm_peer_for_realm(&self, realm_id: &RealmId) -> Option<MemberId> {
        let my_id = self.id();
        self.peer_realms
            .iter()
            .find(|entry| entry.value() == realm_id)
            .and_then(|entry| {
                entry.key().iter().find(|id| **id != my_id).copied()
            })
    }

    /// Leave a realm.
    ///
    /// Removes the realm from local state and disconnects from peers.
    /// The realm data is retained in storage, allowing you to rejoin later
    /// if you still have the invite code.
    ///
    /// # Arguments
    ///
    /// * `id` - The realm ID to leave
    ///
    /// # Errors
    ///
    /// Returns an error if the realm is not found or the node-level
    /// leave operation fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// network.leave_realm(&realm.id()).await?;
    /// ```
    pub async fn leave_realm(&self, id: &RealmId) -> Result<()> {
        // Remove from local caches
        self.realms.remove(id);

        // Remove from peer_realms if it's a peer-based realm
        self.peer_realms.retain(|_, v| v != id);

        // Leave at the node level (broadcasts leave, cleans up gossip)
        self.inner.leave_interface(id).await?;

        Ok(())
    }

    // ============================================================
    // Direct connection — "Identity IS Connection"
    // ============================================================

    /// Connect to a peer by MemberId — the core one-call API.
    ///
    /// This is the primary way to establish a DM connection. It:
    /// 1. Computes a deterministic DM realm ID from both MemberIds
    /// 2. Creates/joins the interface (joins gossip topic automatically)
    /// 3. Initiates ML-KEM key exchange if we're the initiator (lower MemberId)
    /// 4. Returns a Realm that's ready for messaging once key exchange completes
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The MemberId of the peer to connect to
    ///
    /// # Example
    ///
    /// ```ignore
    /// let realm = network.connect(nova_member_id).await?;
    /// realm.send("Hey Nova!").await?;
    /// ```
    pub async fn connect(&self, peer_id: MemberId) -> Result<(Realm, PeerInfo)> {
        let my_id = self.id();

        if peer_id == my_id {
            return Err(IndraError::InvalidOperation(
                "Cannot connect to yourself".to_string(),
            ));
        }

        // Ensure network transport is started
        if !self.is_running() {
            self.inner.start().await?;
            self.join_inbox().await?;
        }

        // 1. Use dm_story_id for canonical DM identity, derive interface ID from it
        let artifact_id = dm_story_id(my_id, peer_id);
        let realm_id = artifact_interface_id(&artifact_id);

        // 2. Check if already loaded
        if let Some(state) = self.realms.get(&realm_id) {
            let realm = Realm::from_id_with_chat_doc(realm_id, state.name.clone(), state.artifact_id.clone(), Arc::clone(&self.inner), Arc::clone(&state.chat_doc));
            let peer_info = self.extract_peer(&realm).await?;

            // Best-effort re-notify: if the peer hasn't reciprocated yet,
            // re-send our inbox notification (single attempt, no retries).
            self.re_notify_peer_inbox(peer_id, realm_id).await;

            return Ok((realm, peer_info));
        }

        // 3. Establish transport connectivity to peer via relay
        //    MemberId IS the raw PublicKey bytes, so we can connect directly.
        if let Err(e) = self.inner.connect_to_peer(&peer_id).await {
            tracing::debug!(
                peer = %hex::encode(&peer_id[..8]),
                error = %e,
                "Transport connect to peer failed (may still work via gossip)"
            );
        }

        // 4. Create the interface with deterministic ID, seed, and peer as bootstrap
        let peer_public_key = iroh::PublicKey::from_bytes(&peer_id)
            .map_err(|e| IndraError::Crypto(format!("Invalid peer key: {}", e)))?;
        let seed = artifact_key_seed(&artifact_id);
        let (interface_id, invite_key) = self
            .inner
            .create_interface_with_seed(realm_id, &seed, Some("DM"), vec![peer_public_key])
            .await?;

        // Add peer as member so send_message can reach them
        let peer_identity = IrohIdentity::from(peer_public_key);
        let _ = self.inner.add_member(&interface_id, peer_identity).await;

        // Register DM as a Story artifact in the HomeRealm
        if let Ok(home) = self.home_realm().await {
            if let Err(e) = home.ensure_dm_story(&artifact_id, peer_id).await {
                tracing::debug!(error = %e, "Failed to register DM story in home realm (non-fatal)");
            }
        }

        // 4. Cache realm state
        self.realms.insert(
            interface_id,
            RealmState {
                name: Some("DM".to_string()),
                artifact_id: Some(artifact_id),
                chat_doc: Arc::new(OnceCell::new()),
            },
        );

        // 5. Add contact if not already present (auto-confirm for direct connect)
        //    Look up the peer's display name from the node's peer registry
        //    (populated by gossip discovery) so it appears in the UI.
        let contacts = self.join_contacts_realm().await?;
        if !contacts.is_contact(&peer_id).await {
            let peer_name = self.inner.storage().peer_registry()
                .get(&IrohIdentity::from(peer_public_key))
                .ok()
                .flatten()
                .and_then(|r| r.display_name.clone());
            let _ = contacts.add_contact_with_name(peer_id, peer_name).await;
        }
        let _ = contacts.confirm_contact(&peer_id).await;

        // 6. Cache peer mapping
        let mut peers = vec![my_id, peer_id];
        peers.sort();
        self.peer_realms.insert(peers, interface_id);

        tracing::info!(
            peer = %hex::encode(&peer_id[..8]),
            realm = %hex::encode(&realm_id.as_bytes()[..8]),
            initiator = is_initiator(&my_id, &peer_id),
            "Direct connection established"
        );

        // 7. Send connection notification to peer's inbox
        self.notify_peer_inbox(peer_id, realm_id).await;

        let realm = Realm::new(
            interface_id,
            Some("DM".to_string()),
            Some(artifact_id),
            InviteCode::new(invite_key),
            Arc::clone(&self.inner),
        );

        // 8. Extract peer info and emit ConversationOpened event
        let peer = self.extract_peer(&realm).await?;
        let _ = self.peer_event_tx.send(PeerEvent::ConversationOpened {
            realm_id: realm.id(),
            peer: peer.clone(),
        });

        // Trigger immediate contact poll to pick up the new peer
        self.poll_notify.notify_one();

        Ok((realm, peer))
    }

    /// Send a ConnectionNotify to the peer's inbox realm.
    ///
    /// Spawns a background task that aggressively retries delivery for up to
    /// 60 seconds. Before each attempt, establishes a transport connection
    /// to the peer (required for `send_message` to actually deliver).
    async fn notify_peer_inbox(&self, peer_id: MemberId, dm_realm_id: RealmId) {
        let my_id = self.id();
        let peer_inbox_id = inbox_realm_id(peer_id);

        // Convert peer MemberId → PublicKey for bootstrap
        let peer_public_key = match iroh::PublicKey::from_bytes(&peer_id) {
            Ok(pk) => pk,
            Err(e) => {
                tracing::debug!(error = %e, "Invalid peer key for inbox notify");
                return;
            }
        };

        // Join the peer's inbox realm temporarily with deterministic key + bootstrap
        let peer_inbox_seed = inbox_key_seed(&peer_id);
        let join_result = self
            .inner
            .create_interface_with_seed(peer_inbox_id, &peer_inbox_seed, Some("PeerInbox"), vec![peer_public_key])
            .await;

        if let Err(e) = join_result {
            tracing::debug!(
                peer = %hex::encode(&peer_id[..8]),
                error = %e,
                "Failed to join peer inbox (non-fatal)"
            );
            return;
        }

        // Add peer as member so send_message delivers to them
        let peer_identity = IrohIdentity::from(peer_public_key);
        let _ = self.inner.add_member(&peer_inbox_id, peer_identity).await;

        // Build the notification
        let mut notify = ConnectionNotify::new(my_id, dm_realm_id);
        if let Some(name) = self.display_name() {
            notify = notify.with_name(name);
        }
        // Include our endpoint address so peer can connect directly
        if let Some(addr) = self.inner.endpoint_addr().await {
            if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                notify = notify.with_endpoint_addr(addr_bytes);
            }
        }

        let payload = match notify.to_bytes() {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(error = %e, "Failed to serialize inbox notification");
                return;
            }
        };

        // Spawn background task: aggressively retry for 60s with connect_to_peer
        // before each attempt. send_message only delivers to connected peers,
        // so we must ensure the QUIC transport connection is established first.
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut sent = false;
            for attempt in 0..12u32 {
                // Establish transport connection before each send attempt.
                // This is critical: send_message only delivers to peers with
                // an active QUIC connection (transport.is_connected).
                if let Err(e) = inner.connect_to_peer(&peer_id).await {
                    tracing::debug!(
                        peer = %hex::encode(&peer_id[..8]),
                        attempt,
                        error = %e,
                        "Inbox notify: transport connect failed, will retry"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }

                // Small delay to let transport stabilize after connect
                if attempt == 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }

                match inner.send_message(&peer_inbox_id, payload.clone()).await {
                    Ok(_) => {
                        tracing::info!(
                            peer = %hex::encode(&peer_id[..8]),
                            attempt,
                            "Sent connection notification to peer inbox"
                        );
                        sent = true;
                        break;
                    }
                    Err(e) => {
                        tracing::debug!(
                            peer = %hex::encode(&peer_id[..8]),
                            attempt,
                            error = %e,
                            "Inbox notify attempt failed (retrying)"
                        );
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }

            if !sent {
                tracing::debug!(
                    peer = %hex::encode(&peer_id[..8]),
                    "All inbox notify attempts exhausted (non-fatal)"
                );
            }

            // Leave peer's inbox after delivery or timeout
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let _ = inner.leave_interface(&peer_inbox_id).await;
        });
    }

    /// Re-notification for peers where the initial notify may have been lost.
    ///
    /// Called from `connect()` early-return path. Throttled to once per 30 seconds
    /// per peer (not once-per-session) so the polling loop can keep retrying.
    /// Establishes transport connection before sending.
    async fn re_notify_peer_inbox(&self, peer_id: MemberId, dm_realm_id: RealmId) {
        // Throttle: skip if we re-notified this peer within the last 30 seconds
        let now = std::time::Instant::now();
        if let Some(last) = self.re_notified_peers.get(&peer_id) {
            if now.duration_since(*last) < std::time::Duration::from_secs(30) {
                return;
            }
        }
        self.re_notified_peers.insert(peer_id, now);

        tracing::debug!(
            peer = %hex::encode(&peer_id[..8]),
            "Re-notifying peer inbox (with transport connect)"
        );

        // Establish transport connection first — critical for send_message delivery
        if let Err(e) = self.inner.connect_to_peer(&peer_id).await {
            tracing::debug!(
                peer = %hex::encode(&peer_id[..8]),
                error = %e,
                "Re-notify: transport connect failed"
            );
            // Don't return — still try send_message in case connection exists from elsewhere
        }

        let my_id = self.id();
        let peer_inbox_id = inbox_realm_id(peer_id);

        let peer_public_key = match iroh::PublicKey::from_bytes(&peer_id) {
            Ok(pk) => pk,
            Err(_) => return,
        };

        // Join the peer's inbox realm
        let peer_inbox_seed = inbox_key_seed(&peer_id);
        if self
            .inner
            .create_interface_with_seed(peer_inbox_id, &peer_inbox_seed, Some("PeerInbox"), vec![peer_public_key])
            .await
            .is_err()
        {
            return;
        }

        let peer_identity = IrohIdentity::from(peer_public_key);
        let _ = self.inner.add_member(&peer_inbox_id, peer_identity).await;

        // Build notification
        let mut notify = ConnectionNotify::new(my_id, dm_realm_id);
        if let Some(name) = self.display_name() {
            notify = notify.with_name(name);
        }
        if let Some(addr) = self.inner.endpoint_addr().await {
            if let Ok(addr_bytes) = postcard::to_allocvec(&addr) {
                notify = notify.with_endpoint_addr(addr_bytes);
            }
        }

        // Single send attempt (polling loop will call us again in 30s if needed)
        if let Ok(payload) = notify.to_bytes() {
            match self.inner.send_message(&peer_inbox_id, payload).await {
                Ok(_) => {
                    tracing::info!(
                        peer = %hex::encode(&peer_id[..8]),
                        "Re-notification sent to peer inbox"
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        peer = %hex::encode(&peer_id[..8]),
                        error = %e,
                        "Re-notification failed (non-fatal)"
                    );
                }
            }
        }

        // Leave inbox after a short delay
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let _ = inner.leave_interface(&peer_inbox_id).await;
        });
    }

    /// Connect to a peer using a compact identity code (bech32m).
    ///
    /// Parses the identity code to extract the MemberId, then calls `connect()`.
    ///
    /// # Arguments
    ///
    /// * `code` - A bech32m identity code like `indra1qw508d6q...`
    ///
    /// # Example
    ///
    /// ```ignore
    /// let realm = network.connect_by_code("indra1qw508d6q...").await?;
    /// ```
    pub async fn connect_by_code(&self, code: &str) -> Result<(Realm, PeerInfo)> {
        // Try parsing as identity code first, then as URI with name
        let (identity_code, display_name) = IdentityCode::parse_uri(code)?;
        let peer_id = identity_code.member_id();

        // If we got a display name, update the contact
        if let Some(name) = display_name {
            let contacts = self.join_contacts_realm().await?;
            if !contacts.is_contact(&peer_id).await {
                let _ = contacts.add_contact_with_name(peer_id, Some(name)).await;
            }
        }

        self.connect(peer_id).await
    }

    /// Get this network's compact identity code (bech32m).
    ///
    /// This is a short string (~58 chars) that can be shared for connecting.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let code = network.identity_code();
    /// println!("Share this: {}", code);  // indra1qw508d6q...
    /// ```
    pub fn identity_code(&self) -> String {
        IdentityCode::from_member_id(self.id()).encode()
    }

    /// Get this network's identity URI with display name.
    ///
    /// Includes the display name as a query parameter for convenience.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let uri = network.identity_uri();
    /// println!("Share this: {}", uri);  // indra1qw508d6q...?name=Zephyr
    /// ```
    pub fn identity_uri(&self) -> String {
        IdentityCode::from_member_id(self.id()).to_uri(self.display_name().as_deref())
    }

    /// Create an encounter for in-person peer discovery.
    ///
    /// Generates a 6-digit code, joins the encounter gossip topic,
    /// and broadcasts our MemberId. Returns the code and a handle
    /// for cleanup.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (code, handle) = network.create_encounter().await?;
    /// println!("Tell them: {}", code);  // "743901"
    /// ```
    pub async fn create_encounter(&self) -> Result<(String, encounter::EncounterHandle)> {
        // Ensure network is started
        if !self.is_running() {
            self.inner.start().await?;
        }

        // 1. Generate random 6-digit code
        let code = encounter::generate_encounter_code();

        // 2. Get encounter topics (current + previous time window)
        let topics = encounter::encounter_topics(&code);

        // 3. Join the encounter topics as interfaces
        for topic in &topics {
            let _ = self
                .inner
                .create_interface_with_id(*topic, Some("Encounter"))
                .await;
        }

        let handle = encounter::EncounterHandle {
            code: code.clone(),
            topics: topics.clone(),
        };

        tracing::info!(code = %code, "Created encounter");

        // 4. Schedule cleanup after 90 seconds (60s window + 30s grace)
        let inner = Arc::clone(&self.inner);
        let cleanup_topics = topics;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(90)).await;
            for topic in cleanup_topics {
                let _ = inner.leave_interface(&topic).await;
            }
        });

        Ok((code, handle))
    }

    /// Join an encounter using a 6-digit code.
    ///
    /// Joins the encounter gossip topic to discover the other peer's MemberId,
    /// then automatically calls `connect()` with the discovered ID.
    ///
    /// # Arguments
    ///
    /// * `code` - The 6-digit encounter code (e.g., "743901")
    ///
    /// # Example
    ///
    /// ```ignore
    /// let realm = network.join_encounter("743901").await?;
    /// ```
    pub async fn join_encounter(&self, code: &str) -> Result<MemberId> {
        let code = code.trim().to_string();
        encounter::validate_encounter_code(&code)?;

        // Ensure network is started
        if !self.is_running() {
            self.inner.start().await?;
        }

        // Join encounter topics
        let topics = encounter::encounter_topics(&code);
        for topic in &topics {
            let _ = self
                .inner
                .create_interface_with_id(*topic, Some("Encounter"))
                .await;
        }

        // Schedule cleanup after 90 seconds
        let inner = Arc::clone(&self.inner);
        let cleanup_topics = topics;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(90)).await;
            for topic in cleanup_topics {
                let _ = inner.leave_interface(&topic).await;
            }
        });

        // Note: The actual peer discovery happens via gossip events.
        // The caller should listen for peer events on the encounter topics.
        // For now, we return an error indicating the encounter was joined
        // but peer discovery is async.
        Err(IndraError::InvalidOperation(
            "Encounter joined — peer discovery is asynchronous. \
             Listen for peer events to discover the peer's MemberId, \
             then call connect() with it."
                .to_string(),
        ))
    }

    /// Introduce two peers who don't know each other.
    ///
    /// Sends each peer the other's MemberId on their respective
    /// DM realms, allowing them to call `connect()`.
    ///
    /// # Arguments
    ///
    /// * `peer_a` - First peer's MemberId
    /// * `peer_b` - Second peer's MemberId
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Bodhi introduces Zephyr and Nova
    /// bodhi_network.introduce(zephyr_id, nova_id).await?;
    /// ```
    pub async fn introduce(&self, peer_a: MemberId, peer_b: MemberId) -> Result<()> {
        if peer_a == peer_b {
            return Err(IndraError::InvalidOperation(
                "Cannot introduce a peer to themselves".to_string(),
            ));
        }

        // Send peer_b's ID to peer_a via our DM realm with peer_a
        let (realm_a, _) = self.connect(peer_a).await?;
        realm_a.send(format!("__intro__:{}", hex::encode(&peer_b))).await?;

        // Send peer_a's ID to peer_b via our DM realm with peer_b
        let (realm_b, _) = self.connect(peer_b).await?;
        realm_b.send(format!("__intro__:{}", hex::encode(&peer_a))).await?;

        tracing::info!(
            peer_a = %hex::encode(&peer_a[..8]),
            peer_b = %hex::encode(&peer_b[..8]),
            "Introduced two peers"
        );

        Ok(())
    }

    // ============================================================
    // Peer-based realm operations
    // ============================================================

    /// Compute a deterministic realm ID from a set of peers.
    ///
    /// The same set of peers always produces the same realm ID,
    /// regardless of the order they're provided in.
    fn compute_realm_id_for_peers(peers: &[MemberId]) -> RealmId {
        let sorted: BTreeSet<&MemberId> = peers.iter().collect();
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"realm-peers-v1:");
        for peer_id in sorted {
            hasher.update(peer_id);
        }
        InterfaceId::new(*hasher.finalize().as_bytes())
    }

    /// Normalize a peer set to a canonical sorted form.
    fn normalize_peers(peers: &[MemberId]) -> Vec<MemberId> {
        let mut sorted: Vec<MemberId> = peers.to_vec();
        sorted.sort();
        sorted.dedup();
        sorted
    }

    /// Get or create a realm for a specific set of peers.
    ///
    /// This is the primary way to access realms in the "tag friends" pattern.
    /// The peer set IS the realm identity - the same peers always return
    /// the same realm.
    ///
    /// **Important:** All peers must be in your contacts before you can create
    /// a realm with them. Join the contacts realm and add contacts first.
    ///
    /// # Arguments
    ///
    /// * `peers` - The set of member IDs that define this realm (must include yourself)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The peer set doesn't include yourself
    /// - You haven't joined the contacts realm yet
    /// - Any peer (other than yourself) is not in your contacts
    ///
    /// # Example
    ///
    /// ```ignore
    /// // First, join contacts and add friends
    /// let contacts = network.join_contacts_realm().await?;
    /// contacts.add_contact(friend1).await?;
    /// contacts.add_contact(friend2).await?;
    ///
    /// // Now you can create a realm with them
    /// let peers = vec![my_id, friend1, friend2];
    /// let realm = network.realm(peers).await?;
    ///
    /// // Same peers = same realm (order doesn't matter)
    /// let realm2 = network.realm(vec![friend2, my_id, friend1]).await?;
    /// assert_eq!(realm.id(), realm2.id());
    /// ```
    pub async fn realm(&self, peers: Vec<MemberId>) -> Result<Realm> {
        // Ensure we're in the peer set
        let my_id = self.id();
        if !peers.contains(&my_id) {
            return Err(IndraError::InvalidOperation(
                "Peer set must include yourself".to_string(),
            ));
        }

        // Normalize and compute ID
        let normalized = Self::normalize_peers(&peers);
        let realm_id = Self::compute_realm_id_for_peers(&normalized);

        // Check if already loaded (skip contact validation for existing realms)
        if let Some(state) = self.realms.get(&realm_id) {
            return Ok(Realm::from_id_with_chat_doc(realm_id, state.name.clone(), state.artifact_id.clone(), Arc::clone(&self.inner), Arc::clone(&state.chat_doc)));
        }

        // Enforce: all peers must be contacts before creating a new realm
        let contacts_realm = {
            let guard = self.contacts_realm.read().await;
            guard.clone()
        };

        match contacts_realm {
            None => {
                return Err(IndraError::InvalidOperation(
                    "Must join contacts realm before creating peer-based realms. \
                     Call join_contacts_realm() first.".to_string(),
                ));
            }
            Some(contacts) => {
                // Verify all peers (except ourselves) are in our contacts
                let my_contacts = contacts.contacts_list().await;
                for peer in &normalized {
                    if *peer != my_id && !my_contacts.contains(peer) {
                        return Err(IndraError::InvalidOperation(
                            format!(
                                "Cannot create realm: peer {} is not in your contacts. \
                                 Add them as a contact first.",
                                hex::encode(&peer[..8])
                            ),
                        ));
                    }
                }
            }
        }

        // Ensure network is started
        if !self.is_running() {
            self.inner.start().await?;
        }

        // Create the realm with deterministic ID
        let (interface_id, invite_key) = self
            .inner
            .create_interface_with_id(realm_id, None)
            .await?;

        // Cache the realm state
        self.realms.insert(interface_id, RealmState { name: None, artifact_id: None, chat_doc: Arc::new(OnceCell::new()) });

        // Cache the peer mapping
        self.peer_realms.insert(normalized, interface_id);

        Ok(Realm::new(
            interface_id,
            None,
            None,
            InviteCode::new(invite_key),
            Arc::clone(&self.inner),
        ))
    }

    /// Get an existing realm for a peer set without creating it.
    ///
    /// Returns None if the realm hasn't been accessed yet via `realm()`.
    ///
    /// # Arguments
    ///
    /// * `peers` - The set of member IDs that define this realm
    pub fn get_realm(&self, peers: &[MemberId]) -> Option<Realm> {
        let normalized = Self::normalize_peers(peers);
        let realm_id = Self::compute_realm_id_for_peers(&normalized);

        self.realms.get(&realm_id).map(|state| {
            Realm::from_id_with_chat_doc(realm_id, state.name.clone(), state.artifact_id.clone(), Arc::clone(&self.inner), Arc::clone(&state.chat_doc))
        })
    }

    // ============================================================
    // Blocking
    // ============================================================

    /// Block a contact: remove them and automatically leave all shared peer-set realms.
    ///
    /// This is the "hard isolation" response — stronger than negative sentiment.
    /// The contact is removed from the contacts list, and every peer-set realm
    /// that includes the blocked member is left immediately.
    ///
    /// Returns the list of realm IDs that were left as a result of the cascade.
    pub async fn block_contact(&self, member_id: &MemberId) -> Result<Vec<RealmId>> {
        // Get the contacts realm
        let contacts_realm = {
            let guard = self.contacts_realm.read().await;
            guard.clone()
        };

        let contacts = contacts_realm.ok_or_else(|| {
            IndraError::InvalidOperation(
                "Must join contacts realm before blocking. Call join_contacts_realm() first."
                    .to_string(),
            )
        })?;

        // Remove the contact
        let removed = contacts.remove_contact(member_id).await?;
        if !removed {
            return Err(IndraError::InvalidOperation(
                "Cannot block: member is not in your contacts".to_string(),
            ));
        }

        // Find all peer-set realms containing the blocked member and leave them
        let mut left_realms = Vec::new();
        let realms_to_leave: Vec<(Vec<MemberId>, RealmId)> = self
            .peer_realms
            .iter()
            .filter(|entry| entry.key().contains(member_id))
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect();

        for (_peers, realm_id) in realms_to_leave {
            if let Err(e) = self.leave_realm(&realm_id).await {
                // Log but continue — best-effort cascade
                tracing::warn!("Failed to leave realm {} during block cascade: {}", hex::encode(&realm_id.as_bytes()[..8]), e);
            } else {
                left_realms.push(realm_id);
            }
        }

        // Emit PeerBlocked event
        let _ = self.peer_event_tx.send(PeerEvent::PeerBlocked {
            member_id: *member_id,
            left_realms: left_realms.clone(),
        });

        Ok(left_realms)
    }

    // ============================================================
    // Contacts realm
    // ============================================================

    /// Join the global contacts realm.
    ///
    /// The contacts realm is used for:
    /// - Managing your contact list
    /// - Exchanging cryptographic keys with contacts
    /// - Auto-subscribing to peer-set realms
    ///
    /// # Example
    ///
    /// ```ignore
    /// let contacts = network.join_contacts_realm().await?;
    /// contacts.add_contact(friend_id).await?;
    /// ```
    pub async fn join_contacts_realm(&self) -> Result<ContactsRealm> {
        // Check if already joined
        {
            let guard = self.contacts_realm.read().await;
            if let Some(ref contacts) = *guard {
                return Ok(contacts.clone());
            }
        }

        // Ensure network is started
        if !self.is_running() {
            self.inner.start().await?;
        }

        // Get the contacts document from the home realm
        let home = self.home_realm().await?;
        let document = home.contacts().await?;

        // Create the contacts realm wrapper
        let contacts = ContactsRealm::from_home(document, self.id());

        // Cache it
        {
            let mut guard = self.contacts_realm.write().await;
            *guard = Some(contacts.clone());
        }

        Ok(contacts)
    }

    /// Get the contacts realm if already joined.
    ///
    /// Returns None if `join_contacts_realm()` hasn't been called yet.
    pub async fn contacts_realm(&self) -> Option<ContactsRealm> {
        let guard = self.contacts_realm.read().await;
        guard.clone()
    }

    // ============================================================
    // Home realm
    // ============================================================

    /// Get or create the home realm.
    ///
    /// The home realm is a personal realm unique to this user, containing:
    /// - Personal quests and tasks
    /// - Notes and documents
    /// - Stored artifacts (images, files, etc.)
    ///
    /// The home realm ID is deterministically derived from the user's
    /// member ID, enabling multi-device sync - all devices belonging
    /// to the same user will access the same home realm.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let home = network.home_realm().await?;
    ///
    /// // Upload a file
    /// let artifact_id = home.upload("photo.png").await?;
    ///
    /// // Grant access to another member
    /// home.grant_access(&artifact_id, peer_id, AccessMode::Read).await?;
    /// ```
    pub async fn home_realm(&self) -> Result<HomeRealm> {
        // Check if already initialized
        {
            let guard = self.home_realm.read().await;
            if let Some(ref home) = *guard {
                return Ok(home.clone());
            }
        }

        // Ensure network is started
        if !self.is_running() {
            self.inner.start().await?;
        }

        // Get the deterministic home realm ID
        let realm_id = home_realm_id(self.id());

        // Create the home realm interface with deterministic key
        // No bootstrap peers needed — this is our own home realm.
        let seed = home_key_seed(&self.id());
        let (_interface_id, _invite_key) = self
            .inner
            .create_interface_with_seed(realm_id, &seed, Some("Home"), vec![])
            .await?;

        // Cache the realm state
        self.realms.insert(
            realm_id,
            RealmState {
                name: Some("Home".to_string()),
                artifact_id: None,
                chat_doc: Arc::new(OnceCell::new()),
            },
        );

        // Create the home realm wrapper
        let home = HomeRealm::new(realm_id, Arc::clone(&self.inner), self.id()).await?;

        // Cache it
        {
            let mut guard = self.home_realm.write().await;
            *guard = Some(home.clone());
        }

        Ok(home)
    }

    /// Get the home realm if already initialized.
    ///
    /// Returns None if `home_realm()` hasn't been called yet.
    /// Prefer using `home_realm()` which will create it if needed.
    pub async fn get_home_realm(&self) -> Option<HomeRealm> {
        let guard = self.home_realm.read().await;
        guard.clone()
    }

    // ============================================================
    // Global events
    // ============================================================

    /// Get a global event stream across all realms.
    ///
    /// Returns a stream of `GlobalEvent`s that include events from all
    /// currently loaded realms, tagged with their source realm ID.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use futures::StreamExt;
    ///
    /// let mut events = network.events();
    /// while let Some(event) = events.next().await {
    ///     println!("Event in realm {:?}", event.realm_id);
    /// }
    /// ```
    pub fn events(&self) -> impl futures::Stream<Item = GlobalEvent> + Send + '_ {
        let realm_ids: Vec<RealmId> = self.realms.iter().map(|r| *r.key()).collect();
        let inner = Arc::clone(&self.inner);

        async_stream::stream! {
            use futures::StreamExt;
            use std::pin::Pin;

            let mut streams: Vec<Pin<Box<dyn futures::Stream<Item = GlobalEvent> + Send>>> = Vec::new();

            for realm_id in realm_ids {
                if let Ok(rx) = inner.events(&realm_id) {
                    let stream = crate::stream::broadcast_to_stream(rx);
                    let tagged = stream.map(move |event| GlobalEvent {
                        realm_id,
                        event,
                    });
                    streams.push(Box::pin(tagged));
                }
            }

            let mut merged = futures::stream::select_all(streams);
            while let Some(event) = merged.next().await {
                yield event;
            }
        }
    }

    // ============================================================
    // Identity export/import
    // ============================================================

    /// Export the identity keypair for backup.
    ///
    /// Returns a serialized bundle of all cryptographic keys (Ed25519
    /// transport key, ML-DSA signing key, ML-KEM key exchange key).
    /// Store this securely - it contains secret key material.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let backup = network.export_identity().await?;
    /// std::fs::write("identity.backup", &backup)?;
    /// ```
    pub async fn export_identity(&self) -> Result<Vec<u8>> {
        let data_dir = &self.config.data_dir;
        let keystore = indras_node::Keystore::new(data_dir);

        // Read iroh transport key
        let iroh_key = if keystore.exists() {
            tokio::fs::read(keystore.iroh_key_path()).await
                .map_err(|e| IndraError::Crypto(format!("Failed to read iroh key: {}", e)))?
        } else {
            return Err(IndraError::Crypto("No identity keys found to export".to_string()));
        };

        // Read PQ signing identity (both private and public keys, concatenated)
        let pq_identity = if keystore.pq_identity_exists() {
            let sk = tokio::fs::read(keystore.pq_signing_key_path()).await
                .map_err(|e| IndraError::Crypto(format!("Failed to read PQ signing key: {}", e)))?;
            let pk = tokio::fs::read(keystore.pq_verifying_key_path()).await
                .map_err(|e| IndraError::Crypto(format!("Failed to read PQ verifying key: {}", e)))?;
            // Pack as length-prefixed: [sk_len(4 bytes)][sk][pk]
            let mut combined = Vec::new();
            combined.extend_from_slice(&(sk.len() as u32).to_le_bytes());
            combined.extend_from_slice(&sk);
            combined.extend_from_slice(&pk);
            combined
        } else {
            Vec::new()
        };

        // Read PQ KEM keypair (both decapsulation and encapsulation keys, concatenated)
        let pq_kem = if keystore.pq_kem_exists() {
            let dk = tokio::fs::read(keystore.pq_kem_dk_path()).await
                .map_err(|e| IndraError::Crypto(format!("Failed to read PQ KEM dk: {}", e)))?;
            let ek = tokio::fs::read(keystore.pq_kem_ek_path()).await
                .map_err(|e| IndraError::Crypto(format!("Failed to read PQ KEM ek: {}", e)))?;
            // Pack as length-prefixed: [dk_len(4 bytes)][dk][ek]
            let mut combined = Vec::new();
            combined.extend_from_slice(&(dk.len() as u32).to_le_bytes());
            combined.extend_from_slice(&dk);
            combined.extend_from_slice(&ek);
            combined
        } else {
            Vec::new()
        };

        let backup = IdentityBackup {
            iroh_key,
            pq_identity,
            pq_kem,
        };

        let bytes = postcard::to_allocvec(&backup)?;
        Ok(bytes)
    }

    /// Import an identity from a backup.
    ///
    /// Restores the identity keypair from a previously exported backup.
    /// The network must not be running when importing. After import,
    /// create a new `IndrasNetwork` instance to use the restored identity.
    ///
    /// # Arguments
    ///
    /// * `data_dir` - Directory for persistent storage
    /// * `backup` - The backup data from `export_identity()`
    ///
    /// # Example
    ///
    /// ```ignore
    /// let backup = std::fs::read("identity.backup")?;
    /// IndrasNetwork::import_identity("~/.myapp", &backup).await?;
    ///
    /// // Now create a network instance with the restored identity
    /// let network = IndrasNetwork::new("~/.myapp").await?;
    /// ```
    pub async fn import_identity(
        data_dir: impl AsRef<std::path::Path>,
        backup: &[u8],
    ) -> Result<()> {
        let backup: IdentityBackup = postcard::from_bytes(backup)?;
        let data_dir = data_dir.as_ref();
        let keystore = indras_node::Keystore::new(data_dir);

        // Ensure data directory exists
        tokio::fs::create_dir_all(data_dir).await?;

        // Write iroh transport key
        if !backup.iroh_key.is_empty() {
            tokio::fs::write(keystore.iroh_key_path(), &backup.iroh_key).await?;
            indras_node::Keystore::set_restrictive_permissions(&keystore.iroh_key_path())
                .map_err(|e| IndraError::Crypto(format!("Failed to set permissions: {}", e)))?;
        }

        // Write PQ signing identity (unpack length-prefixed format)
        if !backup.pq_identity.is_empty() && backup.pq_identity.len() > 4 {
            let sk_len = u32::from_le_bytes(backup.pq_identity[..4].try_into().unwrap()) as usize;
            let sk = &backup.pq_identity[4..4 + sk_len];
            let pk = &backup.pq_identity[4 + sk_len..];
            tokio::fs::write(keystore.pq_signing_key_path(), sk).await?;
            tokio::fs::write(keystore.pq_verifying_key_path(), pk).await?;
            indras_node::Keystore::set_restrictive_permissions(&keystore.pq_signing_key_path())
                .map_err(|e| IndraError::Crypto(format!("Failed to set permissions: {}", e)))?;
        }

        // Write PQ KEM keypair (unpack length-prefixed format)
        if !backup.pq_kem.is_empty() && backup.pq_kem.len() > 4 {
            let dk_len = u32::from_le_bytes(backup.pq_kem[..4].try_into().unwrap()) as usize;
            let dk = &backup.pq_kem[4..4 + dk_len];
            let ek = &backup.pq_kem[4 + dk_len..];
            tokio::fs::write(keystore.pq_kem_dk_path(), dk).await?;
            tokio::fs::write(keystore.pq_kem_ek_path(), ek).await?;
            indras_node::Keystore::set_restrictive_permissions(&keystore.pq_kem_dk_path())
                .map_err(|e| IndraError::Crypto(format!("Failed to set permissions: {}", e)))?;
        }

        Ok(())
    }

    /// Export only the PQ signing identity (not the iroh transport key).
    ///
    /// For multi-device: the receiving device imports the PQ identity
    /// (shared user identity) but generates its own iroh transport key
    /// (unique device identity). Both devices share the same `UserId`
    /// but have different `MemberId`s.
    pub async fn export_pq_identity(&self) -> Result<Vec<u8>> {
        let data_dir = &self.config.data_dir;
        let keystore = indras_node::Keystore::new(data_dir);

        if !keystore.pq_identity_exists() {
            return Err(IndraError::Crypto("No PQ identity found to export".to_string()));
        }

        let backup = IdentityBackup {
            iroh_key: Vec::new(), // Intentionally empty — device generates its own
            pq_identity: {
                let sk = tokio::fs::read(keystore.pq_signing_key_path()).await
                    .map_err(|e| IndraError::Crypto(format!("Failed to read PQ signing key: {}", e)))?;
                let pk = tokio::fs::read(keystore.pq_verifying_key_path()).await
                    .map_err(|e| IndraError::Crypto(format!("Failed to read PQ verifying key: {}", e)))?;
                let mut combined = Vec::new();
                combined.extend_from_slice(&(sk.len() as u32).to_le_bytes());
                combined.extend_from_slice(&sk);
                combined.extend_from_slice(&pk);
                combined
            },
            pq_kem: {
                if keystore.pq_kem_exists() {
                    let dk = tokio::fs::read(keystore.pq_kem_dk_path()).await
                        .map_err(|e| IndraError::Crypto(format!("Failed to read PQ KEM dk: {}", e)))?;
                    let ek = tokio::fs::read(keystore.pq_kem_ek_path()).await
                        .map_err(|e| IndraError::Crypto(format!("Failed to read PQ KEM ek: {}", e)))?;
                    let mut combined = Vec::new();
                    combined.extend_from_slice(&(dk.len() as u32).to_le_bytes());
                    combined.extend_from_slice(&dk);
                    combined.extend_from_slice(&ek);
                    combined
                } else {
                    Vec::new()
                }
            },
        };

        let bytes = postcard::to_allocvec(&backup)?;
        Ok(bytes)
    }

    /// Import only PQ identity keys into a data directory.
    ///
    /// The iroh transport key is NOT imported — the device will generate
    /// its own fresh transport key when `IndrasNetwork::new()` is called.
    /// This gives the device a unique `MemberId` (device identity) while
    /// sharing the same `UserId` (user identity) derived from the PQ key.
    pub async fn import_pq_identity(
        data_dir: impl AsRef<std::path::Path>,
        backup: &[u8],
    ) -> Result<()> {
        let backup: IdentityBackup = postcard::from_bytes(backup)?;
        let data_dir = data_dir.as_ref();
        let keystore = indras_node::Keystore::new(data_dir);

        tokio::fs::create_dir_all(data_dir).await?;

        // Skip iroh key — device generates its own

        // Write PQ signing identity
        if !backup.pq_identity.is_empty() && backup.pq_identity.len() > 4 {
            let sk_len = u32::from_le_bytes(backup.pq_identity[..4].try_into().unwrap()) as usize;
            let sk = &backup.pq_identity[4..4 + sk_len];
            let pk = &backup.pq_identity[4 + sk_len..];
            tokio::fs::write(keystore.pq_signing_key_path(), sk).await?;
            tokio::fs::write(keystore.pq_verifying_key_path(), pk).await?;
            indras_node::Keystore::set_restrictive_permissions(&keystore.pq_signing_key_path())
                .map_err(|e| IndraError::Crypto(format!("Failed to set permissions: {}", e)))?;
        }

        // Write PQ KEM keypair
        if !backup.pq_kem.is_empty() && backup.pq_kem.len() > 4 {
            let dk_len = u32::from_le_bytes(backup.pq_kem[..4].try_into().unwrap()) as usize;
            let dk = &backup.pq_kem[4..4 + dk_len];
            let ek = &backup.pq_kem[4 + dk_len..];
            tokio::fs::write(keystore.pq_kem_dk_path(), dk).await?;
            tokio::fs::write(keystore.pq_kem_ek_path(), ek).await?;
            indras_node::Keystore::set_restrictive_permissions(&keystore.pq_kem_dk_path())
                .map_err(|e| IndraError::Crypto(format!("Failed to set permissions: {}", e)))?;
        }

        Ok(())
    }

    // ============================================================
    // Escape hatches
    // ============================================================

    /// Access the underlying node.
    ///
    /// This is an escape hatch for advanced users who need direct
    /// access to the infrastructure layer.
    pub fn node(&self) -> &IndrasNode {
        &self.inner
    }

    /// Access the underlying node as an `Arc`.
    pub fn node_arc(&self) -> Arc<IndrasNode> {
        self.inner.clone()
    }

    /// Access the storage layer.
    pub fn storage(&self) -> &CompositeStorage<IrohIdentity> {
        self.inner.storage()
    }

    /// Access the embedded relay auth service for direct contact sync.
    ///
    /// Returns `None` if the relay service has not started yet (i.e. before
    /// [`start`](Self::start) completes).
    pub fn relay_auth(&self) -> Option<&Arc<indras_relay::AuthService>> {
        self.inner.relay_service().map(|rs| rs.auth())
    }

    /// Get the embedded relay service for dashboard queries.
    ///
    /// Returns `None` if the relay service has not started yet (i.e. before
    /// [`start`](Self::start) completes).
    pub fn relay_service(&self) -> Option<&Arc<indras_relay::RelayService>> {
        self.node().relay_service()
    }

    /// Create a relay client using this node's transport identity.
    ///
    /// The returned client can connect to any relay server (including this
    /// node's own embedded relay) for store-and-forward blob storage.
    pub fn relay_client(&self) -> indras_transport::relay_client::RelayClient {
        let secret = self.inner.secret_key();
        let secret_bytes = secret.to_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret_bytes);
        indras_transport::relay_client::RelayClient::new(signing_key, secret.clone())
    }

    /// Get the shared relay blob endpoint for vault sync.
    ///
    /// Lazily creates a single `(RelayClient, Endpoint)` pair with a fresh
    /// transport key. All vault relay connections in this process reuse this
    /// endpoint, avoiding the overhead of creating one QUIC listener per
    /// relay session.
    pub async fn relay_blob_endpoint(
        &self,
    ) -> Result<&(indras_transport::relay_client::RelayClient, iroh::Endpoint)> {
        if let Some(pair) = self.relay_blob_endpoint.get() {
            return Ok(pair);
        }

        let signing_key = {
            let secret = self.inner.secret_key();
            ed25519_dalek::SigningKey::from_bytes(&secret.to_bytes())
        };
        let transport_secret = iroh::SecretKey::generate(&mut rand::rng());
        let client =
            indras_transport::relay_client::RelayClient::new(signing_key, transport_secret);
        let endpoint = client
            .create_endpoint()
            .await
            .map_err(|e| IndraError::Network(e.to_string()))?;

        // Race-safe: if another task initialized first, return theirs
        let _ = self.relay_blob_endpoint.set((client, endpoint));
        Ok(self.relay_blob_endpoint.get().unwrap())
    }

    /// Get this node's endpoint address for sharing with peers.
    ///
    /// Returns `None` if the transport has not started yet (i.e. before
    /// [`start`](Self::start) completes).
    pub async fn endpoint_addr(&self) -> Option<iroh::EndpointAddr> {
        self.inner.endpoint_addr().await
    }

    /// Access the network configuration.
    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }

    /// Save a JSON snapshot of this node's world view to `{data_dir}/world-view.json`.
    ///
    /// The snapshot captures identity, interfaces, members, peers, and
    /// transport state. Comparing snapshots across instances reveals
    /// sync discrepancies.
    // ============================================================
    // Peering — peer state, events, and contact management
    // ============================================================

    /// Snapshot of currently known peers.
    pub fn peers(&self) -> Vec<PeerInfo> {
        self.peers_rx.borrow().clone()
    }

    /// Reactive watcher for peer list changes.
    pub fn watch_peers(&self) -> watch::Receiver<Vec<PeerInfo>> {
        self.peers_rx.clone()
    }

    /// Subscribe to all peering events (peers, conversations, saves, etc.).
    pub fn peer_events(&self) -> broadcast::Receiver<PeerEvent> {
        self.peer_event_tx.subscribe()
    }

    /// Atomically subscribe to events AND get the current peer snapshot.
    ///
    /// This avoids the race where peers connect between `subscribe()` and `peers()`:
    /// the receiver is created first, so any changes after the snapshot will arrive
    /// as events.
    pub fn peer_events_with_snapshot(&self) -> (broadcast::Receiver<PeerEvent>, Vec<PeerInfo>) {
        let rx = self.peer_event_tx.subscribe();
        let peers = self.peers_rx.borrow().clone();
        (rx, peers)
    }

    /// Trigger an immediate contact poll cycle (instead of waiting for the next tick).
    pub fn refresh_peers(&self) {
        self.poll_notify.notify_one();
    }

    /// Remove a contact without the realm cascade.
    ///
    /// Returns `true` if the contact was found and removed.
    pub async fn remove_contact(&self, peer_id: &MemberId) -> Result<bool> {
        let contacts = self.contacts_realm_or_err().await?;
        let removed = contacts.remove_contact(peer_id).await?;

        if removed {
            let _ = self.peer_event_tx.send(PeerEvent::PeerDisconnected {
                member_id: *peer_id,
            });
        }

        Ok(removed)
    }

    /// Update sentiment toward a contact (-1, 0, or +1). Clamped to [-1, 1].
    ///
    /// Emits [`PeerEvent::SentimentChanged`] on success.
    pub async fn update_sentiment(&self, member_id: &MemberId, sentiment: i8) -> Result<()> {
        let clamped = sentiment.clamp(-1, 1);
        let contacts = self.contacts_realm_or_err().await?;
        contacts.update_sentiment(member_id, clamped).await?;

        let _ = self.peer_event_tx.send(PeerEvent::SentimentChanged {
            member_id: *member_id,
            sentiment: clamped,
        });

        Ok(())
    }

    /// Get sentiment toward a specific contact.
    pub async fn get_sentiment(&self, member_id: &MemberId) -> Result<Option<i8>> {
        let contacts = self.contacts_realm_or_err().await?;
        Ok(contacts.get_sentiment(member_id).await)
    }

    /// Set whether sentiment toward a contact is relayable to second-degree peers.
    pub async fn set_relayable(&self, member_id: &MemberId, relayable: bool) -> Result<()> {
        let contacts = self.contacts_realm_or_err().await?;
        contacts.set_relayable(member_id, relayable).await?;
        Ok(())
    }

    /// Get the full contact entry (sentiment, status, relayable, display_name).
    pub async fn get_contact_entry(
        &self,
        member_id: &MemberId,
    ) -> Result<Option<crate::contacts::ContactEntry>> {
        let contacts = self.contacts_realm_or_err().await?;
        Ok(contacts.get_contact_entry(member_id).await)
    }

    /// Build an aggregated sentiment view about a member from direct + relayed signals.
    pub async fn sentiment_view(
        &self,
        about: MemberId,
    ) -> Result<crate::sentiment::SentimentView> {
        let contacts = self.contacts_realm_or_err().await?;
        let direct = contacts.contacts_with_sentiment().await;

        let direct_about: Vec<(MemberId, i8)> = direct
            .into_iter()
            .filter(|(id, _)| *id == about)
            .collect();

        Ok(crate::sentiment::SentimentView {
            direct: direct_about,
            relayed: vec![],
        })
    }

    /// Helper: get the contacts realm or return an error.
    async fn contacts_realm_or_err(&self) -> Result<ContactsRealm> {
        self.contacts_realm()
            .await
            .ok_or(IndraError::ContactsRealmNotJoined)
    }

    /// Extract the remote peer from a DM realm's member list.
    async fn extract_peer(&self, realm: &Realm) -> Result<PeerInfo> {
        let members = realm.member_list().await?;
        let my_id = self.id();

        let member = members
            .iter()
            .find(|m| m.id() != my_id)
            .ok_or(IndraError::NoPeerInRealm)?;

        let (sentiment, status) = if let Some(cr) = self.contacts_realm().await {
            match cr.get_contact_entry(&member.id()).await {
                Some(entry) => (entry.sentiment, entry.status),
                None => (0, crate::contacts::ContactStatus::default()),
            }
        } else {
            (0, crate::contacts::ContactStatus::default())
        };

        Ok(PeerInfo {
            member_id: member.id(),
            display_name: member.name(),
            connected_at: chrono::Utc::now().timestamp(),
            sentiment,
            status,
        })
    }

    // ============================================================
    // World view
    // ============================================================

    pub async fn save_world_view(&self) -> Result<std::path::PathBuf> {
        let view = crate::world_view::WorldView::build(self).await;
        let path = self.config.data_dir.join("world-view.json");
        view.save(&path)?;
        tracing::info!(path = %path.display(), "Saved world view");
        Ok(path)
    }
}

impl NetworkBuilder {
    /// Build the IndrasNetwork instance.
    pub async fn build(self) -> Result<Arc<IndrasNetwork>> {
        IndrasNetwork::with_config(self.build_config()).await
    }
}

// Simple hex encoding for error messages
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_network_creation() {
        let temp = TempDir::new().unwrap();
        let network = IndrasNetwork::new(temp.path()).await.unwrap();
        assert!(!network.is_running());
    }

    #[tokio::test]
    async fn test_network_builder() {
        let temp = TempDir::new().unwrap();
        let network = IndrasNetwork::builder()
            .data_dir(temp.path())
            .display_name("Test Node")
            .build()
            .await
            .unwrap();

        assert_eq!(network.display_name(), Some("Test Node".to_string()));
    }
}
