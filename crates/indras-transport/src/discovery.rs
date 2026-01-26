//! Peer discovery using iroh-gossip
//!
//! Provides presence announcement and peer discovery through gossip protocol.
//! Supports both global presence discovery and per-realm peer discovery.

use std::time::Instant;

use dashmap::DashMap;
use iroh::PublicKey;
use iroh_gossip::Gossip;
use iroh_gossip::api::GossipTopic;
use iroh_gossip::proto::TopicId;
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, info, instrument, warn};

use indras_core::InterfaceId;
use indras_core::identity::PeerIdentity;

use crate::identity::IrohIdentity;
use crate::protocol::{
    InterfaceJoinMessage, InterfaceLeaveMessage, IntroductionRequestMessage,
    IntroductionResponseMessage, PeerIntroductionMessage, PresenceInfo, RealmPeerInfo,
    WireMessage, frame_message, parse_framed_message,
};

/// Configuration for peer discovery
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// How often to announce presence (milliseconds)
    pub announce_interval_ms: u64,
    /// Timeout for considering a peer offline (milliseconds)
    pub peer_timeout_ms: u64,
    /// Maximum peers to track
    pub max_tracked_peers: usize,
    /// Topic ID for presence gossip
    pub topic_id: TopicId,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            announce_interval_ms: 30_000,
            peer_timeout_ms: 90_000,
            max_tracked_peers: 1000,
            topic_id: Self::default_topic_id(),
        }
    }
}

impl DiscoveryConfig {
    /// Default topic ID for presence gossip
    pub fn default_topic_id() -> TopicId {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        b"indras/presence/v1".hash(&mut hasher);
        let hash = hasher.finish();
        let mut topic = [0u8; 32];
        topic[..8].copy_from_slice(&hash.to_le_bytes());
        topic[8..16].copy_from_slice(&hash.to_be_bytes());
        TopicId::from(topic)
    }
}

/// Errors in peer discovery
#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("Failed to join gossip topic: {0}")]
    JoinError(String),

    #[error("Failed to broadcast: {0}")]
    BroadcastError(String),

    #[error("Failed to serialize message: {0}")]
    SerializationError(String),

    #[error("Failed to deserialize message: {0}")]
    DeserializationError(String),

    #[error("Gossip error: {0}")]
    GossipError(String),

    #[error("Discovery service not running")]
    NotRunning,
}

/// Events emitted by the discovery service
#[derive(Debug, Clone)]
pub enum PeerEvent {
    /// A new peer was discovered (global presence)
    Discovered(PeerInfo),
    /// A peer's information was updated (global presence)
    Updated(PeerInfo),
    /// A peer went offline (timed out)
    Lost(IrohIdentity),

    // ========== Realm-specific discovery events ==========
    /// A new peer joined a realm
    RealmPeerJoined {
        /// The realm/interface the peer joined
        interface_id: InterfaceId,
        /// Information about the peer
        peer_info: RealmPeerInfo,
    },
    /// A peer left a realm
    RealmPeerLeft {
        /// The realm/interface the peer left
        interface_id: InterfaceId,
        /// The peer that left
        peer_id: IrohIdentity,
    },
    /// A peer requested introductions (we should respond with known members)
    IntroductionRequested {
        /// The realm/interface
        interface_id: InterfaceId,
        /// The peer requesting introductions
        requester: IrohIdentity,
        /// Peers they already know about
        known_peers: Vec<IrohIdentity>,
    },
}

/// Information about a discovered peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// The peer's identity
    pub identity: IrohIdentity,
    /// Last known presence info
    pub presence: PresenceInfo,
    /// When we last heard from this peer
    pub last_seen_millis: i64,
}

/// Rate limit duration for introduction responses (30 seconds)
const INTRODUCTION_RATE_LIMIT_SECS: u64 = 30;

