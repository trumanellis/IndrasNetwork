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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custody_error_display() {
        let err = CustodyError::StorageFull { max: 1000 };
        let msg = format!("{}", err);
        assert!(msg.contains("Custody storage full"));
        assert!(msg.contains("1000"));

        let err = CustodyError::AlreadyHaveCustody;
        assert!(format!("{}", err).contains("Already have custody"));

        let err = CustodyError::NotInCustody;
        assert!(format!("{}", err).contains("not in custody"));

        let err = CustodyError::TransferTimeout;
        assert!(format!("{}", err).contains("timed out"));

        let err = CustodyError::Refused {
            reason: "storage full".to_string(),
        };
        assert!(format!("{}", err).contains("refused"));
        assert!(format!("{}", err).contains("storage full"));

        let err = CustodyError::NoPendingTransfer;
        assert!(format!("{}", err).contains("No pending transfer"));
    }

    #[test]
    fn test_bundle_error_display() {
        let err = BundleError::Expired;
        assert!(format!("{}", err).contains("expired"));

        let err = BundleError::Invalid("missing header".to_string());
        assert!(format!("{}", err).contains("Invalid bundle"));
        assert!(format!("{}", err).contains("missing header"));

        let err = BundleError::TooLarge {
            size: 2000,
            max: 1000,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("exceeds maximum size"));
        assert!(msg.contains("2000"));
        assert!(msg.contains("1000"));

        let err = BundleError::Duplicate;
        assert!(format!("{}", err).contains("Duplicate"));
    }

    #[test]
    fn test_dtn_error_display() {
        let custody_err = CustodyError::NotInCustody;
        let dtn_err: DtnError = custody_err.into();
        assert!(format!("{}", dtn_err).contains("Custody error"));

        let bundle_err = BundleError::Expired;
        let dtn_err: DtnError = bundle_err.into();
        assert!(format!("{}", dtn_err).contains("Bundle error"));
    }

    #[test]
    fn test_dtn_error_from_routing() {
        let routing_err = indras_core::RoutingError::NoRoute;
        let dtn_err: DtnError = routing_err.into();
        assert!(format!("{}", dtn_err).contains("Routing error"));
    }

    #[test]
    fn test_dtn_error_from_storage() {
        let storage_err = indras_core::StorageError::CapacityExceeded;
        let dtn_err: DtnError = storage_err.into();
        assert!(format!("{}", dtn_err).contains("Storage error"));
    }

    #[test]
    fn test_error_debug() {
        // Ensure Debug is implemented for all error types
        let err = CustodyError::NotInCustody;
        assert!(!format!("{:?}", err).is_empty());

        let err = BundleError::Expired;
        assert!(!format!("{:?}", err).is_empty());

        let err: DtnError = BundleError::Expired.into();
        assert!(!format!("{:?}", err).is_empty());
    }
}
