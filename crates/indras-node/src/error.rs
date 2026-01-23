//! Error types for the node coordinator

use thiserror::Error;

/// Errors that can occur in the node coordinator
#[derive(Debug, Error)]
pub enum NodeError {
    /// Transport layer error
    #[error("Transport error: {0}")]
    Transport(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(#[from] indras_storage::StorageError),

    /// Sync error
    #[error("Sync error: {0}")]
    Sync(String),

    /// Interface not found
    #[error("Interface not found: {0}")]
    InterfaceNotFound(String),

    /// Node not started
    #[error("Node not started")]
    NotStarted,

    /// Node already started
    #[error("Node already started")]
    AlreadyStarted,

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Channel error (broadcast/mpsc)
    #[error("Channel error: {0}")]
    Channel(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(String),
}

impl From<indras_transport::AdapterError> for NodeError {
    fn from(e: indras_transport::AdapterError) -> Self {
        NodeError::Transport(e.to_string())
    }
}

impl From<indras_core::error::InterfaceError> for NodeError {
    fn from(e: indras_core::error::InterfaceError) -> Self {
        NodeError::Sync(e.to_string())
    }
}

impl<T> From<tokio::sync::broadcast::error::SendError<T>> for NodeError {
    fn from(e: tokio::sync::broadcast::error::SendError<T>) -> Self {
        NodeError::Channel(format!("Broadcast send error: {}", e))
    }
}

impl From<postcard::Error> for NodeError {
    fn from(e: postcard::Error) -> Self {
        NodeError::Serialization(e.to_string())
    }
}

/// Result type alias for node operations
pub type NodeResult<T> = Result<T, NodeError>;
