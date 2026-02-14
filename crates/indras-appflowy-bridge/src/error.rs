//! Error types for the AppFlowy bridge

use thiserror::Error;

/// Errors that can occur in the AppFlowy bridge
#[derive(Debug, Error)]
pub enum BridgeError {
    /// Failed to encode/decode an envelope
    #[error("envelope error: {0}")]
    Envelope(String),

    /// Failed to apply a Yrs update
    #[error("yrs update error: {0}")]
    YrsUpdate(String),

    /// Node communication error
    #[error("node error: {0}")]
    Node(String),

    /// Interface not found or not joined
    #[error("interface not found: {0}")]
    InterfaceNotFound(String),

    /// Channel closed unexpectedly
    #[error("channel closed")]
    ChannelClosed,

    /// Bridge not initialized
    #[error("bridge not initialized — call init() first")]
    NotInitialized,
}
