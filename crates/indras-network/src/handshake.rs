//! Handshake realm - bidirectional connection establishment.
//!
//! Each user has a personal "handshake realm" derived from their member ID.
//! When someone accepts your contact invite, they write a `ConnectionRequest`
//! to your handshake realm. Your node picks it up on next refresh/startup,
//! completing the bidirectional connection.

use crate::member::MemberId;
use crate::network::RealmId;

use indras_core::InterfaceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Generate a deterministic inbox interface ID from a member ID.
///
/// Each user has exactly one inbox interface where others can
/// leave connection requests. The same member_id always produces
/// the same interface_id.
pub fn inbox_interface_id(member_id: MemberId) -> RealmId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:handshake:v1:");
    hasher.update(&member_id);
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Maximum number of requests allowed in a single inbox.
const MAX_INBOX_SIZE: usize = 100;

/// Default request expiry: 30 days in milliseconds.
const REQUEST_EXPIRY_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// A request from another user to establish a bidirectional connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRequest {
    /// The member ID of the requester.
    pub member_id: MemberId,
    /// The display name of the requester (if known).
    pub display_name: Option<String>,
    /// When the request was created (millis since epoch).
    pub timestamp_millis: u64,
}

/// Document schema for storing pending connection requests.
///
/// Keyed by member ID to deduplicate — each member can have at most
/// one pending request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HandshakeDocument {
    /// Pending connection requests, keyed by requester member ID.
    pub requests: BTreeMap<MemberId, ConnectionRequest>,
}

impl HandshakeDocument {
    /// Add or update a connection request.
    ///
    /// Returns `false` if the inbox is full and the request is from a new member.
    /// Updates in place if the member already has a request (deduplication).
    pub fn add_request(&mut self, request: ConnectionRequest) -> bool {
        if self.requests.len() >= MAX_INBOX_SIZE
            && !self.requests.contains_key(&request.member_id)
        {
            return false; // Inbox full
        }
        self.requests.insert(request.member_id, request);
        true
    }

    /// Remove requests older than the given expiry duration.
    ///
    /// Returns the number of pruned requests.
    pub fn prune_expired(&mut self, now_millis: u64, max_age_ms: u64) -> usize {
        let before = self.requests.len();
        self.requests.retain(|_, req| {
            now_millis.saturating_sub(req.timestamp_millis) < max_age_ms
        });
        before - self.requests.len()
    }

    /// Prune expired requests using the default 30-day expiry.
    pub fn prune_expired_default(&mut self, now_millis: u64) -> usize {
        self.prune_expired(now_millis, REQUEST_EXPIRY_MS)
    }

    /// Remove a processed request.
    pub fn remove_request(&mut self, member_id: &MemberId) -> Option<ConnectionRequest> {
        self.requests.remove(member_id)
    }

    /// Get all pending requests.
    pub fn pending_requests(&self) -> impl Iterator<Item = &ConnectionRequest> {
        self.requests.values()
    }

    /// Number of pending requests.
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    /// Whether there are no pending requests.
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
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

