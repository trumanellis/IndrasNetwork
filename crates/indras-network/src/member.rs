//! Member type - simplified identity wrapper.
//!
//! Wraps `IrohIdentity` from the transport layer with a user-friendly API.

use chrono::{DateTime, Utc};
use indras_core::{PeerIdentity, PresenceStatus};
use indras_transport::IrohIdentity;
use std::fmt;
use std::hash::{Hash, Hasher};

/// Unique identifier for a member (32-byte public key hash).
pub type MemberId = [u8; 32];

/// A member of a realm.
///
/// Wraps the underlying cryptographic identity with a user-friendly API
/// including display names and convenient formatting.
#[derive(Clone)]
pub struct Member {
    /// The underlying transport identity.
    inner: IrohIdentity,
    /// Optional display name.
    display_name: Option<String>,
    /// Last known presence status.
    presence: PresenceStatus,
    /// Timestamp of last observed activity.
    last_seen: Option<DateTime<Utc>>,
}

impl Member {
    /// Create a new member from an identity.
    pub fn new(identity: IrohIdentity) -> Self {
        Self {
            inner: identity,
            display_name: None,
            presence: PresenceStatus::default(),
            last_seen: None,
        }
    }

    /// Create a new member with a display name.
    pub fn with_name(identity: IrohIdentity, name: impl Into<String>) -> Self {
        Self {
            inner: identity,
            display_name: Some(name.into()),
            presence: PresenceStatus::default(),
            last_seen: None,
        }
    }

    /// Get the member's unique identifier.
    pub fn id(&self) -> MemberId {
        let bytes = self.inner.as_bytes();
        let mut id = [0u8; 32];
        id.copy_from_slice(&bytes[..32.min(bytes.len())]);
        id
    }

    /// Get the member's display name.
    ///
    /// Returns the display name if set, otherwise returns the short ID.
    pub fn name(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| self.inner.short_id())
    }

    /// Get the member's short ID (first 8 hex characters).
    pub fn short_id(&self) -> String {
        self.inner.short_id()
    }

    /// Get the underlying iroh public key.
    pub fn public_key(&self) -> &iroh::PublicKey {
        self.inner.public_key()
    }

    /// Set the display name.
    pub fn set_display_name(&mut self, name: Option<String>) {
        self.display_name = name;
    }

    // ============================================================
    // Presence
    // ============================================================

    /// Get the member's current presence status.
    pub fn presence(&self) -> PresenceStatus {
        self.presence
    }

    /// Check if the member is currently online.
    ///
    /// Returns true if the presence status is `Online`, `Away`, or `Busy`
    /// (anything other than `Offline`).
    pub fn is_online(&self) -> bool {
        !matches!(self.presence, PresenceStatus::Offline)
    }

    /// Get the timestamp of the member's last observed activity.
    ///
    /// Returns `None` if no activity has been observed.
    pub fn last_seen(&self) -> Option<DateTime<Utc>> {
        self.last_seen
    }

    /// Update the member's presence status and last-seen timestamp.
    pub fn set_presence(&mut self, status: PresenceStatus) {
        self.presence = status;
        if !matches!(status, PresenceStatus::Offline) {
            self.last_seen = Some(Utc::now());
        }
    }

    /// Update the last-seen timestamp to now.
    pub fn touch(&mut self) {
        self.last_seen = Some(Utc::now());
    }

    // ============================================================
    // Escape hatch
    // ============================================================

    /// Access the underlying transport identity.
    pub fn identity(&self) -> &IrohIdentity {
        &self.inner
    }
}

impl fmt::Debug for Member {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Member")
            .field("id", &self.short_id())
            .field("display_name", &self.display_name)
            .finish()
    }
}

impl fmt::Display for Member {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl PartialEq for Member {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for Member {}

impl Hash for Member {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl From<IrohIdentity> for Member {
    fn from(identity: IrohIdentity) -> Self {
        Self::new(identity)
    }
}

/// Events related to realm membership changes.
#[derive(Debug, Clone)]
pub enum MemberEvent {
    /// A new member joined the realm.
    Joined(Member),
    /// A member left the realm.
    Left(Member),
    /// A member's profile was updated.
    Updated(Member),
    /// A new member was discovered via gossip (includes PQ keys).
    Discovered(MemberInfo),
}

/// Extended member information including post-quantum keys.
///
/// This is returned from discovery and includes the member's
/// PQ keys for secure direct communication.
#[derive(Debug, Clone)]
pub struct MemberInfo {
    /// The member.
    pub member: Member,
    /// ML-KEM-768 encapsulation key for sending encrypted keys to this peer.
    pub pq_encapsulation_key: Option<Vec<u8>>,
    /// ML-DSA-65 verifying key for verifying signatures from this peer.
    pub pq_verifying_key: Option<Vec<u8>>,
}

impl MemberInfo {
    /// Create member info from transport layer peer info.
    pub fn from_realm_peer_info(info: indras_transport::RealmPeerInfo) -> Self {
        let member = if let Some(name) = info.display_name {
            Member::with_name(info.peer_id, name)
        } else {
            Member::new(info.peer_id)
        };

        Self {
            member,
            pq_encapsulation_key: info.pq_encapsulation_key,
            pq_verifying_key: info.pq_verifying_key,
        }
    }

    /// Check if this member has PQ keys for secure communication.
    pub fn has_pq_keys(&self) -> bool {
        self.pq_encapsulation_key.is_some() && self.pq_verifying_key.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_display() {
        // Can't easily test without real identity, but structure is correct
    }
}
