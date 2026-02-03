//! Handshake inbox — bidirectional connection establishment via messages.
//!
//! Each user has a personal inbox interface derived from their member ID.
//! When someone accepts your contact invite, they send a `ConnectionRequest`
//! message to your inbox. Your node picks it up via `events_since()`,
//! completing the bidirectional connection.

use crate::member::MemberId;
use crate::network::RealmId;

use indras_core::InterfaceId;
use serde::{Deserialize, Serialize};

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

/// Backward-compatible alias.
#[inline]
pub fn handshake_realm_id(member_id: MemberId) -> RealmId {
    inbox_interface_id(member_id)
}

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
    fn test_handshake_realm_id_deterministic() {
        let id1 = handshake_realm_id(test_member_id());
        let id2 = handshake_realm_id(test_member_id());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_handshake_realm_id_unique() {
        let id1 = handshake_realm_id(test_member_id());
        let id2 = handshake_realm_id(another_member_id());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_handshake_realm_id_differs_from_home() {
        let member = test_member_id();
        let handshake = handshake_realm_id(member);
        let home = crate::home_realm::home_realm_id(member);
        assert_ne!(handshake, home);
    }

    #[test]
    fn test_connection_request_serialization() {
        let req = ConnectionRequest {
            member_id: test_member_id(),
            display_name: Some("Sage".to_string()),
            timestamp_millis: 12345,
        };

        let bytes = postcard::to_allocvec(&req).unwrap();
        let deserialized: ConnectionRequest = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(deserialized.member_id, test_member_id());
        assert_eq!(deserialized.display_name, Some("Sage".to_string()));
        assert_eq!(deserialized.timestamp_millis, 12345);
    }
}
