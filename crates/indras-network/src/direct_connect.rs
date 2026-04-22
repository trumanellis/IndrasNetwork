//! Direct peer connection — "Identity IS Connection".
//!
//! Replaces the legacy realm-based handshake with a single-call `connect(member_id)` API.
//! Uses deterministic DM realm IDs and in-band ML-KEM key exchange.

use crate::member::MemberId;
use crate::network::RealmId;

use indras_artifacts::ArtifactId;
use indras_core::InterfaceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Derive a deterministic inbox realm ID for a peer.
///
/// Anyone who knows the MemberId can compute this, enabling
/// them to send connection notifications to the peer's inbox.
pub fn inbox_realm_id(member_id: MemberId) -> RealmId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"inbox-v1:");
    hasher.update(&member_id);
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Derive a deterministic interface key seed for a peer's inbox realm.
///
/// This is used to derive the symmetric encryption key for the inbox.
/// Anyone who knows the MemberId can compute this.
pub fn inbox_key_seed(member_id: &MemberId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"inbox-key-v1:");
    hasher.update(member_id);
    *hasher.finalize().as_bytes()
}

/// A notification sent to a peer's inbox when someone connects to them.
///
/// Serialized as the payload of a message on the inbox realm.
///
/// Carries the sender's PQ verifying key plus a signature over the
/// other fields so receivers can confirm the notify wasn't forged
/// by someone who merely knew the target's `MemberId`. Signature
/// verification gates admission in the inbox handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionNotify {
    /// The sender's member ID.
    pub sender_id: MemberId,
    /// Optional display name.
    pub display_name: Option<String>,
    /// The DM realm ID for this connection.
    pub dm_realm_id: InterfaceId,
    /// Timestamp (millis since epoch).
    pub timestamp_millis: u64,
    /// Optional endpoint address for direct QUIC connection.
    pub endpoint_addr: Option<Vec<u8>>,
    /// Sender's PQ (Dilithium3) verifying-key bytes. Callers set
    /// this via [`ConnectionNotify::sign`]; unsigned notifies leave
    /// it empty and will fail [`verify`].
    #[serde(default)]
    pub sender_pq_vk: Vec<u8>,
    /// Dilithium3 signature over the canonical byte encoding of
    /// the sibling fields. Empty until [`ConnectionNotify::sign`].
    #[serde(default)]
    pub signature: Vec<u8>,
}

impl ConnectionNotify {
    /// Create a new connection notification.
    pub fn new(sender_id: MemberId, dm_realm_id: InterfaceId) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            sender_id,
            dm_realm_id,
            display_name: None,
            timestamp_millis: now,
            endpoint_addr: None,
            sender_pq_vk: Vec::new(),
            signature: Vec::new(),
        }
    }

    /// Set the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the endpoint address.
    pub fn with_endpoint_addr(mut self, addr: Vec<u8>) -> Self {
        self.endpoint_addr = Some(addr);
        self
    }

    /// Sign the notify under the given PQ identity. Call this just
    /// before serialisation; any subsequent mutation invalidates
    /// the signature.
    pub fn sign(mut self, identity: &indras_crypto::pq_identity::PQIdentity) -> Self {
        let msg = self.canonical_bytes_for_signing();
        let sig = identity.sign(&msg);
        self.sender_pq_vk = identity.verifying_key_bytes();
        self.signature = sig.to_bytes().to_vec();
        self
    }

    /// Verify that this notify was signed by a holder of
    /// `sender_pq_vk`. Returns `false` on any shape / signature
    /// mismatch — receivers drop rather than admit.
    pub fn verify(&self) -> bool {
        use indras_crypto::pq_identity::{PQPublicIdentity, PQSignature};
        if self.sender_pq_vk.is_empty() || self.signature.is_empty() {
            return false;
        }
        let Ok(pk) = PQPublicIdentity::from_bytes(&self.sender_pq_vk) else {
            return false;
        };
        let Ok(sig) = PQSignature::from_bytes(self.signature.clone()) else {
            return false;
        };
        pk.verify(&self.canonical_bytes_for_signing(), &sig)
    }

    /// Blake3 digest of the sender's PQ verifying key — a stable
    /// 32-byte UserId matching what peer-keys directories publish.
    /// Returns `None` when the notify is unsigned.
    pub fn sender_user_id(&self) -> Option<[u8; 32]> {
        if self.sender_pq_vk.is_empty() {
            return None;
        }
        Some(*blake3::hash(&self.sender_pq_vk).as_bytes())
    }

    /// Domain-separated serialisation the signature binds to.
    fn canonical_bytes_for_signing(&self) -> Vec<u8> {
        const DOMAIN: &[u8] = b"indras:connection-notify:v1";
        let mut out = Vec::with_capacity(DOMAIN.len() + 128);
        out.extend_from_slice(DOMAIN);
        out.extend_from_slice(&self.sender_id);
        out.extend_from_slice(self.dm_realm_id.as_bytes());
        out.extend_from_slice(&self.timestamp_millis.to_le_bytes());
        let name_bytes = self.display_name.as_deref().unwrap_or("").as_bytes();
        out.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(name_bytes);
        let endpoint_bytes = self.endpoint_addr.as_deref().unwrap_or(&[]);
        out.extend_from_slice(&(endpoint_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(endpoint_bytes);
        out
    }
}

/// Notification sent to a peer's inbox inviting them to a group realm.
///
/// The invitee uses `artifact_id` (deterministic via `group_tree_id`) to derive
/// the group's interface id and key seed, materializes it in their own home
/// realm index, and begins syncing. The `members` list lets the invitee grant
/// each peer on their own side so outgoing mutations reach everyone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInvite {
    /// The sender's member ID.
    pub sender_id: MemberId,
    /// The group's tree artifact ID.
    pub artifact_id: ArtifactId,
    /// Display name for the group.
    pub name: String,
    /// Full member set (creator + invitees), so each peer can wire its own grants.
    pub members: Vec<MemberId>,
    /// Timestamp (millis since epoch).
    pub timestamp_millis: u64,
}

