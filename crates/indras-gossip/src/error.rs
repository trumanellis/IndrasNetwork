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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_error_display() {
        let err = GossipError::SubscribeFailed("timeout".to_string());
        assert!(format!("{}", err).contains("failed to subscribe"));
        assert!(format!("{}", err).contains("timeout"));

        let err = GossipError::BroadcastFailed("network down".to_string());
        assert!(format!("{}", err).contains("failed to broadcast"));

        let err = GossipError::EncodeFailed("invalid utf8".to_string());
        assert!(format!("{}", err).contains("failed to encode"));

        let err = GossipError::DecodeFailed("corrupt data".to_string());
        assert!(format!("{}", err).contains("failed to decode"));

        let err = GossipError::SignatureVerificationFailed("bad sig".to_string());
        assert!(format!("{}", err).contains("signature verification failed"));

        let err = GossipError::TopicNotFound("topic123".to_string());
        assert!(format!("{}", err).contains("topic not found"));
        assert!(format!("{}", err).contains("topic123"));

        let err = GossipError::AlreadySubscribed;
        assert!(format!("{}", err).contains("already subscribed"));

        let err = GossipError::NotSubscribed;
        assert!(format!("{}", err).contains("not subscribed"));

        let err = GossipError::ChannelClosed;
        assert!(format!("{}", err).contains("channel closed"));

        let err = GossipError::Other("unknown error".to_string());
        assert!(format!("{}", err).contains("unknown error"));
    }

    #[test]
    fn test_gossip_error_from_postcard() {
        // Create a postcard error by trying to deserialize invalid data
        let bad_data = [0xff, 0xff, 0xff];
        let postcard_err = postcard::from_bytes::<u64>(&bad_data).unwrap_err();

        let gossip_err: GossipError = postcard_err.into();
        assert!(matches!(gossip_err, GossipError::EncodeFailed(_)));
    }

    #[test]
    fn test_gossip_error_debug() {
        let err = GossipError::ChannelClosed;
        let debug_str = format!("{:?}", err);
        assert!(!debug_str.is_empty());
    }
}