/// Peer discovery service using iroh-gossip
pub struct DiscoveryService {
    /// Gossip handle
    gossip: Gossip,
    /// Topic handle for global presence
    topic: RwLock<Option<GossipTopic>>,
    /// Known peers (global presence)
    known_peers: DashMap<IrohIdentity, PeerInfo>,
    /// Our own identity
    local_identity: IrohIdentity,
    /// Configuration
    config: DiscoveryConfig,
    /// Event broadcaster
    event_tx: broadcast::Sender<PeerEvent>,
    /// Running state
    running: RwLock<bool>,

    // ========== Per-realm discovery ==========
    /// Topics for each realm we're a member of
    realm_topics: DashMap<InterfaceId, GossipTopic>,
    /// Known peers per realm (InterfaceId -> (PeerId -> PeerInfo))
    realm_peers: DashMap<InterfaceId, DashMap<IrohIdentity, RealmPeerInfo>>,
    /// Rate limiting for introduction responses: (InterfaceId, PeerId) -> last_response_time
    introduction_rate_limit: DashMap<(InterfaceId, IrohIdentity), Instant>,
    /// Our PQ keys (set via set_pq_keys)
    pq_encapsulation_key: RwLock<Option<Vec<u8>>>,
    pq_verifying_key: RwLock<Option<Vec<u8>>>,
    /// Our display name
    display_name: RwLock<Option<String>>,
}