impl GroupInvite {
    /// Create a new group invite.
    pub fn new(
        sender_id: MemberId,
        artifact_id: ArtifactId,
        name: impl Into<String>,
        members: Vec<MemberId>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            sender_id,
            artifact_id,
            name: name.into(),
            members,
            timestamp_millis: now,
        }
    }
}

/// Tagged wire format for all messages delivered on the peer-inbox realm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InboxMessage {
    /// A direct-message / peer-connection notification.
    Connection(ConnectionNotify),
    /// An invitation to join a group realm.
    GroupInvite(GroupInvite),
}

impl InboxMessage {
    /// Serialize to bytes for sending as an interface message.
    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Deserialize from bytes received on the inbox realm.
    pub fn from_bytes(data: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(data)
    }
}

/// Default key exchange expiry: 7 days in milliseconds.
const KEY_EXCHANGE_EXPIRY_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Determine who initiates the key exchange (lower MemberId = initiator).
///
/// This provides deterministic tie-breaking when both peers try
/// to initiate simultaneously.
pub fn is_initiator(my_id: &MemberId, peer_id: &MemberId) -> bool {
    my_id < peer_id
}

/// Status of a pending key exchange for a DM connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyExchangeStatus {
    /// Waiting for the peer to join the gossip topic.
    AwaitingPeer,
    /// We sent our key exchange, waiting for confirmation.
    Initiated,
    /// Key exchange complete, interface key established.
    Complete,
}

/// Tracks a pending DM key exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingKeyExchange {
    /// The peer we're connecting to.
    pub peer_id: MemberId,
    /// The DM realm ID.
    pub realm_id: RealmId,
    /// Current status.
    pub status: KeyExchangeStatus,
    /// When the exchange was initiated (millis since epoch).
    pub created_at: u64,
}

impl PendingKeyExchange {
    /// Create a new pending key exchange.
    pub fn new(peer_id: MemberId, realm_id: RealmId) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            peer_id,
            realm_id,
            status: KeyExchangeStatus::AwaitingPeer,
            created_at: now,
        }
    }

    /// Check if this key exchange has expired (7-day TTL).
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.is_expired_at(now)
    }

    /// Check expiry against a specific timestamp (for testing).
    pub fn is_expired_at(&self, now_millis: u64) -> bool {
        now_millis.saturating_sub(self.created_at) > KEY_EXCHANGE_EXPIRY_MS
    }
}

/// Registry of pending key exchanges, keyed by DM realm ID.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyExchangeRegistry {
    pub exchanges: BTreeMap<RealmId, PendingKeyExchange>,
}

