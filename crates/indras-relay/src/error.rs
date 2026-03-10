//! Relay-specific error types

use thiserror::Error;

use indras_core::InterfaceId;

/// Errors that can occur in the relay
#[derive(Debug, Error)]
pub enum RelayError {
    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Quota exceeded
    #[error("Quota exceeded for peer: {reason}")]
    QuotaExceeded { reason: String },

    /// Registration error
    #[error("Registration error: {0}")]
    Registration(String),

    /// Interface not registered
    #[error("Interface not registered: {0:?}")]
    InterfaceNotRegistered(InterfaceId),

    /// Transport error
    #[error("Transport error: {0}")]
    Transport(String),

    /// Config error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Admin API error
    #[error("Admin API error: {0}")]
    Admin(String),

    /// Authentication failed
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Tier access denied
    #[error("Tier access denied: {0}")]
    TierAccessDenied(String),

    /// Invalid credential
    #[error("Invalid credential: {0}")]
    InvalidCredential(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),

    /// Database table error
    #[error("Database table error: {0}")]
    TableError(#[from] redb::TableError),

    /// Database storage error
    #[error("Database storage error: {0}")]
    StorageError(#[from] redb::StorageError),

    /// Database commit error
    #[error("Database commit error: {0}")]
    CommitError(#[from] redb::CommitError),

    /// Database transaction error
    #[error("Database transaction error: {0}")]
    TransactionError(#[from] redb::TransactionError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result type for relay operations
pub type RelayResult<T> = Result<T, RelayError>;
