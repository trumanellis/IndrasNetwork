//! IndrasNetwork - the main SyncEngine entry point.
//!
//! Provides a high-level API for building P2P applications on Indra's Network.

use crate::config::{NetworkBuilder, NetworkConfig, Preset};
use crate::connection::{
    connection_realm_id, ConnectionDocument, ConnectionOffer, ConnectionAccept,
    ConnectionStatus, PendingConnection,
};
use crate::contacts::{contacts_realm_id, ContactsRealm};
use crate::direct_connect::{dm_realm_id, is_initiator, KeyExchangeStatus, PendingKeyExchange};
use crate::document::Document;
use crate::encounter;
use crate::error::{IndraError, Result};
use crate::home_realm::{home_realm_id, HomeRealm};
use crate::identity_code::IdentityCode;
use crate::invite::InviteCode;
use crate::member::{Member, MemberId};
use crate::realm::Realm;

use dashmap::DashMap;
use indras_core::InterfaceId;
use indras_crypto::InterfaceKey;
use indras_node::{IndrasNode, InviteKey, ReceivedEvent};
use indras_storage::CompositeStorage;
use indras_transport::{IrohIdentity, PeerEvent};
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
    contacts_realm: RwLock<Option<ContactsRealm>>,
    /// The home realm (lazily initialized).
    home_realm: RwLock<Option<HomeRealm>>,
    /// Active outgoing connection invites, keyed by connection realm ID.
    pending_connections: Arc<DashMap<RealmId, PendingConnection>>,
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
            contacts_realm: RwLock::new(None),
            home_realm: RwLock::new(None),
            pending_connections: Arc::new(DashMap::new()),
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

        // Restore persisted pending connections (best-effort)
        if let Err(e) = self.restore_pending_connections() {
            tracing::warn!(error = %e, "Failed to restore pending connections");
        }

        Ok(())
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

        // 3. Create the interface with deterministic ID
        let (interface_id, invite_key) = self
            .inner
            .create_interface_with_id(realm_id, Some("DM"))
            .await?;

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

        Ok(Realm::new(
            interface_id,
            Some("DM".to_string()),
            InviteCode::new(invite_key),
            Arc::clone(&self.inner),
        ))
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
                let my_contacts = contacts.contacts_list();
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

        // Clean up any pending connection realms involving the blocked member
        let blocked_id = *member_id;
        let connection_realms_to_clean: Vec<RealmId> = self
            .pending_connections
            .iter()
            .map(|entry| *entry.key())
            .collect();
        for conn_realm_id in connection_realms_to_clean {
            if let Ok(doc) = Document::<ConnectionDocument>::new(
                conn_realm_id,
                "connection".to_string(),
                Arc::clone(&self.inner),
            ).await {
                let data = doc.read().await;
                let involves_blocked = data.get_offer().map_or(false, |o| o.member_id == blocked_id)
                    || data.all_accepts().contains_key(&blocked_id);
                drop(data);
                if involves_blocked {
                    let _ = self.inner.leave_interface(&conn_realm_id).await;
                    self.pending_connections.remove(&conn_realm_id);
                }
            }
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
    // Connection invites (realm-based bidirectional handshake)
    // ============================================================

    /// Create a connection invite for this network identity.
    ///
    /// Generates a random nonce, derives a connection realm, seeds it with
    /// a `ConnectionOffer` containing our identity and PQ keys, and returns
    /// an invite code that can be shared out-of-band.
    pub async fn create_connection_invite(&self) -> Result<crate::contact_invite::ContactInviteCode> {
        // Ensure network is started
        if !self.is_running() {
            self.start().await?;
        }

        let my_id = self.id();

        // 1. Generate random 16-byte nonce
        let nonce: [u8; 16] = rand::random();

        // 2. Derive connection realm ID
        let realm_id = connection_realm_id(my_id, nonce);

        // 3. Create the connection realm interface
        let (_iface_id, invite_key) = self
            .inner
            .create_interface_with_id(realm_id, Some("Connection"))
            .await?;

        // 4. Get the interface encryption key
        let iface_key = self.inner.interface_key(&realm_id).ok_or_else(|| {
            IndraError::InvalidOperation("Connection realm key not found after creation".to_string())
        })?;

        // 5. Seed ConnectionOffer
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let endpoint_addr = if let Some(addr) = self.inner.endpoint_addr().await {
            postcard::to_allocvec(&addr).unwrap_or_default()
        } else {
            Vec::new()
        };

        let offer = ConnectionOffer {
            member_id: my_id,
            display_name: self.display_name().map(|s| s.to_string()),
            pq_encapsulation_key: self.inner.pq_kem_keypair().encapsulation_key_bytes(),
            pq_verifying_key: self.inner.pq_identity().verifying_key_bytes(),
            endpoint_addr,
            timestamp_millis: now,
        };

        let doc: Document<ConnectionDocument> = Document::new(
            realm_id,
            "connection".to_string(),
            Arc::clone(&self.inner),
        )
        .await?;

        doc.update(|conn| {
            conn.set_offer(offer);
        })
        .await?;

        // 6. Register in pending_connections
        self.pending_connections.insert(
            realm_id,
            PendingConnection {
                realm_id,
                nonce,
                created_at: now,
                status: ConnectionStatus::AwaitingAccept,
            },
        );

        // Persist to disk so invites survive restarts
        if let Err(e) = self.persist_pending_connections() {
            tracing::warn!(error = %e, "Failed to persist pending connections");
        }

        // 7. Build and return ContactInviteCode
        let bootstrap = invite_key
            .bootstrap_peers
            .first()
            .cloned()
            .unwrap_or_default();

        let code = crate::contact_invite::ContactInviteCode::new(
            my_id,
            self.display_name().map(|s| s.to_string()),
            nonce,
            bootstrap,
            *iface_key.as_bytes(),
        );

        tracing::info!(
            realm = %hex::encode(&realm_id.as_bytes()[..8]),
            "Created connection invite"
        );

        Ok(code)
    }

    /// Accept a connection invite code, completing the bidirectional handshake.
    ///
    /// Derives the connection realm from the invite, joins it, reads the
    /// inviter's `ConnectionOffer`, writes a `ConnectionAccept` with our info,
    /// adds the inviter as a Confirmed contact, and connects transport.
    pub async fn accept_connection_invite(
        &self,
        code: &crate::contact_invite::ContactInviteCode,
    ) -> Result<()> {
        // Ensure network is started
        if !self.is_running() {
            self.start().await?;
        }

        // 1. Derive connection realm ID from inviter's member_id + nonce
        let realm_id = connection_realm_id(code.member_id(), *code.connection_nonce());

        // 2. Set the interface encryption key from the invite
        let iface_key = InterfaceKey::from_bytes(*code.realm_key(), realm_id);
        self.inner.set_interface_key(realm_id, iface_key);

        // 3. Join the connection realm
        let invite = InviteKey::new(realm_id)
            .with_bootstrap(code.bootstrap().to_vec());

        self.inner.join_interface(invite).await?;

        // 4. Connect to bootstrap for transport
        if !code.bootstrap().is_empty() {
            if let Err(e) = self.inner.connect_to_bootstrap(code.bootstrap()).await {
                tracing::debug!(error = %e, "Bootstrap connectivity with inviter (non-fatal)");
            }
        }

        // 5. Read ConnectionOffer with retry (wait for CRDT sync)
        let doc: Document<ConnectionDocument> = Document::new(
            realm_id,
            "connection".to_string(),
            Arc::clone(&self.inner),
        )
        .await?;

        let mut offer = None;
        for attempt in 0..6 {
            if attempt > 0 {
                // Exponential backoff: 500ms, 1s, 2s, 4s, 8s
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << attempt))).await;
            }
            // Re-check the realm's event log for new messages from the inviter
            let _ = doc.refresh().await;
            let data = doc.read().await;
            if let Some(o) = data.get_offer() {
                offer = Some(o.clone());
                break;
            }
            drop(data);
            tracing::debug!(attempt = attempt + 1, "Connection offer not found yet, retrying...");
        }

        let offer = offer.ok_or_else(|| {
            IndraError::InvalidOperation("Connection offer not found in realm after retries".to_string())
        })?;

        // 6. Validate offer's member_id matches invite
        if offer.member_id != code.member_id() {
            return Err(IndraError::InvalidOperation(
                "Connection offer member_id does not match invite".to_string(),
            ));
        }

        // 7. Write ConnectionAccept with our info
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let endpoint_addr = if let Some(addr) = self.inner.endpoint_addr().await {
            postcard::to_allocvec(&addr).unwrap_or_default()
        } else {
            Vec::new()
        };

        let accept = ConnectionAccept {
            member_id: self.id(),
            display_name: self.display_name().map(|s| s.to_string()),
            pq_encapsulation_key: self.inner.pq_kem_keypair().encapsulation_key_bytes(),
            pq_verifying_key: self.inner.pq_identity().verifying_key_bytes(),
            endpoint_addr,
            timestamp_millis: now,
        };

        doc.update(|conn| {
            conn.add_accept(accept);
        })
        .await?;

        // 8. Add inviter as Confirmed contact
        let contacts = self.join_contacts_realm().await?;
        if !contacts.is_contact(&code.member_id()).await {
            contacts
                .add_contact_with_name(
                    code.member_id(),
                    code.display_name().map(|s| s.to_string()),
                )
                .await?;
        }
        let _ = contacts.confirm_contact(&code.member_id()).await;

        // 9. Connect transport to inviter via their offer's endpoint
        if !offer.endpoint_addr.is_empty() {
            if let Err(e) = self.inner.connect_to_bootstrap(&offer.endpoint_addr).await {
                tracing::debug!(error = %e, "Transport connect to inviter (non-fatal)");
            }
        }

        // 10. Schedule deferred cleanup of connection realm
        let inner = Arc::clone(&self.inner);
        let cleanup_realm_id = realm_id;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            if let Err(e) = inner.leave_interface(&cleanup_realm_id).await {
                tracing::debug!(error = %e, "Connection realm cleanup (non-fatal)");
            }
        });

        tracing::info!(
            inviter = %hex::encode(&code.member_id()[..8]),
            "Accepted connection invite"
        );

        Ok(())
    }

    /// Spawn a background listener for incoming connection accepts.
    ///
    /// Subscribes to `PeerEvent::RealmPeerJoined` and checks if the
    /// joined realm is a pending connection. When found, reads the
    /// `ConnectionDocument` for accepts, adds contacts, and cleans up.
    pub async fn spawn_connection_listener(&self) -> Result<()> {
        let peer_events = self.inner.subscribe_peer_events().await;
        let Some(mut peer_rx) = peer_events else {
            tracing::debug!("No transport available for connection listener");
            return Ok(());
        };

        let inner = Arc::clone(&self.inner);
        let pending = Arc::clone(&self.pending_connections);
        let my_id = self.id();

        tokio::spawn(async move {
            loop {
                match peer_rx.recv().await {
                    Ok(PeerEvent::RealmPeerJoined { interface_id, .. }) => {
                        // Check if this is a pending connection realm
                        if !pending.contains_key(&interface_id) {
                            continue;
                        }

                        // Read ConnectionDocument with retries (acceptor needs time
                        // to read our offer and write their accept back)
                        let accepts = {
                            let doc: std::result::Result<Document<ConnectionDocument>, _> =
                                Document::new(
                                    interface_id,
                                    "connection".to_string(),
                                    Arc::clone(&inner),
                                )
                                .await;

                            let Ok(doc) = doc else {
                                tracing::debug!("Failed to read connection document");
                                continue;
                            };

                            let mut found = std::collections::BTreeMap::new();
                            for attempt in 0..6 {
                                // Backoff: 2s, 3s, 4s, 6s, 8s, 12s
                                let delay = if attempt == 0 { 2000 } else { 1000 * (1 << attempt) };
                                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                                let _ = doc.refresh().await;
                                let data = doc.read().await;
                                let accepts = data.all_accepts().clone();
                                drop(data);
                                if !accepts.is_empty() {
                                    found = accepts;
                                    break;
                                }
                                tracing::debug!(
                                    attempt = attempt + 1,
                                    "No connection accepts yet, retrying..."
                                );
                            }
                            drop(doc);
                            found
                        };

                        if accepts.is_empty() {
                            tracing::debug!("No accepts received after retries, giving up");
                            continue;
                        }

                        // Collect accept info for processing (avoid holding Document across awaits)
                        let mut new_contacts: Vec<(MemberId, Option<String>, Vec<u8>)> = Vec::new();
                        for (acceptor_id, accept) in &accepts {
                            if *acceptor_id == my_id {
                                continue;
                            }
                            new_contacts.push((
                                *acceptor_id,
                                accept.display_name.clone(),
                                accept.endpoint_addr.clone(),
                            ));
                        }

                        // Process contacts and transport connections
                        for (acceptor_id, display_name, endpoint_addr) in &new_contacts {
                            // Connect transport to acceptor
                            if !endpoint_addr.is_empty() {
                                if let Err(e) =
                                    inner.connect_to_bootstrap(endpoint_addr).await
                                {
                                    tracing::debug!(
                                        error = %e,
                                        "Transport connect to acceptor (non-fatal)"
                                    );
                                }
                            }

                            tracing::info!(
                                acceptor = %hex::encode(&acceptor_id[..8]),
                                name = ?display_name,
                                "Connection accept received"
                            );
                        }

                        // Store accepts for later contact processing via process_pending_accepts()
                        if let Some(mut entry) = pending.get_mut(&interface_id) {
                            entry.status = ConnectionStatus::AcceptReceived;
                        }

                        // Persist updated status
                        if let Err(e) = IndrasNetwork::persist_pending_connections_inner(&inner, &pending) {
                            tracing::warn!(error = %e, "Failed to persist pending connections after accept");
                        }

                        // Schedule deferred cleanup
                        let cleanup_inner = Arc::clone(&inner);
                        let cleanup_pending = Arc::clone(&pending);
                        let cleanup_id = interface_id;
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            if let Err(e) = cleanup_inner.leave_interface(&cleanup_id).await {
                                tracing::debug!(error = %e, "Connection realm cleanup");
                            }
                            cleanup_pending.remove(&cleanup_id);
                            // Persist after cleanup removal
                            if let Err(e) = IndrasNetwork::persist_pending_connections_inner(&cleanup_inner, &cleanup_pending) {
                                tracing::warn!(error = %e, "Failed to persist pending connections after cleanup");
                            }
                        });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!(lagged = n, "Connection listener lagged, continuing");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!("Connection listener channel closed");
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    /// Process pending connection accepts — add acceptors as confirmed contacts.
    ///
    /// Called periodically or after receiving a connection accept notification.
    /// This is separated from the listener to avoid holding non-Send types
    /// across await points in the spawned task.
    pub async fn process_pending_accepts(&self) -> Result<usize> {
        let contacts = self.join_contacts_realm().await?;
        let mut processed = 0;

        // Collect realms with AcceptReceived status
        let received: Vec<RealmId> = self
            .pending_connections
            .iter()
            .filter(|e| e.status == ConnectionStatus::AcceptReceived)
            .map(|e| *e.key())
            .collect();

        for realm_id in received {
            let doc: Document<ConnectionDocument> = match Document::new(
                realm_id,
                "connection".to_string(),
                Arc::clone(&self.inner),
            )
            .await
            {
                Ok(d) => d,
                Err(_) => continue,
            };

            let data = doc.read().await;
            let accepts = data.all_accepts().clone();
            drop(data);

            let my_id = self.id();
            for (acceptor_id, accept) in &accepts {
                if *acceptor_id == my_id {
                    continue;
                }

                if !contacts.is_contact(acceptor_id).await {
                    let _ = contacts
                        .add_contact_with_name(*acceptor_id, accept.display_name.clone())
                        .await;
                }
                let _ = contacts.confirm_contact(acceptor_id).await;
                processed += 1;

                tracing::info!(
                    acceptor = %hex::encode(&acceptor_id[..8]),
                    name = ?accept.display_name,
                    "Contact confirmed from connection accept"
                );
            }

            // Mark complete
            if let Some(mut entry) = self.pending_connections.get_mut(&realm_id) {
                entry.status = ConnectionStatus::Complete;
            }
        }

        // Persist updated statuses
        if processed > 0 {
            if let Err(e) = self.persist_pending_connections() {
                tracing::warn!(error = %e, "Failed to persist pending connections after processing accepts");
            }
        }

        Ok(processed)
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

        // Create the home realm interface
        let (_interface_id, _invite_key) = self
            .inner
            .create_interface_with_id(realm_id, Some("Home"))
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
    // Connection persistence
    // ============================================================

    /// Key for persisting pending connections in redb.
    const PENDING_CONNECTIONS_KEY: &'static [u8] = b"pending_connections:v1";

    /// Persist all pending connections to redb storage.
    fn persist_pending_connections(&self) -> Result<()> {
        Self::persist_pending_connections_inner(&self.inner, &self.pending_connections)
    }

    /// Inner persist helper that works with Arc references (for use in spawned tasks).
    fn persist_pending_connections_inner(
        node: &IndrasNode,
        pending: &DashMap<RealmId, PendingConnection>,
    ) -> Result<()> {
        let map: Vec<(RealmId, PendingConnection)> = pending
            .iter()
            .map(|e| (*e.key(), e.value().clone()))
            .collect();
        let data = postcard::to_allocvec(&map)?;
        node.storage()
            .interface_store()
            .set_document_data(Self::PENDING_CONNECTIONS_KEY, &data)?;
        Ok(())
    }

    /// Restore pending connections from redb storage on startup.
    ///
    /// Skips expired (>7 days) and completed connections.
    fn restore_pending_connections(&self) -> Result<()> {
        if let Ok(Some(data)) = self
            .inner
            .storage()
            .interface_store()
            .get_document_data(Self::PENDING_CONNECTIONS_KEY)
        {
            if let Ok(entries) = postcard::from_bytes::<Vec<(RealmId, PendingConnection)>>(&data) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let seven_days_ms = 7 * 24 * 60 * 60 * 1000u64;
                for (id, conn) in entries {
                    // Skip expired or completed
                    if now.saturating_sub(conn.created_at) > seven_days_ms {
                        continue;
                    }
                    if conn.status == ConnectionStatus::Complete {
                        continue;
                    }
                    self.pending_connections.insert(id, conn);
                }
            }
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
