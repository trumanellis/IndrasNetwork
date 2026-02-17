//! Per-artifact access control primitives.
//!
//! Re-exports core access types from `indras_artifacts` and defines
//! network-specific error types for grant/revoke/transfer/tree operations.

// Re-export canonical types from indras-artifacts
pub use indras_artifacts::{
    AccessGrant, AccessMode, ArtifactProvenance, ArtifactStatus, ProvenanceType,
};

/// Errors when granting access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantError {
    /// Artifact not found in the index.
    NotFound,
    /// Artifact has been recalled.
    Recalled,
    /// Artifact has been transferred away.
    Transferred,
    /// Grantee already has an active grant for this artifact.
    AlreadyGranted,
}

impl std::fmt::Display for GrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "artifact not found"),
            Self::Recalled => write!(f, "artifact has been recalled"),
            Self::Transferred => write!(f, "artifact has been transferred"),
            Self::AlreadyGranted => write!(f, "grantee already has access"),
        }
    }
}

impl std::error::Error for GrantError {}

/// Errors when revoking access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevokeError {
    /// Artifact not found in the index.
    NotFound,
    /// Cannot revoke a Permanent grant.
    CannotRevoke,
    /// Artifact is not in Active status.
    NotActive,
}

impl std::fmt::Display for RevokeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "artifact not found"),
            Self::CannotRevoke => write!(f, "cannot revoke permanent grant"),
            Self::NotActive => write!(f, "artifact is not active"),
        }
    }
}

impl std::error::Error for RevokeError {}

/// Errors when transferring ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferError {
    /// Artifact not found in the index.
    NotFound,
    /// Artifact is not in Active status.
    NotActive,
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "artifact not found"),
            Self::NotActive => write!(f, "artifact is not active"),
        }
    }
}

impl std::error::Error for TransferError {}

/// Errors when performing tree operations (attach, detach).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeError {
    /// Artifact not found in the index.
    NotFound,
    /// Artifact is not in Active status.
    NotActive,
    /// Operation would create a cycle in the parent chain.
    CycleDetected,
    /// Artifact already has a parent (single-parent invariant).
    AlreadyHasParent,
    /// Child is not attached to the specified parent.
    NotAChild,
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "artifact not found"),
            Self::NotActive => write!(f, "artifact is not active"),
            Self::CycleDetected => write!(f, "operation would create a cycle"),
            Self::AlreadyHasParent => write!(f, "artifact already has a parent"),
            Self::NotAChild => write!(f, "artifact is not a child of the specified parent"),
        }
    }
}

impl std::error::Error for TreeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revocable_no_download() {
        let mode = AccessMode::Revocable;
        assert!(!mode.allows_download());
        assert!(!mode.allows_reshare());
        assert!(!mode.is_expired(1000));
    }

    #[test]
    fn test_permanent_allows_download_and_reshare() {
        let mode = AccessMode::Permanent;
        assert!(mode.allows_download());
        assert!(mode.allows_reshare());
        assert!(!mode.is_expired(1000));
    }

    #[test]
    fn test_timed_expiry() {
        let mode = AccessMode::Timed { expires_at: 500 };
        assert!(!mode.allows_download());
        assert!(!mode.allows_reshare());

        // Before expiry
        assert!(!mode.is_expired(499));

        // At expiry boundary
        assert!(mode.is_expired(500));

        // After expiry
        assert!(mode.is_expired(501));
    }

    #[test]
    fn test_transfer_no_download() {
        let mode = AccessMode::Transfer;
        assert!(!mode.allows_download());
        assert!(!mode.allows_reshare());
        assert!(!mode.is_expired(1000));
    }

    #[test]
    fn test_artifact_status_active() {
        let status = ArtifactStatus::Active;
        assert!(status.is_active());

        let recalled = ArtifactStatus::Recalled { recalled_at: 100 };
        assert!(!recalled.is_active());

        let transferred = ArtifactStatus::Transferred {
            to: [1u8; 32],
            transferred_at: 100,
        };
        assert!(!transferred.is_active());
    }

    #[test]
    fn test_access_mode_labels() {
        assert_eq!(AccessMode::Revocable.label(), "revocable");
        assert_eq!(AccessMode::Permanent.label(), "permanent");
        assert_eq!(AccessMode::Timed { expires_at: 0 }.label(), "timed");
        assert_eq!(AccessMode::Transfer.label(), "transfer");
    }

    #[test]
    fn test_access_mode_default_is_revocable() {
        assert_eq!(AccessMode::default(), AccessMode::Revocable);
    }

    #[test]
    fn test_grant_error_display() {
        assert_eq!(GrantError::NotFound.to_string(), "artifact not found");
        assert_eq!(GrantError::Recalled.to_string(), "artifact has been recalled");
        assert_eq!(GrantError::Transferred.to_string(), "artifact has been transferred");
        assert_eq!(GrantError::AlreadyGranted.to_string(), "grantee already has access");
    }

    #[test]
    fn test_revoke_error_display() {
        assert_eq!(RevokeError::NotFound.to_string(), "artifact not found");
        assert_eq!(RevokeError::CannotRevoke.to_string(), "cannot revoke permanent grant");
        assert_eq!(RevokeError::NotActive.to_string(), "artifact is not active");
    }

    #[test]
    fn test_transfer_error_display() {
        assert_eq!(TransferError::NotFound.to_string(), "artifact not found");
        assert_eq!(TransferError::NotActive.to_string(), "artifact is not active");
    }

    #[test]
    fn test_tree_error_display() {
        assert_eq!(TreeError::NotFound.to_string(), "artifact not found");
        assert_eq!(TreeError::NotActive.to_string(), "artifact is not active");
        assert_eq!(TreeError::CycleDetected.to_string(), "operation would create a cycle");
        assert_eq!(TreeError::AlreadyHasParent.to_string(), "artifact already has a parent");
        assert_eq!(TreeError::NotAChild.to_string(), "artifact is not a child of the specified parent");
    }
}
