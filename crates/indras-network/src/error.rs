//! Unified error types for the Indra SyncEngine.
//!
//! Provides user-friendly, actionable error messages that wrap
//! the underlying infrastructure errors.

use indras_node::NodeError;
use indras_storage::StorageError;
use std::io;

/// Result type alias for SyncEngine operations.
pub type Result<T> = std::result::Result<T, IndraError>;

/// Unified error type for the Indra SyncEngine.
///
/// Provides clear, actionable error messages for common failure scenarios.
#[derive(Debug, thiserror::Error)]
pub enum IndraError {
    // ============================================================
    // User-facing errors (actionable)
    // ============================================================
    /// The invite code format is invalid.
    #[error("Invalid invite code: {reason}")]
    InvalidInvite { reason: String },

    /// The invite code has expired.
    #[error("Invite has expired")]
    InviteExpired,

    /// The realm has reached its maximum member capacity.
    #[error("Realm is at capacity")]
    RealmFull,

    /// The user has been removed from this realm.
    #[error("You have been removed from this realm")]
    RemovedFromRealm,

    /// The specified realm was not found.
    #[error("Realm not found: {id}")]
    RealmNotFound { id: String },

    /// The specified document was not found.
    #[error("Document not found: {name}")]
    DocumentNotFound { name: String },

    /// Not connected to the network.
    #[error("Not connected to network")]
    NotConnected,

    /// The network has not been started.
    #[error("Network not started - call start() first")]
    NotStarted,

    /// The network is already running.
    #[error("Network already started")]
    AlreadyStarted,

    /// Operation timed out.
    #[error("Operation timed out")]
    Timeout,

    /// Not a member of the specified realm.
    #[error("Not a member of this realm")]
    NotMember,

    /// Invalid operation for the current state.
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    // ============================================================
    // Wrapped infrastructure errors
    // ============================================================
    /// Network/transport layer error.
    #[error("Network error: {0}")]
    Network(String),

    /// Storage layer error.
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Sync/CRDT error.
    #[error("Sync error: {0}")]
    Sync(String),

    /// Cryptographic operation error.
    #[error("Crypto error: {0}")]
    Crypto(String),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// I/O error.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Document schema error.
    #[error("Document schema error: {0}")]
    Schema(String),

    /// Artifact/blob error.
    #[error("Artifact error: {0}")]
    Artifact(String),

    /// Story authentication error.
    #[error("Story authentication error: {reason}")]
    StoryAuth { reason: String },
}

impl From<NodeError> for IndraError {
    fn from(e: NodeError) -> Self {
        match e {
            NodeError::InterfaceNotFound(id) => IndraError::RealmNotFound { id },
            NodeError::NotStarted => IndraError::NotStarted,
            NodeError::AlreadyStarted => IndraError::AlreadyStarted,
            NodeError::Transport(s) => IndraError::Network(s),
            NodeError::Storage(e) => IndraError::Storage(e),
            NodeError::Sync(s) => IndraError::Sync(s),
            NodeError::Crypto(s) => IndraError::Crypto(s),
            NodeError::Serialization(s) => IndraError::Serialization(s),
            NodeError::Config(s) => IndraError::Config(s),
            NodeError::Io(s) => IndraError::Io(io::Error::other(s)),
            NodeError::StoryAuth(s) => IndraError::StoryAuth { reason: s },
            _ => IndraError::Network(e.to_string()),
        }
    }
}

impl From<postcard::Error> for IndraError {
    fn from(e: postcard::Error) -> Self {
        IndraError::Serialization(e.to_string())
    }
}

impl From<base64::DecodeError> for IndraError {
    fn from(e: base64::DecodeError) -> Self {
        IndraError::InvalidInvite {
            reason: format!("Invalid base64: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = IndraError::RealmNotFound {
            id: "abc123".to_string(),
        };
        assert!(err.to_string().contains("abc123"));

        let err = IndraError::InvalidInvite {
            reason: "malformed".to_string(),
        };
        assert!(err.to_string().contains("malformed"));
    }
}