impl DiscoveryService {
    /// Create a new discovery service
    pub fn new(gossip: Gossip, local_identity: IrohIdentity, config: DiscoveryConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            gossip,
            topic: RwLock::new(None),
            known_peers: DashMap::new(),
            local_identity,
            config,
            event_tx,
            running: RwLock::new(false),
            realm_topics: DashMap::new(),
            realm_peers: DashMap::new(),
            introduction_rate_limit: DashMap::new(),
            pq_encapsulation_key: RwLock::new(None),
            pq_verifying_key: RwLock::new(None),
            display_name: RwLock::new(None),
        }
    }

    /// Set our PQ keys for inclusion in join messages
    pub async fn set_pq_keys(&self, encapsulation_key: Vec<u8>, verifying_key: Vec<u8>) {
        *self.pq_encapsulation_key.write().await = Some(encapsulation_key);
        *self.pq_verifying_key.write().await = Some(verifying_key);
    }

    /// Set our display name
    pub async fn set_display_name(&self, name: impl Into<String>) {
        *self.display_name.write().await = Some(name.into());
    }

    /// Start the discovery service
    ///
    /// Joins the gossip topic and begins listening for peer announcements.
    #[instrument(skip(self, bootstrap_peers), fields(local_peer = %self.local_identity.short_id(), bootstrap_count = bootstrap_peers.len()))]
    pub async fn start(&self, bootstrap_peers: Vec<PublicKey>) -> Result<(), DiscoveryError> {
        let mut running = self.running.write().await;
        if *running {
            debug!("Discovery service already running");
            return Ok(());
        }

        info!("Starting discovery service");

        // Convert PublicKey to the format gossip expects
        let bootstrap: Vec<_> = bootstrap_peers.into_iter().collect();

        // Join the gossip topic
        let topic = self
            .gossip
            .subscribe(self.config.topic_id, bootstrap)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to join gossip topic");
                DiscoveryError::JoinError(e.to_string())
            })?;

        *self.topic.write().await = Some(topic);
        *running = true;

        info!("Joined gossip topic, announcing presence");

        // Announce our presence immediately
        self.announce_presence().await?;

        Ok(())
    }

    /// Stop the discovery service
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        if !*running {
            return;
        }

        info!("Stopping discovery service");
        *running = false;
        *self.topic.write().await = None;

        // Clean up all realm topics
        self.realm_topics.clear();
        self.realm_peers.clear();
        self.introduction_rate_limit.clear();
    }

    // ========================================================================
    // Per-Realm Discovery Methods
    // ========================================================================

    /// Generate a deterministic topic ID for a realm/interface
    ///
    /// The topic ID is derived from the interface ID to ensure all members
    /// of the same realm subscribe to the same gossip topic.
    pub fn topic_for_interface(interface_id: &InterfaceId) -> TopicId {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        b"indras/realm/v1/".hash(&mut hasher);
        interface_id.as_bytes().hash(&mut hasher);
        let hash = hasher.finish();

        let mut topic = [0u8; 32];
        // Use interface ID bytes as base, with hash for uniqueness
        topic[..16].copy_from_slice(&interface_id.as_bytes()[..16]);
        topic[16..24].copy_from_slice(&hash.to_le_bytes());
        topic[24..32].copy_from_slice(&hash.to_be_bytes());
        TopicId::from(topic)
    }

    /// Join a realm's gossip topic for peer discovery
    ///
    /// Subscribes to the realm's topic and broadcasts our join message
    /// with PQ keys to announce our presence to existing members.
    #[instrument(skip(self, bootstrap_peers), fields(interface = %hex::encode(&interface_id.as_bytes()[..8])))]
    pub async fn join_realm_topic(
        &self,
        interface_id: InterfaceId,
        bootstrap_peers: Vec<PublicKey>,
    ) -> Result<(), DiscoveryError> {
        // Check if already joined
        if self.realm_topics.contains_key(&interface_id) {
            debug!("Already joined realm topic");
            return Ok(());
        }

        let topic_id = Self::topic_for_interface(&interface_id);
        debug!(topic = %hex::encode(&topic_id.as_bytes()[..8]), "Joining realm gossip topic");

        // Subscribe to the realm topic
        let topic = self
            .gossip
            .subscribe(topic_id, bootstrap_peers)
            .await
            .map_err(|e| DiscoveryError::JoinError(e.to_string()))?;

        // Store the topic handle
        self.realm_topics.insert(interface_id, topic);

        // Initialize peer tracking for this realm
        self.realm_peers
            .entry(interface_id)
            .or_insert_with(DashMap::new);

        // Broadcast our join message with PQ keys
        self.broadcast_interface_join(interface_id).await?;

        // Send an introduction request to discover existing members
        self.send_introduction_request(interface_id).await?;

        info!("Joined realm gossip topic");
        Ok(())
    }

    /// Leave a realm's gossip topic
    ///
    /// Broadcasts a leave message and cleans up all state for this realm.
    #[instrument(skip(self), fields(interface = %hex::encode(&interface_id.as_bytes()[..8])))]
    pub async fn leave_realm_topic(&self, interface_id: InterfaceId) -> Result<(), DiscoveryError> {
        // Broadcast leave message before unsubscribing
        if let Err(e) = self.broadcast_interface_leave(interface_id).await {
            warn!(error = %e, "Failed to broadcast leave message");
        }

        // Remove topic handle (this unsubscribes)
        self.realm_topics.remove(&interface_id);

        // Clean up peer tracking
        self.realm_peers.remove(&interface_id);

        // Clean up rate limits for this realm
        self.introduction_rate_limit
            .retain(|(iid, _), _| *iid != interface_id);

        info!("Left realm gossip topic");
        Ok(())
    }

    /// Broadcast a message to all members of a realm
    pub async fn broadcast_to_realm(
        &self,
        interface_id: &InterfaceId,
        msg: &WireMessage,
    ) -> Result<(), DiscoveryError> {
        let mut topic = self
            .realm_topics
            .get_mut(interface_id)
            .ok_or(DiscoveryError::NotRunning)?;

        let framed =
            frame_message(msg).map_err(|e| DiscoveryError::SerializationError(e.to_string()))?;

        topic
            .broadcast(framed)
            .await
            .map_err(|e| DiscoveryError::BroadcastError(e.to_string()))?;

        Ok(())
    }

    /// Broadcast our InterfaceJoin message with PQ keys
    async fn broadcast_interface_join(
        &self,
        interface_id: InterfaceId,
    ) -> Result<(), DiscoveryError> {
        let mut join_msg = InterfaceJoinMessage::new(interface_id);

        // Add display name if set
        if let Some(name) = self.display_name.read().await.as_ref() {
            join_msg = join_msg.with_name(name.clone());
        }

        // Add PQ keys if set
        if let Some(key) = self.pq_encapsulation_key.read().await.as_ref() {
            join_msg = join_msg.with_pq_encapsulation_key(key.clone());
        }
        if let Some(key) = self.pq_verifying_key.read().await.as_ref() {
            join_msg = join_msg.with_pq_verifying_key(key.clone());
        }

        let msg = WireMessage::InterfaceJoin(join_msg);
        self.broadcast_to_realm(&interface_id, &msg).await?;

        debug!("Broadcast InterfaceJoin with PQ keys");
        Ok(())
    }

    /// Broadcast InterfaceLeave message
    async fn broadcast_interface_leave(
        &self,
        interface_id: InterfaceId,
    ) -> Result<(), DiscoveryError> {
        let leave_msg = InterfaceLeaveMessage::new(interface_id);
        let msg = WireMessage::InterfaceLeave(leave_msg);
        self.broadcast_to_realm(&interface_id, &msg).await
    }

    /// Send an IntroductionRequest to discover existing members
    async fn send_introduction_request(
        &self,
        interface_id: InterfaceId,
    ) -> Result<(), DiscoveryError> {
        // Get list of peers we already know about
        let known_peers: Vec<IrohIdentity> = self
            .realm_peers
            .get(&interface_id)
            .map(|peers| peers.iter().map(|e| *e.key()).collect())
            .unwrap_or_default();

        let request = IntroductionRequestMessage::new(interface_id).with_known_peers(known_peers);
        let msg = WireMessage::IntroductionRequest(request);
        self.broadcast_to_realm(&interface_id, &msg).await?;

        debug!("Sent IntroductionRequest");
        Ok(())
    }

    /// Broadcast a PeerIntroduction for a new member (so others learn about them)
    pub async fn broadcast_peer_introduction(
        &self,
        interface_id: InterfaceId,
        peer_info: RealmPeerInfo,
    ) -> Result<(), DiscoveryError> {
        let intro = PeerIntroductionMessage::new(interface_id, peer_info);
        let msg = WireMessage::PeerIntroduction(intro);
        self.broadcast_to_realm(&interface_id, &msg).await
    }

    /// Send an IntroductionResponse to a specific peer (rate-limited)
    ///
    /// Returns Ok(true) if response was sent, Ok(false) if rate-limited.
    pub async fn send_introduction_response(
        &self,
        interface_id: InterfaceId,
        requester: IrohIdentity,
        known_by_requester: &[IrohIdentity],
    ) -> Result<bool, DiscoveryError> {
        let rate_key = (interface_id, requester);

        // Check rate limit
        if let Some(last_response) = self.introduction_rate_limit.get(&rate_key) {
            if last_response.elapsed().as_secs() < INTRODUCTION_RATE_LIMIT_SECS {
                debug!(
                    requester = %requester.short_id(),
                    "Rate-limited introduction response"
                );
                return Ok(false);
            }
        }

        // Get members the requester doesn't know about
        let members: Vec<RealmPeerInfo> = self
            .realm_peers
            .get(&interface_id)
            .map(|peers| {
                peers
                    .iter()
                    .filter(|e| !known_by_requester.contains(e.key()) && *e.key() != requester)
                    .map(|e| e.value().clone())
                    .collect()
            })
            .unwrap_or_default();

        // Add ourselves if not known
        if !known_by_requester.contains(&self.local_identity) {
            let mut our_info = RealmPeerInfo::new(self.local_identity);
            if let Some(name) = self.display_name.read().await.as_ref() {
                our_info = our_info.with_name(name.clone());
            }
            if let Some(key) = self.pq_encapsulation_key.read().await.as_ref() {
                our_info = our_info.with_pq_encapsulation_key(key.clone());
            }
            if let Some(key) = self.pq_verifying_key.read().await.as_ref() {
                our_info = our_info.with_pq_verifying_key(key.clone());
            }

            let mut all_members = vec![our_info];
            all_members.extend(members);

            let response = IntroductionResponseMessage::new(interface_id, all_members);
            let msg = WireMessage::IntroductionResponse(response);
            self.broadcast_to_realm(&interface_id, &msg).await?;
        } else if !members.is_empty() {
            let response = IntroductionResponseMessage::new(interface_id, members);
            let msg = WireMessage::IntroductionResponse(response);
            self.broadcast_to_realm(&interface_id, &msg).await?;
        }

        // Update rate limit
        self.introduction_rate_limit.insert(rate_key, Instant::now());

        debug!(requester = %requester.short_id(), "Sent IntroductionResponse");
        Ok(true)
    }

    /// Get known peers for a realm
    pub fn realm_members(&self, interface_id: &InterfaceId) -> Vec<RealmPeerInfo> {
        self.realm_peers
            .get(interface_id)
            .map(|peers| peers.iter().map(|e| e.value().clone()).collect())
            .unwrap_or_default()
    }

    /// Get a specific peer's info in a realm
    pub fn get_realm_peer(
        &self,
        interface_id: &InterfaceId,
        peer_id: &IrohIdentity,
    ) -> Option<RealmPeerInfo> {
        self.realm_peers
            .get(interface_id)
            .and_then(|peers| peers.get(peer_id).map(|e| e.value().clone()))
    }

    /// Check if we're subscribed to a realm's gossip topic
    pub fn is_in_realm(&self, interface_id: &InterfaceId) -> bool {
        self.realm_topics.contains_key(interface_id)
    }

    /// Announce our presence to the network
    pub async fn announce_presence(&self) -> Result<(), DiscoveryError> {
        let mut topic_guard = self.topic.write().await;
        let topic = topic_guard.as_mut().ok_or(DiscoveryError::NotRunning)?;

        let presence = PresenceInfo::new(self.local_identity);
        let msg = WireMessage::PresenceAnnounce(presence);

        let framed =
            frame_message(&msg).map_err(|e| DiscoveryError::SerializationError(e.to_string()))?;

        topic
            .broadcast(framed)
            .await
            .map_err(|e| DiscoveryError::BroadcastError(e.to_string()))?;

        debug!(identity = %self.local_identity.short_id(), "Announced presence");
        Ok(())
    }

    /// Query for peers (broadcasts a presence query)
    pub async fn query_peers(&self) -> Result<(), DiscoveryError> {
        let mut topic_guard = self.topic.write().await;
        let topic = topic_guard.as_mut().ok_or(DiscoveryError::NotRunning)?;

        let msg = WireMessage::PresenceQuery;
        let framed =
            frame_message(&msg).map_err(|e| DiscoveryError::SerializationError(e.to_string()))?;

        topic
            .broadcast(framed)
            .await
            .map_err(|e| DiscoveryError::BroadcastError(e.to_string()))?;

        debug!("Sent presence query");
        Ok(())
    }

    /// Get all known peers
    pub fn known_peers(&self) -> Vec<PeerInfo> {
        self.known_peers.iter().map(|e| e.value().clone()).collect()
    }

    /// Get info for a specific peer
    pub fn get_peer(&self, identity: &IrohIdentity) -> Option<PeerInfo> {
        self.known_peers.get(identity).map(|e| e.value().clone())
    }

    /// Subscribe to peer events
    pub fn subscribe(&self) -> broadcast::Receiver<PeerEvent> {
        self.event_tx.subscribe()
    }

    /// Handle a received gossip message
    pub fn handle_message(&self, data: &[u8]) -> Result<(), DiscoveryError> {
        let msg = parse_framed_message(data)
            .map_err(|e| DiscoveryError::DeserializationError(e.to_string()))?;

        match msg {
            // Global presence messages
            WireMessage::PresenceAnnounce(presence) => {
                self.handle_presence_announce(presence);
            }
            WireMessage::PresenceQuery => {
                debug!("Received presence query");
            }
            WireMessage::PresenceResponse(peers) => {
                for presence in peers {
                    self.handle_presence_announce(presence);
                }
            }

            // Realm discovery messages
            WireMessage::InterfaceJoin(join_msg) => {
                self.handle_interface_join(join_msg);
            }
            WireMessage::InterfaceLeave(leave_msg) => {
                self.handle_interface_leave(leave_msg);
            }
            WireMessage::PeerIntroduction(intro_msg) => {
                self.handle_peer_introduction(intro_msg);
            }
            WireMessage::IntroductionRequest(request_msg) => {
                self.handle_introduction_request(request_msg);
            }
            WireMessage::IntroductionResponse(response_msg) => {
                self.handle_introduction_response(response_msg);
            }

            _ => {
                // Ignore other message types
            }
        }

        Ok(())
    }

    /// Handle InterfaceJoin message - a peer joined the realm
    #[instrument(skip(self, msg), fields(interface = %hex::encode(&msg.interface_id.as_bytes()[..8])))]
    fn handle_interface_join(&self, msg: InterfaceJoinMessage) {
        // Ignore if we're not in this realm
        if !self.realm_topics.contains_key(&msg.interface_id) {
            return;
        }

        // Extract peer ID from the message (we need the sender, but it's not in the message)
        // For now, we create peer info from the message fields
        // NOTE: In a real implementation, the sender ID would come from the gossip layer
        // For now, we skip self-messages in the caller

        // This is handled by the node layer which has access to the sender ID
        debug!("Received InterfaceJoin (handled by node layer)");
    }

    /// Handle InterfaceLeave message - a peer left the realm
    fn handle_interface_leave(&self, msg: InterfaceLeaveMessage) {
        // Ignore if we're not in this realm
        if !self.realm_topics.contains_key(&msg.interface_id) {
            return;
        }

        // This is handled by the node layer which has access to the sender ID
        debug!("Received InterfaceLeave (handled by node layer)");
    }

    /// Handle PeerIntroduction message - learn about a peer from another member
    fn handle_peer_introduction(&self, msg: PeerIntroductionMessage) {
        // Ignore if we're not in this realm
        if !self.realm_topics.contains_key(&msg.interface_id) {
            return;
        }

        // Ignore our own introductions
        if msg.peer_info.peer_id == self.local_identity {
            return;
        }

        self.add_realm_peer(msg.interface_id, msg.peer_info);
    }

    /// Handle IntroductionRequest - emit event for node to respond
    fn handle_introduction_request(&self, msg: IntroductionRequestMessage) {
        // Ignore if we're not in this realm
        if !self.realm_topics.contains_key(&msg.interface_id) {
            return;
        }

        // Emit event so the node layer can respond with rate limiting
        // NOTE: The requester ID should come from gossip layer sender info
        // For now, this is handled at the node layer
        debug!("Received IntroductionRequest (handled by node layer)");
    }

    /// Handle IntroductionResponse - learn about members we didn't know
    fn handle_introduction_response(&self, msg: IntroductionResponseMessage) {
        // Ignore if we're not in this realm
        if !self.realm_topics.contains_key(&msg.interface_id) {
            return;
        }

        for peer_info in msg.members {
            // Skip ourselves
            if peer_info.peer_id == self.local_identity {
                continue;
            }

            self.add_realm_peer(msg.interface_id, peer_info);
        }
    }

    /// Add or update a peer in a realm's peer list
    fn add_realm_peer(&self, interface_id: InterfaceId, peer_info: RealmPeerInfo) {
        let peers = self
            .realm_peers
            .entry(interface_id)
            .or_insert_with(DashMap::new);

        let is_new = !peers.contains_key(&peer_info.peer_id);

        // Insert or update
        peers.insert(peer_info.peer_id, peer_info.clone());

        // Emit event
        if is_new {
            info!(
                peer = %peer_info.peer_id.short_id(),
                realm = %hex::encode(&interface_id.as_bytes()[..8]),
                "Discovered new realm peer"
            );
            let _ = self.event_tx.send(PeerEvent::RealmPeerJoined {
                interface_id,
                peer_info,
            });
        }
    }

    /// Remove a peer from a realm's peer list
    pub fn remove_realm_peer(&self, interface_id: InterfaceId, peer_id: IrohIdentity) {
        if let Some(peers) = self.realm_peers.get(&interface_id) {
            if peers.remove(&peer_id).is_some() {
                info!(
                    peer = %peer_id.short_id(),
                    realm = %hex::encode(&interface_id.as_bytes()[..8]),
                    "Realm peer left"
                );
                let _ = self.event_tx.send(PeerEvent::RealmPeerLeft {
                    interface_id,
                    peer_id,
                });
            }
        }
    }

    /// Handle a presence announcement
    #[instrument(skip(self, presence), fields(remote_peer = %presence.peer_id.short_id()))]
    fn handle_presence_announce(&self, presence: PresenceInfo) {
        // Ignore our own announcements
        if presence.peer_id == self.local_identity {
            return;
        }

        let peer_id = presence.peer_id;
        let now = chrono::Utc::now().timestamp_millis();

        let peer_info = PeerInfo {
            identity: peer_id,
            presence: presence.clone(),
            last_seen_millis: now,
        };

        let is_new = !self.known_peers.contains_key(&peer_id);

        // Update or insert
        self.known_peers.insert(peer_id, peer_info.clone());

        // Emit event
        let event = if is_new {
            info!(event = "peer_discovered", "Discovered new peer");
            PeerEvent::Discovered(peer_info)
        } else {
            debug!(event = "peer_updated", "Updated peer info");
            PeerEvent::Updated(peer_info)
        };

        let _ = self.event_tx.send(event);
    }

    /// Remove peers that haven't been seen recently
    pub fn cleanup_stale_peers(&self) {
        let now = chrono::Utc::now().timestamp_millis();
        let timeout = self.config.peer_timeout_ms as i64;

        let stale: Vec<_> = self
            .known_peers
            .iter()
            .filter(|e| now - e.value().last_seen_millis > timeout)
            .map(|e| *e.key())
            .collect();

        for peer_id in stale {
            self.known_peers.remove(&peer_id);
            info!(peer = %peer_id.short_id(), "Peer timed out");
            let _ = self.event_tx.send(PeerEvent::Lost(peer_id));
        }
    }

    /// Get discovery statistics
    pub fn stats(&self) -> DiscoveryStats {
        DiscoveryStats {
            known_peers: self.known_peers.len(),
            max_tracked: self.config.max_tracked_peers,
            realm_count: self.realm_topics.len(),
            realm_peers: self
                .realm_peers
                .iter()
                .map(|e| (*e.key(), e.value().len()))
                .collect(),
        }
    }

    /// Get the list of realms we're currently in
    pub fn active_realms(&self) -> Vec<InterfaceId> {
        self.realm_topics.iter().map(|e| *e.key()).collect()
    }
}

