//! Direct peer connection — "Identity IS Connection".
//!
//! Replaces the legacy realm-based handshake with a single-call `connect(member_id)` API.
//! Uses deterministic DM realm IDs and in-band ML-KEM key exchange.

use crate::member::MemberId;
use crate::network::RealmId;

use indras_core::InterfaceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default key exchange expiry: 7 days in milliseconds.
const KEY_EXCHANGE_EXPIRY_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Deterministic key seed for a DM realm between two members.
///
/// Both peers independently compute the same seed (and thus the same
/// `InterfaceKey`) because the inputs are sorted. Anyone who knows both
/// MemberIds can derive this — content confidentiality for DMs should
/// eventually use a higher-level protocol (e.g., Double Ratchet).
pub fn dm_key_seed(a: &MemberId, b: &MemberId) -> [u8; 32] {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"dm-key-v1:");
    hasher.update(first);
    hasher.update(second);
    *hasher.finalize().as_bytes()
}

/// Derive a deterministic DM realm ID from two member IDs.
///
/// The same pair of members always produces the same realm ID,
/// regardless of who initiates. IDs are sorted to ensure determinism.
///
/// # Example
///
/// ```ignore
/// let realm_id = dm_realm_id(zephyr_id, nova_id);
/// // Same as:
/// let realm_id2 = dm_realm_id(nova_id, zephyr_id);
/// assert_eq!(realm_id, realm_id2);
/// ```
pub fn dm_realm_id(a: MemberId, b: MemberId) -> RealmId {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"dm-v1:");
    hasher.update(&first);
    hasher.update(&second);
    InterfaceId::new(*hasher.finalize().as_bytes())
}

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
    fn test_dm_realm_id_deterministic() {
        let id1 = dm_realm_id(zephyr_id(), nova_id());
        let id2 = dm_realm_id(zephyr_id(), nova_id());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_dm_realm_id_symmetric() {
        let id1 = dm_realm_id(zephyr_id(), nova_id());
        let id2 = dm_realm_id(nova_id(), zephyr_id());
        assert_eq!(id1, id2, "DM realm ID should be the same regardless of order");
    }

    #[test]
    fn test_dm_realm_id_unique_per_pair() {
        let id1 = dm_realm_id(zephyr_id(), nova_id());
        let id2 = dm_realm_id(zephyr_id(), sage_id());
        let id3 = dm_realm_id(nova_id(), sage_id());
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_dm_realm_id_differs_from_home() {
        let member = zephyr_id();
        let dm = dm_realm_id(member, nova_id());
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
        let exchange = PendingKeyExchange::new(nova_id(), dm_realm_id(zephyr_id(), nova_id()));
        assert!(!exchange.is_expired());
    }

    #[test]
    fn test_pending_key_exchange_expired() {
        let realm_id = dm_realm_id(zephyr_id(), nova_id());
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
        let realm_id = dm_realm_id(zephyr_id(), nova_id());
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
        let realm_id = dm_realm_id(zephyr_id(), nova_id());
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
        let realm_id = dm_realm_id(zephyr_id(), nova_id());
        let exchange = PendingKeyExchange::new(nova_id(), realm_id);

        let bytes = postcard::to_allocvec(&exchange).unwrap();
        let deserialized: PendingKeyExchange = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(deserialized.peer_id, nova_id());
        assert_eq!(deserialized.realm_id, realm_id);
        assert_eq!(deserialized.status, KeyExchangeStatus::AwaitingPeer);
    }
}