    #[test]
    fn test_inbox_interface_id_deterministic() {
        let id1 = inbox_interface_id(test_member_id());
        let id2 = inbox_interface_id(test_member_id());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_inbox_interface_id_unique() {
        let id1 = inbox_interface_id(test_member_id());
        let id2 = inbox_interface_id(another_member_id());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_inbox_interface_id_differs_from_home() {
        let member = test_member_id();
        let inbox = inbox_interface_id(member);
        let home = crate::home_realm::home_realm_id(member);
        assert_ne!(inbox, home);
    }

    #[test]
    fn test_handshake_document_add_remove() {
        let mut doc = HandshakeDocument::default();
        assert!(doc.is_empty());

        let req = ConnectionRequest {
            member_id: test_member_id(),
            display_name: Some("Zephyr".to_string()),
            timestamp_millis: 1000,
        };

        doc.add_request(req);
        assert_eq!(doc.len(), 1);
        assert!(!doc.is_empty());

        let removed = doc.remove_request(&test_member_id());
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().display_name, Some("Zephyr".to_string()));
        assert!(doc.is_empty());
    }

    #[test]
    fn test_handshake_document_deduplication() {
        let mut doc = HandshakeDocument::default();

        let req1 = ConnectionRequest {
            member_id: test_member_id(),
            display_name: Some("Zephyr".to_string()),
            timestamp_millis: 1000,
        };
        let req2 = ConnectionRequest {
            member_id: test_member_id(),
            display_name: Some("Zephyr Updated".to_string()),
            timestamp_millis: 2000,
        };

        doc.add_request(req1);
        doc.add_request(req2);

        // Should still be 1 entry (deduped by member_id)
        assert_eq!(doc.len(), 1);
        let pending: Vec<_> = doc.pending_requests().collect();
        assert_eq!(pending[0].display_name, Some("Zephyr Updated".to_string()));
        assert_eq!(pending[0].timestamp_millis, 2000);
    }

    #[test]
    fn test_handshake_document_multiple_requests() {
        let mut doc = HandshakeDocument::default();

        doc.add_request(ConnectionRequest {
            member_id: test_member_id(),
            display_name: Some("Zephyr".to_string()),
            timestamp_millis: 1000,
        });
        doc.add_request(ConnectionRequest {
            member_id: another_member_id(),
            display_name: Some("Nova".to_string()),
            timestamp_millis: 2000,
        });

        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn test_handshake_document_serialization() {
        let mut doc = HandshakeDocument::default();
        doc.add_request(ConnectionRequest {
            member_id: test_member_id(),
            display_name: Some("Sage".to_string()),
            timestamp_millis: 12345,
        });

        let bytes = postcard::to_allocvec(&doc).unwrap();
        let deserialized: HandshakeDocument = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(deserialized.len(), 1);
        let req = deserialized.requests.get(&test_member_id()).unwrap();
        assert_eq!(req.display_name, Some("Sage".to_string()));
        assert_eq!(req.timestamp_millis, 12345);
    }

    #[test]
    fn test_inbox_size_limit() {
        let mut doc = HandshakeDocument::default();

        // Fill to MAX_INBOX_SIZE
        for i in 0..MAX_INBOX_SIZE {
            let mut mid = [0u8; 32];
            mid[0] = (i / 256) as u8;
            mid[1] = (i % 256) as u8;
            let added = doc.add_request(ConnectionRequest {
                member_id: mid,
                display_name: None,
                timestamp_millis: 1000,
            });
            assert!(added);
        }

        assert_eq!(doc.len(), MAX_INBOX_SIZE);

        // New member should be rejected
        let mut new_mid = [0xFFu8; 32];
        let added = doc.add_request(ConnectionRequest {
            member_id: new_mid,
            display_name: None,
            timestamp_millis: 1000,
        });
        assert!(!added);
        assert_eq!(doc.len(), MAX_INBOX_SIZE);

        // Existing member update should still work
        let mut existing_mid = [0u8; 32];
        let added = doc.add_request(ConnectionRequest {
            member_id: existing_mid,
            display_name: Some("Updated".to_string()),
            timestamp_millis: 2000,
        });
        assert!(added);
        assert_eq!(doc.len(), MAX_INBOX_SIZE);
    }

    #[test]
    fn test_prune_expired() {
        let mut doc = HandshakeDocument::default();

        // Add an old request (1 day ago)
        doc.add_request(ConnectionRequest {
            member_id: test_member_id(),
            display_name: Some("Orion".to_string()),
            timestamp_millis: 1000,
        });

        // Add a recent request
        doc.add_request(ConnectionRequest {
            member_id: another_member_id(),
            display_name: Some("Lyra".to_string()),
            timestamp_millis: 90_000,
        });

        assert_eq!(doc.len(), 2);

        // Prune with 50 second expiry at time 100_000
        let pruned = doc.prune_expired(100_000, 50_000);
        assert_eq!(pruned, 1); // Only old request pruned
        assert_eq!(doc.len(), 1);
        assert!(doc.requests.contains_key(&another_member_id()));
        assert!(!doc.requests.contains_key(&test_member_id()));
    }

    #[test]
    fn test_prune_expired_default_keeps_recent() {
        let mut doc = HandshakeDocument::default();
        let now = 1_700_000_000_000u64; // ~2023

        doc.add_request(ConnectionRequest {
            member_id: test_member_id(),
            display_name: None,
            timestamp_millis: now - 1000, // 1 second ago
        });

        let pruned = doc.prune_expired_default(now);
        assert_eq!(pruned, 0);
        assert_eq!(doc.len(), 1);
    }
}