/// Discovery statistics
#[derive(Debug, Clone)]
pub struct DiscoveryStats {
    /// Number of known peers (global)
    pub known_peers: usize,
    /// Maximum tracked peers
    pub max_tracked: usize,
    /// Number of realms we're subscribed to
    pub realm_count: usize,
    /// Peer count per realm
    pub realm_peers: Vec<(InterfaceId, usize)>,
}

// Simple hex encoding for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_topic_id() {
        let topic = DiscoveryConfig::default_topic_id();
        // Verify it's deterministic
        let topic2 = DiscoveryConfig::default_topic_id();
        assert_eq!(topic, topic2);
    }

    #[test]
    fn test_presence_info_creation() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let id = IrohIdentity::new(secret.public());

        let presence = PresenceInfo::new(id).with_name("TestNode");

        assert_eq!(presence.peer_id, id);
        assert_eq!(presence.display_name, Some("TestNode".to_string()));
        assert!(presence.accepting_connections);
    }

    #[test]
    fn test_presence_info_with_neighbors() {
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());
        let secret3 = SecretKey::generate(&mut rand::rng());

        let id = IrohIdentity::new(secret1.public());
        let neighbor1 = IrohIdentity::new(secret2.public());
        let neighbor2 = IrohIdentity::new(secret3.public());

        let presence = PresenceInfo::new(id).with_neighbors(vec![neighbor1, neighbor2]);

        assert_eq!(presence.neighbors.len(), 2);
        assert!(presence.neighbors.contains(&neighbor1));
        assert!(presence.neighbors.contains(&neighbor2));
    }

    #[test]
    fn test_presence_info_timestamp_is_recent() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let id = IrohIdentity::new(secret.public());

        let before = chrono::Utc::now().timestamp_millis();
        let presence = PresenceInfo::new(id);
        let after = chrono::Utc::now().timestamp_millis();

        assert!(presence.timestamp_millis >= before);
        assert!(presence.timestamp_millis <= after);
    }

    #[test]
    fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();
        assert!(config.announce_interval_ms > 0);
        assert!(config.peer_timeout_ms > 0);
        assert!(config.max_tracked_peers > 0);
    }

    #[test]
    fn test_discovery_config_custom() {
        let config = DiscoveryConfig {
            announce_interval_ms: 5000,
            peer_timeout_ms: 15000,
            max_tracked_peers: 500,
            ..Default::default()
        };

        assert_eq!(config.announce_interval_ms, 5000);
        assert_eq!(config.peer_timeout_ms, 15000);
        assert_eq!(config.max_tracked_peers, 500);
    }

    #[test]
    fn test_topic_for_interface_deterministic() {
        use indras_core::InterfaceId;

        let interface_id = InterfaceId::new([0x42; 32]);

        // Should produce same topic ID for same interface
        let topic1 = DiscoveryService::topic_for_interface(&interface_id);
        let topic2 = DiscoveryService::topic_for_interface(&interface_id);
        assert_eq!(topic1, topic2);

        // Different interface should produce different topic
        let other_id = InterfaceId::new([0x43; 32]);
        let topic3 = DiscoveryService::topic_for_interface(&other_id);
        assert_ne!(topic1, topic3);
    }

    #[test]
    fn test_topic_for_interface_unique() {
        use indras_core::InterfaceId;
        use std::collections::HashSet;

        let mut topics = HashSet::new();

        // Generate 100 random interface IDs and verify unique topics
        for i in 0..100u8 {
            let mut bytes = [0u8; 32];
            bytes[0] = i;
            let interface_id = InterfaceId::new(bytes);
            let topic = DiscoveryService::topic_for_interface(&interface_id);
            topics.insert(topic.as_bytes().to_vec());
        }

        assert_eq!(topics.len(), 100);
    }

    #[test]
    fn test_peer_event_realm_variants() {
        use indras_core::InterfaceId;
        use iroh::SecretKey;
        use crate::protocol::RealmPeerInfo;

        let secret = SecretKey::generate(&mut rand::rng());
        let peer = IrohIdentity::new(secret.public());
        let interface_id = InterfaceId::new([0xAB; 32]);

        // Test RealmPeerJoined
        let peer_info = RealmPeerInfo::new(peer).with_name("TestPeer");
        let event = PeerEvent::RealmPeerJoined {
            interface_id,
            peer_info: peer_info.clone(),
        };

        match event {
            PeerEvent::RealmPeerJoined { interface_id: iid, peer_info: pi } => {
                assert_eq!(iid, interface_id);
                assert_eq!(pi.peer_id, peer);
                assert_eq!(pi.display_name, Some("TestPeer".to_string()));
            }
            _ => panic!("Expected RealmPeerJoined"),
        }

        // Test RealmPeerLeft
        let event = PeerEvent::RealmPeerLeft {
            interface_id,
            peer_id: peer,
        };

        match event {
            PeerEvent::RealmPeerLeft { interface_id: iid, peer_id: pid } => {
                assert_eq!(iid, interface_id);
                assert_eq!(pid, peer);
            }
            _ => panic!("Expected RealmPeerLeft"),
        }

        // Test IntroductionRequested
        let event = PeerEvent::IntroductionRequested {
            interface_id,
            requester: peer,
            known_peers: vec![],
        };

        match event {
            PeerEvent::IntroductionRequested { interface_id: iid, requester, known_peers } => {
                assert_eq!(iid, interface_id);
                assert_eq!(requester, peer);
                assert!(known_peers.is_empty());
            }
            _ => panic!("Expected IntroductionRequested"),
        }
    }
}
