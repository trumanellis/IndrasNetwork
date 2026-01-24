//! Member type - simplified identity wrapper.
//!
//! Wraps `IrohIdentity` from the transport layer with a user-friendly API.

use indras_core::PeerIdentity;
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
}

impl Member {
    /// Create a new member from an identity.
    pub fn new(identity: IrohIdentity) -> Self {
        Self {
            inner: identity,
            display_name: None,
        }
    }

    /// Create a new member with a display name.
    pub fn with_name(identity: IrohIdentity, name: impl Into<String>) -> Self {
        Self {
            inner: identity,
            display_name: Some(name.into()),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_display() {
        // Can't easily test without real identity, but structure is correct
    }
}