impl KeyExchangeRegistry {
    pub fn new() -> Self {
        Self {
            exchanges: BTreeMap::new(),
        }
    }

    /// Register a new pending key exchange.
    pub fn insert(&mut self, exchange: PendingKeyExchange) {
        self.exchanges.insert(exchange.realm_id, exchange);
    }

    /// Get a pending exchange by realm ID.
    pub fn get(&self, realm_id: &RealmId) -> Option<&PendingKeyExchange> {
        self.exchanges.get(realm_id)
    }

    /// Get a mutable reference to a pending exchange.
    pub fn get_mut(&mut self, realm_id: &RealmId) -> Option<&mut PendingKeyExchange> {
        self.exchanges.get_mut(realm_id)
    }

    /// Remove a completed or expired exchange.
    pub fn remove(&mut self, realm_id: &RealmId) -> Option<PendingKeyExchange> {
        self.exchanges.remove(realm_id)
    }

    /// Remove all expired exchanges. Returns the number removed.
    pub fn cleanup_expired(&mut self) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let before = self.exchanges.len();
        self.exchanges.retain(|_, v| !v.is_expired_at(now));
        before - self.exchanges.len()
    }

    /// Find a pending exchange by peer ID.
    pub fn find_by_peer(&self, peer_id: &MemberId) -> Option<&PendingKeyExchange> {
        self.exchanges.values().find(|e| e.peer_id == *peer_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zephyr_id() -> MemberId {
        [1u8; 32]
    }

    fn nova_id() -> MemberId {
        [2u8; 32]
    }

    fn sage_id() -> MemberId {
        [3u8; 32]
    }

    #[test]
    fn test_dm_story_id_deterministic() {
        let artifact1 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let artifact2 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        assert_eq!(artifact1, artifact2);

        let id1 = crate::artifact_sync::artifact_interface_id(&artifact1);
        let id2 = crate::artifact_sync::artifact_interface_id(&artifact2);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_dm_story_id_symmetric() {
        let artifact1 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let artifact2 = indras_artifacts::dm_story_id(nova_id(), zephyr_id());
        assert_eq!(artifact1, artifact2, "DM artifact ID should be the same regardless of order");

        let id1 = crate::artifact_sync::artifact_interface_id(&artifact1);
        let id2 = crate::artifact_sync::artifact_interface_id(&artifact2);
        assert_eq!(id1, id2, "DM realm ID should be the same regardless of order");
    }

    #[test]
    fn test_dm_story_id_unique_per_pair() {
        let artifact1 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let artifact2 = indras_artifacts::dm_story_id(zephyr_id(), sage_id());
        let artifact3 = indras_artifacts::dm_story_id(nova_id(), sage_id());
        assert_ne!(artifact1, artifact2);
        assert_ne!(artifact1, artifact3);
        assert_ne!(artifact2, artifact3);

        let id1 = crate::artifact_sync::artifact_interface_id(&artifact1);
        let id2 = crate::artifact_sync::artifact_interface_id(&artifact2);
        let id3 = crate::artifact_sync::artifact_interface_id(&artifact3);
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_dm_realm_id_differs_from_home() {
        let member = zephyr_id();
        let artifact = indras_artifacts::dm_story_id(member, nova_id());
        let dm = crate::artifact_sync::artifact_interface_id(&artifact);
        let home = crate::home_realm::home_realm_id(member);
        assert_ne!(dm, home);
    }

    #[test]
    fn test_is_initiator_deterministic() {
        // Lower ID is always the initiator
        assert!(is_initiator(&zephyr_id(), &nova_id()));
        assert!(!is_initiator(&nova_id(), &zephyr_id()));
    }

    #[test]
    fn test_is_initiator_same_id() {
        // Same ID: not initiator (edge case — shouldn't connect to self)
        assert!(!is_initiator(&zephyr_id(), &zephyr_id()));
    }

    #[test]
    fn test_pending_key_exchange_not_expired() {
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let realm_id = crate::artifact_sync::artifact_interface_id(&artifact);
        let exchange = PendingKeyExchange::new(nova_id(), realm_id);
        assert!(!exchange.is_expired());
    }

    #[test]
    fn test_pending_key_exchange_expired() {
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let realm_id = crate::artifact_sync::artifact_interface_id(&artifact);
        let mut exchange = PendingKeyExchange::new(nova_id(), realm_id);
        // Set created_at to 8 days ago
        let eight_days_ms = 8 * 24 * 60 * 60 * 1000u64;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        exchange.created_at = now.saturating_sub(eight_days_ms);
        assert!(exchange.is_expired());
    }

    #[test]
    fn test_key_exchange_registry() {
        let mut registry = KeyExchangeRegistry::new();
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let realm_id = crate::artifact_sync::artifact_interface_id(&artifact);
        let exchange = PendingKeyExchange::new(nova_id(), realm_id);

        registry.insert(exchange);
        assert!(registry.get(&realm_id).is_some());
        assert!(registry.find_by_peer(&nova_id()).is_some());
        assert!(registry.find_by_peer(&sage_id()).is_none());

        registry.remove(&realm_id);
        assert!(registry.get(&realm_id).is_none());
    }

    #[test]
    fn test_key_exchange_registry_cleanup() {
        let mut registry = KeyExchangeRegistry::new();
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let realm_id = crate::artifact_sync::artifact_interface_id(&artifact);
        let mut exchange = PendingKeyExchange::new(nova_id(), realm_id);

        // Make it expired
        let eight_days_ms = 8 * 24 * 60 * 60 * 1000u64;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        exchange.created_at = now.saturating_sub(eight_days_ms);

        registry.insert(exchange);
        assert_eq!(registry.exchanges.len(), 1);

        let removed = registry.cleanup_expired();
        assert_eq!(removed, 1);
        assert!(registry.exchanges.is_empty());
    }

    #[test]
    fn test_key_exchange_serialization() {
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let realm_id = crate::artifact_sync::artifact_interface_id(&artifact);
        let exchange = PendingKeyExchange::new(nova_id(), realm_id);

        let bytes = postcard::to_allocvec(&exchange).unwrap();
        let deserialized: PendingKeyExchange = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(deserialized.peer_id, nova_id());
        assert_eq!(deserialized.realm_id, realm_id);
        assert_eq!(deserialized.status, KeyExchangeStatus::AwaitingPeer);
    }

    #[test]
    fn test_inbox_realm_id_deterministic() {
        let id1 = inbox_realm_id(zephyr_id());
        let id2 = inbox_realm_id(zephyr_id());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_inbox_realm_id_unique_per_member() {
        let id1 = inbox_realm_id(zephyr_id());
        let id2 = inbox_realm_id(nova_id());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_inbox_realm_id_differs_from_dm() {
        let inbox = inbox_realm_id(zephyr_id());
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let dm = crate::artifact_sync::artifact_interface_id(&artifact);
        assert_ne!(inbox, dm);
    }

    #[test]
    fn test_inbox_realm_id_differs_from_home() {
        let inbox = inbox_realm_id(zephyr_id());
        let home = crate::home_realm::home_realm_id(zephyr_id());
        assert_ne!(inbox, home);
    }

    #[test]
    fn test_inbox_key_seed_deterministic() {
        let k1 = inbox_key_seed(&zephyr_id());
        let k2 = inbox_key_seed(&zephyr_id());
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_inbox_key_seed_unique_per_member() {
        let k1 = inbox_key_seed(&zephyr_id());
        let k2 = inbox_key_seed(&nova_id());
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_dm_artifact_key_seed_deterministic() {
        let artifact1 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let artifact2 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let k1 = crate::artifact_sync::artifact_key_seed(&artifact1);
        let k2 = crate::artifact_sync::artifact_key_seed(&artifact2);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_dm_artifact_key_seed_symmetric() {
        let artifact1 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let artifact2 = indras_artifacts::dm_story_id(nova_id(), zephyr_id());
        let k1 = crate::artifact_sync::artifact_key_seed(&artifact1);
        let k2 = crate::artifact_sync::artifact_key_seed(&artifact2);
        assert_eq!(k1, k2, "DM key seed should be the same regardless of order");
    }

    #[test]
    fn test_dm_artifact_key_seed_unique_per_pair() {
        let artifact1 = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let artifact2 = indras_artifacts::dm_story_id(zephyr_id(), sage_id());
        let artifact3 = indras_artifacts::dm_story_id(nova_id(), sage_id());
        let k1 = crate::artifact_sync::artifact_key_seed(&artifact1);
        let k2 = crate::artifact_sync::artifact_key_seed(&artifact2);
        let k3 = crate::artifact_sync::artifact_key_seed(&artifact3);
        assert_ne!(k1, k2);
        assert_ne!(k1, k3);
        assert_ne!(k2, k3);
    }

    #[test]
    fn test_dm_artifact_key_seed_differs_from_realm_id() {
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let seed = crate::artifact_sync::artifact_key_seed(&artifact);
        let realm = crate::artifact_sync::artifact_interface_id(&artifact);
        assert_ne!(seed, *realm.as_bytes(), "Key seed must differ from realm ID");
    }

    #[test]
    fn test_dm_artifact_key_seed_differs_from_inbox_key_seed() {
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let dm = crate::artifact_sync::artifact_key_seed(&artifact);
        let inbox = inbox_key_seed(&zephyr_id());
        assert_ne!(dm, inbox);
    }

    #[test]
    fn test_connection_notify_serialization() {
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let dm_id = crate::artifact_sync::artifact_interface_id(&artifact);
        let notify = ConnectionNotify::new(zephyr_id(), dm_id)
            .with_name("Zephyr");

        let msg = InboxMessage::Connection(notify);
        let bytes = msg.to_bytes().unwrap();
        let deserialized = InboxMessage::from_bytes(&bytes).unwrap();

        match deserialized {
            InboxMessage::Connection(n) => {
                assert_eq!(n.sender_id, zephyr_id());
                assert_eq!(n.dm_realm_id, dm_id);
                assert_eq!(n.display_name, Some("Zephyr".to_string()));
                assert!(n.timestamp_millis > 0);
            }
            _ => panic!("expected Connection variant"),
        }
    }

    #[test]
    fn test_connection_notify_signed_verify_roundtrip() {
        use indras_crypto::pq_identity::PQIdentity;
        let artifact = indras_artifacts::dm_story_id(zephyr_id(), nova_id());
        let dm_id = crate::artifact_sync::artifact_interface_id(&artifact);
        let identity = PQIdentity::generate();

        let notify = ConnectionNotify::new(zephyr_id(), dm_id)
            .with_name("Zephyr")
            .with_endpoint_addr(vec![1, 2, 3])
            .sign(&identity);
        assert!(notify.verify(), "signed notify must verify");
        assert_eq!(
            notify.sender_user_id().unwrap(),
            *blake3::hash(&identity.verifying_key_bytes()).as_bytes()
        );

        // Serialise + deserialise preserves the signature.
        let msg = InboxMessage::Connection(notify.clone());
        let bytes = msg.to_bytes().unwrap();
        let deserialized = InboxMessage::from_bytes(&bytes).unwrap();
        let round = match deserialized {
            InboxMessage::Connection(n) => n,
            _ => panic!("expected Connection"),
        };
        assert!(round.verify(), "roundtripped notify must verify");

        // Any field mutation after signing invalidates the sig.
        let mut tampered = notify.clone();
        tampered.display_name = Some("Impostor".into());
        assert!(!tampered.verify(), "mutated notify must not verify");

        // An unsigned notify fails closed.
        let unsigned = ConnectionNotify::new(zephyr_id(), dm_id);
        assert!(!unsigned.verify());

        // Wrong-key signature is rejected.
        let attacker = PQIdentity::generate();
        let mut forged = notify.clone();
        forged.sender_pq_vk = attacker.verifying_key_bytes();
        assert!(!forged.verify());
    }

    #[test]
    fn test_group_invite_serialization() {
        let artifact = indras_artifacts::group_tree_id(zephyr_id(), &[nova_id(), sage_id()]);
        let invite = GroupInvite::new(
            zephyr_id(),
            artifact,
            "Alpha",
            vec![zephyr_id(), nova_id(), sage_id()],
        );

        let msg = InboxMessage::GroupInvite(invite);
        let bytes = msg.to_bytes().unwrap();
        let deserialized = InboxMessage::from_bytes(&bytes).unwrap();

        match deserialized {
            InboxMessage::GroupInvite(g) => {
                assert_eq!(g.sender_id, zephyr_id());
                assert_eq!(g.artifact_id, artifact);
                assert_eq!(g.name, "Alpha");
                assert_eq!(g.members.len(), 3);
            }
            _ => panic!("expected GroupInvite variant"),
        }
    }
}
