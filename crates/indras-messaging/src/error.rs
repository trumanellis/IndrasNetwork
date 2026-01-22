//! Error types for indras-messaging

use thiserror::Error;

/// Errors that can occur in the messaging layer
#[derive(Debug, Error)]
pub enum MessagingError {
    /// Interface not found
    #[error("interface not found: {0}")]
    InterfaceNotFound(String),

    /// Not a member of the interface
    #[error("not a member of interface")]
    NotMember,

    /// Encryption failed
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    /// Decryption failed
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    /// Serialization failed
    #[error("serialization failed: {0}")]
    SerializationFailed(String),

    /// Gossip error
    #[error("gossip error: {0}")]
    GossipError(String),

    /// Sync error
    #[error("sync error: {0}")]
    SyncError(String),

    /// Storage error
    #[error("storage error: {0}")]
    StorageError(String),

    /// Invalid message format
    #[error("invalid message format: {0}")]
    InvalidFormat(String),

    /// Message not found
    #[error("message not found")]
    MessageNotFound,

    /// Invalid invite
    #[error("invalid invite: {0}")]
    InvalidInvite(String),

    /// Already joined interface
    #[error("already joined interface")]
    AlreadyJoined,

    /// Channel closed
    #[error("channel closed")]
    ChannelClosed,

    /// Generic messaging error
    #[error("messaging error: {0}")]
    Other(String),
}

impl From<indras_core::InterfaceError> for MessagingError {
    fn from(e: indras_core::InterfaceError) -> Self {
        MessagingError::SyncError(e.to_string())
    }
}

impl From<indras_gossip::GossipError> for MessagingError {
    fn from(e: indras_gossip::GossipError) -> Self {
        MessagingError::GossipError(e.to_string())
    }
}

impl From<indras_crypto::CryptoError> for MessagingError {
    fn from(e: indras_crypto::CryptoError) -> Self {
        MessagingError::EncryptionFailed(e.to_string())
    }
}

impl From<postcard::Error> for MessagingError {
    fn from(e: postcard::Error) -> Self {
        MessagingError::SerializationFailed(e.to_string())
    }
}

/// Result type for messaging operations
pub type MessagingResult<T> = Result<T, MessagingError>;
