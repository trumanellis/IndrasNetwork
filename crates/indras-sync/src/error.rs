//! Error types for indras-sync

use thiserror::Error;

/// Errors that can occur during synchronization
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("Failed to load document: {0}")]
    DocumentLoad(String),

    #[error("Document operation failed: {0}")]
    DocumentOperation(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Sync merge failed: {0}")]
    SyncMerge(String),

    #[error("Interface not found: {0}")]
    InterfaceNotFound(String),

    #[error("Not a member of interface")]
    NotMember,

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Protocol error: {0}")]
    Protocol(String),
}

/// Result type for sync operations
pub type SyncResult<T> = Result<T, SyncError>;
