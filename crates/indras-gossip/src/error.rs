//! Error types for indras-gossip

use thiserror::Error;

/// Errors that can occur in the gossip layer
#[derive(Debug, Error)]
pub enum GossipError {
    /// Failed to subscribe to topic
    #[error("failed to subscribe to topic: {0}")]
    SubscribeFailed(String),

    /// Failed to broadcast message
    #[error("failed to broadcast message: {0}")]
    BroadcastFailed(String),

    /// Failed to encode message
    #[error("failed to encode message: {0}")]
    EncodeFailed(String),

    /// Failed to decode message
    #[error("failed to decode message: {0}")]
    DecodeFailed(String),

    /// Signature verification failed
    #[error("signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    /// Topic not found
    #[error("topic not found: {0}")]
    TopicNotFound(String),

    /// Already subscribed to topic
    #[error("already subscribed to topic")]
    AlreadySubscribed,

    /// Not subscribed to topic
    #[error("not subscribed to topic")]
    NotSubscribed,

    /// Channel closed
    #[error("channel closed")]
    ChannelClosed,

    /// Generic gossip error
    #[error("gossip error: {0}")]
    Other(String),
}

impl From<postcard::Error> for GossipError {
    fn from(e: postcard::Error) -> Self {
        GossipError::EncodeFailed(e.to_string())
    }
}

/// Result type for gossip operations
pub type GossipResult<T> = Result<T, GossipError>;
