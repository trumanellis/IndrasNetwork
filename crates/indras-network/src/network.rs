//! IndrasNetwork - the main SDK entry point.
//!
//! Provides a high-level API for building P2P applications on Indra's Network.

use crate::config::{NetworkBuilder, NetworkConfig, Preset};
use crate::contacts::{contacts_realm_id, ContactsRealm};
use crate::error::{IndraError, Result};
use crate::invite::InviteCode;
use crate::member::{Member, MemberId};
use crate::realm::Realm;

use dashmap::DashMap;
use indras_core::InterfaceId;
use indras_node::IndrasNode;
use indras_storage::CompositeStorage;
use indras_transport::IrohIdentity;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unique identifier for a realm.
pub type RealmId = InterfaceId;

/// The main entry point for the Indra SDK.
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
    pub async fn with_config(config: NetworkConfig) -> Result<Self> {
        let node_config = config.to_node_config();
        let node = IndrasNode::new(node_config).await?;

        let identity = Member::new(*node.identity());

        Ok(Self {
            inner: Arc::new(node),
            realms: Arc::new(DashMap::new()),
            peer_realms: Arc::new(DashMap::new()),
            contacts_realm: RwLock::new(None),
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
        self.config.display_name = Some(name.into());
        // TODO: Persist to storage and broadcast to realms
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
    /// # Arguments
    ///
    /// * `peers` - The set of member IDs that define this realm (must include yourself)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Tag friends to access a realm
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

        // Check if already loaded
        if let Some(state) = self.realms.get(&realm_id) {
            return Ok(Realm::from_id(realm_id, state.name.clone(), Arc::clone(&self.inner)));
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
