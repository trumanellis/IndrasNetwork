//! Peer discovery using iroh-gossip
//!
//! Provides presence announcement and peer discovery through gossip protocol.

use dashmap::DashMap;
use iroh::PublicKey;
use iroh_gossip::Gossip;
use iroh_gossip::api::GossipTopic;
use iroh_gossip::proto::TopicId;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};

use indras_core::identity::PeerIdentity;

use crate::identity::IrohIdentity;
use crate::protocol::{PresenceInfo, WireMessage, frame_message, parse_framed_message};

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
    /// A new peer was discovered
    Discovered(PeerInfo),
    /// A peer's information was updated
    Updated(PeerInfo),
    /// A peer went offline (timed out)
    Lost(IrohIdentity),
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

/// Peer discovery service using iroh-gossip
pub struct DiscoveryService {
    /// Gossip handle
    gossip: Gossip,
    /// Topic handle for presence
    topic: RwLock<Option<GossipTopic>>,
    /// Known peers
    known_peers: DashMap<IrohIdentity, PeerInfo>,
    /// Our own identity
    local_identity: IrohIdentity,
    /// Configuration
    config: DiscoveryConfig,
    /// Event broadcaster
    event_tx: broadcast::Sender<PeerEvent>,
    /// Running state
    running: RwLock<bool>,
}

impl DiscoveryService {
    /// Create a new discovery service
    pub fn new(
        gossip: Gossip,
        local_identity: IrohIdentity,
        config: DiscoveryConfig,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            gossip,
            topic: RwLock::new(None),
            known_peers: DashMap::new(),
            local_identity,
            config,
            event_tx,
            running: RwLock::new(false),
        }
    }

    /// Start the discovery service
    ///
    /// Joins the gossip topic and begins listening for peer announcements.
    pub async fn start(&self, bootstrap_peers: Vec<PublicKey>) -> Result<(), DiscoveryError> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }

        info!(identity = %self.local_identity.short_id(), "Starting discovery service");

        // Convert PublicKey to the format gossip expects
        let bootstrap: Vec<_> = bootstrap_peers.into_iter().collect();

        // Join the gossip topic
        let topic = self.gossip
            .subscribe(self.config.topic_id, bootstrap)
            .await
            .map_err(|e| DiscoveryError::JoinError(e.to_string()))?;

        *self.topic.write().await = Some(topic);
        *running = true;

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
    }

    /// Announce our presence to the network
    pub async fn announce_presence(&self) -> Result<(), DiscoveryError> {
        let mut topic_guard = self.topic.write().await;
        let topic = topic_guard.as_mut().ok_or(DiscoveryError::NotRunning)?;

        let presence = PresenceInfo::new(self.local_identity);
        let msg = WireMessage::PresenceAnnounce(presence);

        let framed = frame_message(&msg)
            .map_err(|e| DiscoveryError::SerializationError(e.to_string()))?;

        topic.broadcast(framed)
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
        let framed = frame_message(&msg)
            .map_err(|e| DiscoveryError::SerializationError(e.to_string()))?;

        topic.broadcast(framed)
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
            _ => {
                // Ignore non-presence messages
            }
        }

        Ok(())
    }

    /// Handle a presence announcement
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
            info!(peer = %peer_id.short_id(), "Discovered new peer");
            PeerEvent::Discovered(peer_info)
        } else {
            debug!(peer = %peer_id.short_id(), "Updated peer info");
            PeerEvent::Updated(peer_info)
        };

        let _ = self.event_tx.send(event);
    }

    /// Remove peers that haven't been seen recently
    pub fn cleanup_stale_peers(&self) {
        let now = chrono::Utc::now().timestamp_millis();
        let timeout = self.config.peer_timeout_ms as i64;

        let stale: Vec<_> = self.known_peers
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
        }
    }
}

/// Discovery statistics
#[derive(Debug, Clone)]
pub struct DiscoveryStats {
    /// Number of known peers
    pub known_peers: usize,
    /// Maximum tracked peers
    pub max_tracked: usize,
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

        let secret = SecretKey::generate(&mut rand::thread_rng());
        let id = IrohIdentity::new(secret.public());

        let presence = PresenceInfo::new(id)
            .with_name("TestNode");

        assert_eq!(presence.peer_id, id);
        assert_eq!(presence.display_name, Some("TestNode".to_string()));
        assert!(presence.accepting_connections);
    }
}
