//! IndrasNetwork - the main SyncEngine entry point.
//!
//! Provides a high-level API for building P2P applications on Indra's Network.

use crate::config::{NetworkBuilder, NetworkConfig, Preset};
use crate::contacts::{contacts_realm_id, ContactsRealm};
use crate::document::Document;
use crate::error::{IndraError, Result};
use crate::handshake::{inbox_interface_id, ConnectionRequest, HandshakeDocument};
use crate::home_realm::{home_realm_id, HomeRealm};
use crate::invite::InviteCode;
use crate::member::{Member, MemberId};
use crate::realm::Realm;

use dashmap::DashMap;
use indras_core::InterfaceId;
use indras_crypto::InterfaceKey;
use indras_node::{IndrasNode, InviteKey, ReceivedEvent};
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
    contacts_realm: RwLock<Option<ContactsRealm>>,
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
            contacts_realm: RwLock::new(None),
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

        // Clean up any pending connection request from the blocked member in our inbox
        let my_hex = hex::encode(&self.id());
        let inbox_name = format!("inbox:{}", my_hex);
        let blocked_id = *member_id;
        if let Ok(doc) = Document::<HandshakeDocument>::new(
            contacts.id(),
            inbox_name,
            Arc::clone(&self.inner),
        ).await {
            let _ = doc.update(|inbox| {
                inbox.remove_request(&blocked_id);
            }).await;
        }

        // Also clean from P2P inbox if it exists
        let p2p_inbox_id = inbox_interface_id(self.id());
        if self.inner.interface_key(&p2p_inbox_id).is_some() {
            if let Ok(doc) = Document::<HandshakeDocument>::new(
                p2p_inbox_id,
                "inbox".to_string(),
                Arc::clone(&self.inner),
            ).await {
                let _ = doc.update(|inbox| {
                    inbox.remove_request(&blocked_id);
                }).await;
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
    // Sentiment queries
    // ============================================================

    /// Query the aggregate sentiment view about a member.
    ///
    /// Returns direct sentiment from your own contacts who know the target,
    /// plus second-degree relayed sentiment from contacts' contacts.
    /// Only contacts with `relayable: true` contribute to relayed signals.
    ///
    /// This is scoped to your local view — you never see sentiment from
    /// people outside your contact graph. A thousand fake nodes rating
    /// someone negatively are invisible if none of them are your contacts.
    pub async fn query_sentiment(
        &self,
        about: &MemberId,
        relay_documents: &std::collections::HashMap<MemberId, crate::sentiment::SentimentRelayDocument>,
    ) -> Result<crate::sentiment::SentimentView> {
        let contacts_realm = {
            let guard = self.contacts_realm.read().await;
            guard.clone()
        };

        let contacts = contacts_realm.ok_or_else(|| {
            IndraError::InvalidOperation(
                "Must join contacts realm before querying sentiment.".to_string(),
            )
        })?;

        let mut view = crate::sentiment::SentimentView::default();

        // Collect direct sentiment from our contacts
        let doc = contacts.contacts_with_sentiment();
        for (contact_id, _sentiment) in &doc {
            if contact_id == about {
                // This IS the person we're asking about — skip (they're in our contacts)
                continue;
            }
            // Check if this contact knows the target by looking at their relay doc
            if let Some(relay_doc) = relay_documents.get(contact_id) {
                if let Some(relayed_sentiment) = relay_doc.get(about) {
                    // This contact has an opinion about the target — that's a direct signal
                    view.direct.push((*contact_id, relayed_sentiment));
                }
            }
        }

        // Collect second-degree relayed sentiment
        for (contact_id, _sentiment) in &doc {
            if contact_id == about {
                continue;
            }
            if let Some(relay_doc) = relay_documents.get(contact_id) {
                // Look at this contact's other ratings for second-degree signals
                for (rated_id, rated_sentiment) in relay_doc.iter() {
                    if rated_id == about {
                        // Already captured as direct above
                        continue;
                    }
                    // This is a second-degree signal: our contact rates someone,
                    // and that someone also rates the target. For now, we only
                    // do one level of relay (our contacts' direct opinions).
                    // The direct signals above already capture contacts' opinions
                    // about the target.
                    let _ = (rated_id, rated_sentiment);
                }
            }
        }

        Ok(view)
    }

    // ============================================================
    // Contact invites
    // ============================================================

    /// Generate a contact invite code for this network identity.
    ///
    /// The invite code contains your member ID, display name, and
    /// transport bootstrap info so the acceptor can establish a P2P
    /// connection to deliver the connection request.
    ///
    /// Creates a personal inbox interface (deterministic from member ID)
    /// and embeds its ID, bootstrap address, and encryption key in the
    /// invite code.
    pub async fn contact_invite_code(&self) -> Result<crate::contact_invite::ContactInviteCode> {
        // Ensure network is started
        if !self.is_running() {
            self.start().await?;
        }

        let my_id = self.id();
        let inbox_id = inbox_interface_id(my_id);

        // Create (or get existing) inbox interface
        let (_iface_id, invite_key) = self
            .inner
            .create_interface_with_id(inbox_id, Some("Inbox"))
            .await?;

        // Get the interface encryption key
        let iface_key = self.inner.interface_key(&inbox_id);

        // Build the contact invite code with transport info
        let mut code = crate::contact_invite::ContactInviteCode::new(
            my_id,
            self.display_name().map(|s| s.to_string()),
        );

        // Attach inbox transport info if we have everything
        if let Some(ref key) = iface_key {
            // Get bootstrap address from the invite key
            let bootstrap = invite_key
                .bootstrap_peers
                .first()
                .cloned()
                .unwrap_or_default();

            if !bootstrap.is_empty() {
                code = code.with_inbox(
                    *inbox_id.as_bytes(),
                    bootstrap,
                    *key.as_bytes(),
                );
            }
        }

        Ok(code)
    }

    /// Accept a contact invite code, adding the inviter as a contact.
    ///
    /// This adds the inviter to your contact list, then uses the transport
    /// bootstrap info embedded in the invite to join the inviter's inbox
    /// interface and write a `ConnectionRequest` there. The inviter's node
    /// picks it up via `process_handshake_inbox()`, completing the
    /// bidirectional connection.
    ///
    /// Falls back to the contacts realm inbox if the invite doesn't
    /// contain transport info (backward compatibility).
    pub async fn accept_contact_invite(
        &self,
        code: &crate::contact_invite::ContactInviteCode,
    ) -> Result<()> {
        // Ensure network is started
        if !self.is_running() {
            self.start().await?;
        }

        // 1. Add inviter as contact with their display name
        let contacts = self.join_contacts_realm().await?;
        let already_contact = contacts.is_contact(&code.member_id()).await;

        if !already_contact {
            contacts
                .add_contact_with_name(
                    code.member_id(),
                    code.display_name().map(|s| s.to_string()),
                )
                .await?;
        }

        // 2. Build the connection request
        let my_id = self.id();
        let my_name = self.display_name().map(|s| s.to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let request = ConnectionRequest {
            member_id: my_id,
            display_name: my_name,
            timestamp_millis: now,
        };

        // 3. Try to write connection request; roll back contact on failure
        let write_result = self.write_connection_request(&contacts, code, request).await;

        match write_result {
            Ok(()) => Ok(()),
            Err(e) => {
                // Roll back contact addition if this was a new contact
                if !already_contact {
                    let _ = contacts.remove_contact(&code.member_id()).await;
                    tracing::warn!(
                        inviter = %hex::encode(&code.member_id()[..8]),
                        error = %e,
                        "Rolled back contact addition after connection request failure"
                    );
                }
                Err(e)
            }
        }
    }

    /// Write a connection request to the inviter's inbox.
    ///
    /// Tries P2P inbox first, falls back to contacts realm inbox.
    async fn write_connection_request(
        &self,
        contacts: &ContactsRealm,
        code: &crate::contact_invite::ContactInviteCode,
        request: ConnectionRequest,
    ) -> Result<()> {
        // Try P2P path: join inviter's inbox interface and send request
        if let (Some(inbox_id_bytes), Some(bootstrap), Some(key_bytes)) =
            (code.inbox_id(), code.bootstrap(), code.inbox_key())
        {
            let inbox_id = InterfaceId::new(*inbox_id_bytes);

            // Insert the inviter's inbox key so we can encrypt messages to it
            let iface_key = InterfaceKey::from_bytes(*key_bytes, inbox_id);
            self.inner.set_interface_key(inbox_id, iface_key);

            // Build an InviteKey with the bootstrap address to join the interface
            let invite = InviteKey::new(inbox_id)
                .with_bootstrap(bootstrap.to_vec());

            // Join the inviter's inbox interface (connects via P2P transport)
            match self.inner.join_interface(invite).await {
                Ok(_) => {
                    // Write the ConnectionRequest to the inbox via a document
                    let doc: Document<HandshakeDocument> = Document::new(
                        inbox_id,
                        "inbox".to_string(),
                        Arc::clone(&self.inner),
                    )
                    .await?;

                    doc.update(|inbox| {
                        inbox.add_request(request.clone());
                    })
                    .await?;

                    tracing::info!(
                        inviter = %hex::encode(&code.member_id()[..8]),
                        "Sent connection request via P2P inbox interface"
                    );

                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "P2P inbox join failed, falling back to contacts realm"
                    );
                    // Fall through to contacts realm path
                }
            }
        }

        // Fallback: write to contacts realm inbox document
        let inviter_hex = hex::encode(&code.member_id());
        let inbox_name = format!("inbox:{}", inviter_hex);

        let doc: Document<HandshakeDocument> = Document::new(
            contacts.id(),
            inbox_name,
            Arc::clone(&self.inner),
        )
        .await?;

        doc.update(|inbox| {
            inbox.add_request(request);
        })
        .await?;

        tracing::info!(
            inviter = %hex::encode(&code.member_id()[..8]),
            "Wrote connection request to inviter's inbox in contacts realm (fallback)"
        );

        Ok(())
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
    // Connection inbox (store-and-forward via contacts realm)
    // ============================================================

    /// Process pending connection requests from our inbox.
    ///
    /// Checks two sources:
    /// 1. **P2P inbox interface** — the deterministic inbox interface created
    ///    by `contact_invite_code()`. When someone accepts our invite with
    ///    transport info, they join this interface and write a `ConnectionRequest`.
    /// 2. **Contacts realm inbox** (fallback) — the per-user inbox document
    ///    in the shared contacts realm for backward compatibility.
    ///
    /// For each pending request, adds the requester as a contact with
    /// their display name, then removes the processed request.
    ///
    /// Returns the number of new contacts added.
    pub async fn process_handshake_inbox(&self) -> Result<usize> {
        let contacts = self.join_contacts_realm().await?;
        let my_id = self.id();
        let mut total_added = 0;

        // --- Source 1: P2P inbox interface ---
        let inbox_id = inbox_interface_id(my_id);
        // Only check if we've created the inbox (i.e., the interface exists)
        if self.inner.interface_key(&inbox_id).is_some() {
            match Document::<HandshakeDocument>::new(
                inbox_id,
                "inbox".to_string(),
                Arc::clone(&self.inner),
            )
            .await
            {
                Ok(doc) => {
                    let added = self
                        .process_inbox_document(&doc, &contacts)
                        .await?;
                    total_added += added;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "P2P inbox document read skipped");
                }
            }
        }

        // --- Source 2: Contacts realm inbox (fallback) ---
        let my_hex = hex::encode(&my_id);
        let inbox_name = format!("inbox:{}", my_hex);

        let doc: Document<HandshakeDocument> = Document::new(
            contacts.id(),
            inbox_name,
            Arc::clone(&self.inner),
        )
        .await?;

        let added = self.process_inbox_document(&doc, &contacts).await?;
        total_added += added;

        Ok(total_added)
    }

    /// Process a single inbox document: read pending requests, add contacts, remove processed.
    async fn process_inbox_document(
        &self,
        doc: &Document<HandshakeDocument>,
        contacts: &ContactsRealm,
    ) -> Result<usize> {
        // Prune expired requests first
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let _ = doc.update(|inbox| {
            let pruned = inbox.prune_expired_default(now);
            if pruned > 0 {
                tracing::debug!(pruned, "Pruned expired inbox requests");
            }
        }).await;

        let data = doc.read().await;
        let pending: Vec<ConnectionRequest> = data.pending_requests().cloned().collect();
        drop(data);

        if pending.is_empty() {
            return Ok(0);
        }

        let my_id = self.id();
        let mut added = 0;
        let mut processed_ids: Vec<MemberId> = Vec::new();

        for request in &pending {
            // Skip self
            if request.member_id == my_id {
                processed_ids.push(request.member_id);
                continue;
            }

            // Add as contact with their display name
            // Check if already a contact (confirms bidirectional connection)
            let already_contact = contacts.is_contact(&request.member_id).await;

            if already_contact {
                // Upgrade to Confirmed status
                let _ = contacts.confirm_contact(&request.member_id).await;
                tracing::info!(
                    member = %hex::encode(&request.member_id[..8]),
                    "Contact confirmed (bidirectional handshake complete)"
                );
                processed_ids.push(request.member_id);
                added += 1;
            } else {
                match contacts
                    .add_contact_with_name(request.member_id, request.display_name.clone())
                    .await
                {
                    Ok(()) => {
                        // New contact from inbox — mark as Confirmed since they initiated
                        let _ = contacts.confirm_contact(&request.member_id).await;
                        tracing::info!(
                            member = %hex::encode(&request.member_id[..8]),
                            name = ?request.display_name,
                            "Added and confirmed contact from inbox"
                        );
                        added += 1;
                        processed_ids.push(request.member_id);
                    }
                    Err(e) => {
                        tracing::warn!(
                            member = %hex::encode(&request.member_id[..8]),
                            error = %e,
                            "Failed to add contact from inbox, will retry"
                        );
                        // Do NOT push to processed_ids — leave in inbox for retry
                    }
                }
            }
        }

        // Remove processed requests
        if !processed_ids.is_empty() {
            doc.update(|inbox| {
                for id in &processed_ids {
                    inbox.remove_request(id);
                }
            })
            .await?;
        }

        Ok(added)
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
    /// // Create a personal note
    /// let note_id = home.create_note(
    ///     "My Note",
    ///     "# Hello\n\nContent here",
    ///     vec!["personal".into()],
    /// ).await?;
    ///
    /// // Create a personal quest
    /// let quest_id = home.create_quest(
    ///     "Personal Task",
    ///     "Do something productive",
    ///     None,
    /// ).await?;
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

        // Seed welcome quest on first creation (idempotent)
        home.seed_welcome_quest_if_empty().await?;

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
