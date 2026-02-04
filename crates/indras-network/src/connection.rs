//! Connection protocol - realm-based bidirectional handshake.
//!
//! Both parties exchange contact info in a shared temporary realm derived
//! from a random nonce. After exchange, the connection realm is discarded
//! and both sides use deterministic peer-set realms for ongoing communication.

use crate::member::MemberId;
use crate::network::RealmId;

use indras_core::InterfaceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default connection offer expiry: 7 days in milliseconds.
const CONNECTION_EXPIRY_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Derive a deterministic realm ID for a connection invite.
///
/// Each invite produces a unique realm because the nonce is random.
/// Both sides can independently derive the same realm ID from
/// the inviter's member ID and the nonce.
pub fn connection_realm_id(inviter_id: MemberId, nonce: [u8; 16]) -> RealmId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"connection-v1:");
    hasher.update(&inviter_id);
    hasher.update(b":");
    hasher.update(&nonce);
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// A's pre-seeded contact info in the connection realm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionOffer {
    /// The inviter's member ID.
    pub member_id: MemberId,
    /// Display name of the inviter.
    pub display_name: Option<String>,
    /// ML-KEM encapsulation key bytes for post-quantum key exchange.
    pub pq_encapsulation_key: Vec<u8>,
    /// ML-DSA verifying key bytes for post-quantum signatures.
    pub pq_verifying_key: Vec<u8>,
    /// Serialized endpoint address for transport bootstrap.
    pub endpoint_addr: Vec<u8>,
    /// When the offer was created (millis since epoch).
    pub timestamp_millis: u64,
}

/// B's response in the connection realm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionAccept {
    /// The acceptor's member ID.
    pub member_id: MemberId,
    /// Display name of the acceptor.
    pub display_name: Option<String>,
    /// ML-KEM encapsulation key bytes for post-quantum key exchange.
    pub pq_encapsulation_key: Vec<u8>,
    /// ML-DSA verifying key bytes for post-quantum signatures.
    pub pq_verifying_key: Vec<u8>,
    /// Serialized endpoint address for transport bootstrap.
    pub endpoint_addr: Vec<u8>,
    /// When the accept was created (millis since epoch).
    pub timestamp_millis: u64,
}

/// CRDT document schema for the connection realm.
///
/// Contains the inviter's offer and any acceptor responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectionDocument {
    /// The inviter's offer (seeded when the invite is created).
    pub offer: Option<ConnectionOffer>,
    /// Acceptor responses, keyed by member ID (supports multiple acceptors).
    pub accepts: BTreeMap<MemberId, ConnectionAccept>,
}

impl ConnectionDocument {
    /// Set the connection offer (inviter's info).
    pub fn set_offer(&mut self, offer: ConnectionOffer) {
        self.offer = Some(offer);
    }

    /// Add an accept response (acceptor's info).
    pub fn add_accept(&mut self, accept: ConnectionAccept) {
        self.accepts.insert(accept.member_id, accept);
    }

    /// Get the connection offer, if present.
    pub fn get_offer(&self) -> Option<&ConnectionOffer> {
        self.offer.as_ref()
    }

    /// Get all accept responses.
    pub fn all_accepts(&self) -> &BTreeMap<MemberId, ConnectionAccept> {
        &self.accepts
    }

    /// Check whether the offer has expired (7-day TTL).
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.is_expired_at(now)
    }

    /// Check expiry against a specific timestamp (for testing).
    pub fn is_expired_at(&self, now_millis: u64) -> bool {
        match &self.offer {
            Some(offer) => now_millis.saturating_sub(offer.timestamp_millis) > CONNECTION_EXPIRY_MS,
            None => false, // No offer means nothing to expire
        }
    }
}

/// Status of a pending outgoing connection invite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Waiting for an acceptor to join and respond.
    AwaitingAccept,
    /// At least one accept has been received.
    AcceptReceived,
    /// Connection fully established (contacts added).
    Complete,
    /// The invite has expired (7-day TTL).
    Expired,
}

