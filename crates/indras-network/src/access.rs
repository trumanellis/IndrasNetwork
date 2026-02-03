//! Per-artifact access control primitives.
//!
//! Defines fine-grained access modes for artifacts in the shared filesystem.
//! Each artifact can have multiple grants with different modes per person:
//!
//! - **Revocable**: View-only access that can be revoked by the owner
//! - **Permanent**: Co-ownership with download and reshare rights
//! - **Timed**: Auto-expiring access after a deadline
//! - **Transfer**: Full ownership transfer to another person

use crate::member::MemberId;
use serde::{Deserialize, Serialize};

/// How a grantee may interact with an artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessMode {
    /// View-only; owner can revoke at any time.
    Revocable,
    /// Co-ownership: download, reshare, and cannot be revoked.
    Permanent,
    /// Auto-expires after `expires_at` tick.
    Timed {
        /// Tick timestamp when access expires.
        expires_at: u64,
    },
    /// Full ownership transfer (one-shot).
    Transfer,
}

impl AccessMode {
    /// Whether this mode permits downloading (saving a local copy).
    ///
    /// Only `Permanent` grants allow downloads. Revocable and Timed
    /// grants are view-only with soft-enforced download restrictions.
    pub fn allows_download(&self) -> bool {
        matches!(self, Self::Permanent)
    }

    /// Whether this mode permits re-sharing to others.
    ///
    /// Only `Permanent` co-owners may reshare artifacts.
    pub fn allows_reshare(&self) -> bool {
        matches!(self, Self::Permanent)
    }

    /// Whether a `Timed` grant has expired.
    ///
    /// Returns `false` for non-Timed modes (they never expire this way).
    pub fn is_expired(&self, now: u64) -> bool {
        match self {
            Self::Timed { expires_at } => now >= *expires_at,
            _ => false,
        }
    }

    /// Human-readable label for UI display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Revocable => "revocable",
            Self::Permanent => "permanent",
            Self::Timed { .. } => "timed",
            Self::Transfer => "transfer",
        }
    }
}

impl Default for AccessMode {
    fn default() -> Self {
        Self::Revocable
    }
}

/// A single access grant from an owner to a grantee.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessGrant {
    /// Who receives access.
    pub grantee: MemberId,
    /// What kind of access.
    pub mode: AccessMode,
    /// When the grant was created (tick timestamp).
    pub granted_at: u64,
    /// Who created the grant (usually the artifact owner).
    pub granted_by: MemberId,
}

/// Lifecycle status of an artifact in the owner's index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactStatus {
    /// Available for access according to grants.
    Active,
    /// Recalled by the owner — revocable/timed grants removed.
    Recalled {
        /// When the recall happened.
        recalled_at: u64,
    },
    /// Ownership transferred to another member.
    Transferred {
        /// New owner.
        to: MemberId,
        /// When the transfer happened.
        transferred_at: u64,
    },
}

impl Default for ArtifactStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl ArtifactStatus {
    /// Whether the artifact is currently active (not recalled or transferred).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// How the current holder received the artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactProvenance {
    /// The original creator/uploader of the artifact.
    pub original_owner: MemberId,
    /// Who gave us this copy (may equal original_owner).
    pub received_from: MemberId,
    /// When we received it (tick timestamp).
    pub received_at: u64,
    /// How we received it.
    pub received_via: ProvenanceType,
}

/// How an artifact was received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceType {
    /// Received as a co-owner (Permanent grant).
    CoOwnership,
    /// Received via ownership transfer.
    Transfer,
}

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

/// Errors when performing holonic operations (compose, attach, detach).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HolonicError {
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

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "artifact not found"),
            Self::NotActive => write!(f, "artifact is not active"),
        }
    }
}

impl std::error::Error for TransferError {}

impl std::fmt::Display for HolonicError {
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

impl std::error::Error for HolonicError {}

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
    fn test_holonic_error_display() {
        assert_eq!(HolonicError::NotFound.to_string(), "artifact not found");
        assert_eq!(HolonicError::NotActive.to_string(), "artifact is not active");
        assert_eq!(HolonicError::CycleDetected.to_string(), "operation would create a cycle");
        assert_eq!(HolonicError::AlreadyHasParent.to_string(), "artifact already has a parent");
        assert_eq!(HolonicError::NotAChild.to_string(), "artifact is not a child of the specified parent");
    }
}
