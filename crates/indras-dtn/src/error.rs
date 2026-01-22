//! DTN-specific error types

use thiserror::Error;

/// Errors that can occur in the DTN subsystem
#[derive(Debug, Error)]
pub enum DtnError {
    /// Custody-related errors
    #[error("Custody error: {0}")]
    Custody(#[from] CustodyError),

    /// Bundle-related errors
    #[error("Bundle error: {0}")]
    Bundle(#[from] BundleError),

    /// Routing errors
    #[error("Routing error: {0}")]
    Routing(#[from] indras_core::RoutingError),

    /// Storage errors
    #[error("Storage error: {0}")]
    Storage(#[from] indras_core::StorageError),
}

/// Custody transfer errors
#[derive(Debug, Error)]
pub enum CustodyError {
    /// Maximum custody capacity reached
    #[error("Custody storage full (max: {max})")]
    StorageFull { max: usize },

    /// Bundle already has custody
    #[error("Already have custody of bundle")]
    AlreadyHaveCustody,

    /// Bundle not found in custody
    #[error("Bundle not in custody")]
    NotInCustody,

    /// Custody transfer timed out
    #[error("Custody transfer timed out")]
    TransferTimeout,

    /// Custody was refused
    #[error("Custody was refused: {reason}")]
    Refused { reason: String },

    /// Pending transfer not found
    #[error("No pending transfer for bundle")]
    NoPendingTransfer,
}

/// Bundle-related errors
#[derive(Debug, Error)]
pub enum BundleError {
    /// Bundle has expired
    #[error("Bundle has expired")]
    Expired,

    /// Invalid bundle format
    #[error("Invalid bundle: {0}")]
    Invalid(String),

    /// Bundle too large
    #[error("Bundle exceeds maximum size (size: {size}, max: {max})")]
    TooLarge { size: usize, max: usize },

    /// Bundle already seen (duplicate)
    #[error("Duplicate bundle")]
    Duplicate,
}

/// Result type for DTN operations
pub type DtnResult<T> = Result<T, DtnError>;
