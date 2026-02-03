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

    #[error("Internal lock poisoned")]
    LockPoisoned,
}

/// Result type for sync operations
pub type SyncResult<T> = Result<T, SyncError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_error_display() {
        let err = SyncError::DocumentLoad("corrupt data".to_string());
        assert!(format!("{}", err).contains("Failed to load document"));
        assert!(format!("{}", err).contains("corrupt data"));

        let err = SyncError::DocumentOperation("write failed".to_string());
        assert!(format!("{}", err).contains("Document operation failed"));

        let err = SyncError::Serialization("json error".to_string());
        assert!(format!("{}", err).contains("Serialization error"));

        let err = SyncError::Deserialization("parse error".to_string());
        assert!(format!("{}", err).contains("Deserialization error"));

        let err = SyncError::SyncMerge("conflict".to_string());
        assert!(format!("{}", err).contains("Sync merge failed"));

        let err = SyncError::InterfaceNotFound("iface123".to_string());
        assert!(format!("{}", err).contains("Interface not found"));
        assert!(format!("{}", err).contains("iface123"));

        let err = SyncError::NotMember;
        assert!(format!("{}", err).contains("Not a member"));

        let err = SyncError::PeerNotFound("peer_x".to_string());
        assert!(format!("{}", err).contains("Peer not found"));

        let err = SyncError::Protocol("handshake failed".to_string());
        assert!(format!("{}", err).contains("Protocol error"));
    }

    #[test]
    fn test_sync_error_debug() {
        // Ensure Debug is implemented and doesn't panic
        let err = SyncError::NotMember;
        let debug_str = format!("{:?}", err);
        assert!(!debug_str.is_empty());
    }
}
