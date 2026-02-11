//! IndrasNetwork - the main SyncEngine entry point.
//!
//! Provides a high-level API for building P2P applications on Indra's Network.

use crate::config::{NetworkBuilder, NetworkConfig, Preset};
use crate::contacts::{contacts_realm_id, ContactsRealm};
use crate::direct_connect::{dm_key_seed, dm_realm_id, inbox_key_seed, inbox_realm_id, is_initiator, ConnectionNotify};
use crate::encounter;
use crate::error::{IndraError, Result};
use crate::home_realm::{home_key_seed, home_realm_id, HomeRealm};
use crate::identity_code::IdentityCode;
use crate::invite::InviteCode;
use crate::member::{Member, MemberId};
use crate::realm::Realm;

use dashmap::DashMap;
use indras_core::InterfaceId;
use indras_node::{IndrasNode, ReceivedEvent};
use indras_storage::CompositeStorage;
use indras_transport::IrohIdentity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

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
    /// Our identity.
    identity: Member,
}

/// Internal realm state.
struct RealmState {
    name: Option<String>,
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
    pub async fn new(data_dir: impl AsRef<Path>) -> Result<Self> {
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
    pub async fn with_config(mut config: NetworkConfig) -> Result<Self> {
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

        Ok(Self {
            inner: Arc::new(node),
            realms: Arc::new(DashMap::new()),
            peer_realms: Arc::new(DashMap::new()),
            contacts_realm: Arc::new(RwLock::new(None)),
            home_realm: RwLock::new(None),
            config,
            identity,
        })
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
    pub fn display_name(&self) -> Option<&str> {
        self.config.display_name.as_deref()
    }

    /// Set the display name for this network instance.
    pub async fn set_display_name(&mut self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        self.config.display_name = Some(name.clone());

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

    /// Start the network.
    ///
    /// This begins accepting connections and synchronizing with peers.
    /// Must be called before creating or joining realms.
    pub async fn start(&self) -> Result<()> {
        // IndrasNode::start is idempotent, so this is safe to call multiple times
        self.inner.start().await?;

        // Join our inbox realm to receive connection notifications
        self.join_inbox().await?;

        Ok(())
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
                    let dm_realm_id = dm_realm_id(my_id, peer_id);

                    // Check if we already have this DM realm (idempotent)
                    if realms.contains_key(&dm_realm_id) {
                        tracing::debug!(
                            peer = %hex::encode(&peer_id[..8]),
                            "Inbox: DM realm already exists, skipping"
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
                    if let Err(e) = inner.connect_to_peer(&peer_id).await {
                        tracing::debug!(
                            peer = %hex::encode(&peer_id[..8]),
                            error = %e,
                            "Inbox: transport connect failed (may still work via gossip)"
                        );
                    }

                    // Create the DM realm (reciprocate the connection) with deterministic key + bootstrap
                    let dm_seed = dm_key_seed(&my_id, &peer_id);
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

                                let reply = ConnectionNotify::new(my_id, dm_realm_id);
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
            self.start().await?;
        }

        let (interface_id, invite_key) = self.inner.create_interface(Some(name)).await?;

        // Cache the realm state
        self.realms.insert(
            interface_id,
            RealmState {
                name: Some(name.to_string()),
            },
        );

        Ok(Realm::new(
            interface_id,
            Some(name.to_string()),
            InviteCode::new(invite_key),
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
            self.start().await?;
        }

        let invite_code = InviteCode::parse(invite.as_ref())?;
        let interface_id = self.inner.join_interface(invite_code.invite_key().clone()).await?;

        // Cache the realm state
        self.realms.insert(interface_id, RealmState { name: None });

        Ok(Realm::new(
            interface_id,
            None,
            invite_code,
            Arc::clone(&self.inner),
        ))
    }

    /// Get a realm by ID.
    ///
    /// Returns None if the realm is not loaded.
    pub fn get_realm_by_id(&self, id: &RealmId) -> Option<Realm> {
        self.realms.get(id).map(|state| {
            // We need to reconstruct the invite code, which we may not have
            // For now, return a realm without a valid invite code
            Realm::from_id(*id, state.name.clone(), Arc::clone(&self.inner))
        })
    }

    /// List all loaded realms.
    pub fn realms(&self) -> Vec<RealmId> {
        self.realms.iter().map(|r| *r.key()).collect()
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
    pub async fn connect(&self, peer_id: MemberId) -> Result<Realm> {
        let my_id = self.id();

        if peer_id == my_id {
            return Err(IndraError::InvalidOperation(
                "Cannot connect to yourself".to_string(),
            ));
        }

        // Ensure network is started
        if !self.is_running() {
            self.start().await?;
        }

        // 1. Compute deterministic DM realm ID
        let realm_id = dm_realm_id(my_id, peer_id);

        // 2. Check if already loaded
        if let Some(state) = self.realms.get(&realm_id) {
            return Ok(Realm::from_id(realm_id, state.name.clone(), Arc::clone(&self.inner)));
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
        let seed = dm_key_seed(&my_id, &peer_id);
        let (interface_id, invite_key) = self
            .inner
            .create_interface_with_seed(realm_id, &seed, Some("DM"), vec![peer_public_key])
            .await?;

        // Add peer as member so send_message can reach them
        let peer_identity = IrohIdentity::from(peer_public_key);
        let _ = self.inner.add_member(&interface_id, peer_identity).await;

        // 4. Cache realm state
        self.realms.insert(
            interface_id,
            RealmState {
                name: Some("DM".to_string()),
            },
        );

        // 5. Add contact if not already present (auto-confirm for direct connect)
        let contacts = self.join_contacts_realm().await?;
        if !contacts.is_contact(&peer_id).await {
            let _ = contacts.add_contact(peer_id).await;
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

        Ok(Realm::new(
            interface_id,
            Some("DM".to_string()),
            InviteCode::new(invite_key),
            Arc::clone(&self.inner),
        ))
    }

    /// Send a ConnectionNotify to the peer's inbox realm.
    ///
    /// Best-effort: if this fails, the connection still works — the peer
    /// just won't auto-discover it until they independently connect back.
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

        // Serialize and send as a message on the peer's inbox
        match notify.to_bytes() {
            Ok(payload) => {
                if let Err(e) = self.inner.send_message(&peer_inbox_id, payload).await {
                    tracing::debug!(
                        peer = %hex::encode(&peer_id[..8]),
                        error = %e,
                        "Failed to send inbox notification (non-fatal)"
                    );
                } else {
                    tracing::info!(
                        peer = %hex::encode(&peer_id[..8]),
                        "Sent connection notification to peer inbox"
                    );
                }
            }
            Err(e) => {
                tracing::debug!(
                    error = %e,
                    "Failed to serialize inbox notification (non-fatal)"
                );
            }
        }

        // Schedule leaving the peer's inbox after a short delay (cleanup)
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
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
    pub async fn connect_by_code(&self, code: &str) -> Result<Realm> {
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
        IdentityCode::from_member_id(self.id()).to_uri(self.display_name())
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
            self.start().await?;
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
            self.start().await?;
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
        let realm_a = self.connect(peer_a).await?;
        realm_a.send(format!("__intro__:{}", hex::encode(&peer_b))).await?;

        // Send peer_a's ID to peer_b via our DM realm with peer_b
        let realm_b = self.connect(peer_b).await?;
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
            return Ok(Realm::from_id(realm_id, state.name.clone(), Arc::clone(&self.inner)));
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
                let my_contacts = contacts.contacts_list_async().await;
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
            self.start().await?;
        }

        // Create the realm with deterministic ID
        let (interface_id, invite_key) = self
            .inner
            .create_interface_with_id(realm_id, None)
            .await?;

        // Cache the realm state
        self.realms.insert(interface_id, RealmState { name: None });

        // Cache the peer mapping
        self.peer_realms.insert(normalized, interface_id);

        Ok(Realm::new(
            interface_id,
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
            Realm::from_id(realm_id, state.name.clone(), Arc::clone(&self.inner))
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
            self.start().await?;
        }

        // Get the deterministic contacts realm ID
        let realm_id = contacts_realm_id();

        // Create or join the contacts realm
        let (_interface_id, _invite_key) = self
            .inner
            .create_interface_with_id(realm_id, Some("Contacts"))
            .await?;

        // Cache the realm state
        self.realms.insert(
            realm_id,
            RealmState {
                name: Some("Contacts".to_string()),
            },
        );

        // Create the contacts realm wrapper
        let contacts = ContactsRealm::new(
            realm_id,
            Arc::clone(&self.inner),
            self.id(),
        )
        .await?;

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
            self.start().await?;
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

    /// Access the storage layer.
    pub fn storage(&self) -> &CompositeStorage<IrohIdentity> {
        self.inner.storage()
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
    pub async fn build(self) -> Result<IndrasNetwork> {
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

        assert_eq!(network.display_name(), Some("Test Node"));
    }
}