/// Tracks an active outgoing connection invite on the inviter's side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConnection {
    /// The connection realm ID.
    pub realm_id: RealmId,
    /// The random nonce used to derive the realm.
    pub nonce: [u8; 16],
    /// When the invite was created.
    pub created_at: u64,
    /// Current status.
    pub status: ConnectionStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member_id() -> MemberId {
        [1u8; 32]
    }

    fn another_member_id() -> MemberId {
        [2u8; 32]
    }

    fn test_nonce() -> [u8; 16] {
        [42u8; 16]
    }

    fn another_nonce() -> [u8; 16] {
        [99u8; 16]
    }

    #[test]
    fn test_connection_realm_id_deterministic() {
        let id1 = connection_realm_id(test_member_id(), test_nonce());
        let id2 = connection_realm_id(test_member_id(), test_nonce());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_connection_realm_id_unique_by_nonce() {
        let id1 = connection_realm_id(test_member_id(), test_nonce());
        let id2 = connection_realm_id(test_member_id(), another_nonce());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_connection_realm_id_unique_by_member() {
        let id1 = connection_realm_id(test_member_id(), test_nonce());
        let id2 = connection_realm_id(another_member_id(), test_nonce());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_connection_realm_id_differs_from_home() {
        let member = test_member_id();
        let conn = connection_realm_id(member, test_nonce());
        let home = crate::home_realm::home_realm_id(member);
        assert_ne!(conn, home);
    }

    #[test]
    fn test_connection_document_offer_roundtrip() {
        let mut doc = ConnectionDocument::default();
        assert!(doc.get_offer().is_none());
        assert!(doc.all_accepts().is_empty());

        let offer = ConnectionOffer {
            member_id: test_member_id(),
            display_name: Some("Zephyr".to_string()),
            pq_encapsulation_key: vec![1, 2, 3],
            pq_verifying_key: vec![4, 5, 6],
            endpoint_addr: vec![7, 8, 9],
            timestamp_millis: 1000,
        };

        doc.set_offer(offer);
        let read = doc.get_offer().unwrap();
        assert_eq!(read.member_id, test_member_id());
        assert_eq!(read.display_name.as_deref(), Some("Zephyr"));
    }

    #[test]
    fn test_connection_document_accept_roundtrip() {
        let mut doc = ConnectionDocument::default();

        let accept = ConnectionAccept {
            member_id: another_member_id(),
            display_name: Some("Nova".to_string()),
            pq_encapsulation_key: vec![10, 11],
            pq_verifying_key: vec![12, 13],
            endpoint_addr: vec![14, 15],
            timestamp_millis: 2000,
        };

        doc.add_accept(accept);
        assert_eq!(doc.all_accepts().len(), 1);
        let read = doc.all_accepts().get(&another_member_id()).unwrap();
        assert_eq!(read.display_name.as_deref(), Some("Nova"));
    }

    #[test]
    fn test_connection_document_multiple_accepts() {
        let mut doc = ConnectionDocument::default();

        doc.add_accept(ConnectionAccept {
            member_id: [3u8; 32],
            display_name: Some("Sage".to_string()),
            pq_encapsulation_key: vec![],
            pq_verifying_key: vec![],
            endpoint_addr: vec![],
            timestamp_millis: 1000,
        });
        doc.add_accept(ConnectionAccept {
            member_id: [4u8; 32],
            display_name: Some("Orion".to_string()),
            pq_encapsulation_key: vec![],
            pq_verifying_key: vec![],
            endpoint_addr: vec![],
            timestamp_millis: 2000,
        });

        assert_eq!(doc.all_accepts().len(), 2);
    }

    #[test]
    fn test_connection_document_dedup_accepts() {
        let mut doc = ConnectionDocument::default();

        doc.add_accept(ConnectionAccept {
            member_id: another_member_id(),
            display_name: Some("Nova".to_string()),
            pq_encapsulation_key: vec![],
            pq_verifying_key: vec![],
            endpoint_addr: vec![],
            timestamp_millis: 1000,
        });
        doc.add_accept(ConnectionAccept {
            member_id: another_member_id(),
            display_name: Some("Nova Updated".to_string()),
            pq_encapsulation_key: vec![],
            pq_verifying_key: vec![],
            endpoint_addr: vec![],
            timestamp_millis: 2000,
        });

        // BTreeMap deduplicates by member_id
        assert_eq!(doc.all_accepts().len(), 1);
        let read = doc.all_accepts().get(&another_member_id()).unwrap();
        assert_eq!(read.display_name.as_deref(), Some("Nova Updated"));
    }

    #[test]
    fn test_connection_document_not_expired() {
        let mut doc = ConnectionDocument::default();
        let now = 1_700_000_000_000u64;

        doc.set_offer(ConnectionOffer {
            member_id: test_member_id(),
            display_name: None,
            pq_encapsulation_key: vec![],
            pq_verifying_key: vec![],
            endpoint_addr: vec![],
            timestamp_millis: now - 1000, // 1 second ago
        });

        assert!(!doc.is_expired_at(now));
    }

    #[test]
    fn test_connection_document_expired() {
        let mut doc = ConnectionDocument::default();
        let now = 1_700_000_000_000u64;
        let eight_days_ms = 8 * 24 * 60 * 60 * 1000;

        doc.set_offer(ConnectionOffer {
            member_id: test_member_id(),
            display_name: None,
            pq_encapsulation_key: vec![],
            pq_verifying_key: vec![],
            endpoint_addr: vec![],
            timestamp_millis: now - eight_days_ms,
        });

        assert!(doc.is_expired_at(now));
    }

    #[test]
    fn test_connection_document_serialization() {
        let mut doc = ConnectionDocument::default();
        doc.set_offer(ConnectionOffer {
            member_id: test_member_id(),
            display_name: Some("Lyra".to_string()),
            pq_encapsulation_key: vec![1, 2, 3],
            pq_verifying_key: vec![4, 5, 6],
            endpoint_addr: vec![7, 8, 9],
            timestamp_millis: 12345,
        });
        doc.add_accept(ConnectionAccept {
            member_id: another_member_id(),
            display_name: Some("Kai".to_string()),
            pq_encapsulation_key: vec![10],
            pq_verifying_key: vec![11],
            endpoint_addr: vec![12],
            timestamp_millis: 67890,
        });

        let bytes = postcard::to_allocvec(&doc).unwrap();
        let deserialized: ConnectionDocument = postcard::from_bytes(&bytes).unwrap();

        let offer = deserialized.get_offer().unwrap();
        assert_eq!(offer.member_id, test_member_id());
        assert_eq!(offer.display_name.as_deref(), Some("Lyra"));

        let accept = deserialized.all_accepts().get(&another_member_id()).unwrap();
        assert_eq!(accept.display_name.as_deref(), Some("Kai"));
        assert_eq!(accept.timestamp_millis, 67890);
    }

    #[test]
    fn test_no_offer_not_expired() {
        let doc = ConnectionDocument::default();
        assert!(!doc.is_expired());
    }
}
