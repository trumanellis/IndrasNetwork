//! Per-artifact access control primitives.
//!
//! Defines fine-grained access modes for artifacts:
//!
//! - **Revocable**: View-only access that can be revoked by the steward
//! - **Permanent**: Co-stewardship with download and reshare rights
//! - **Timed**: Auto-expiring access after a deadline
//! - **Transfer**: Full stewardship transfer to another player

use crate::artifact::PlayerId;
use serde::{Deserialize, Serialize};

/// How a grantee may interact with an artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessMode {
    /// View-only; steward can revoke at any time.
    Revocable,
    /// Co-stewardship: download, reshare, and cannot be revoked.
    Permanent,
    /// Auto-expires after `expires_at` tick.
    Timed {
        /// Tick timestamp when access expires.
        expires_at: i64,
    },
    /// Full stewardship transfer (one-shot).
    Transfer,
}

impl AccessMode {
    /// Whether this mode permits downloading (saving a local copy).
    pub fn allows_download(&self) -> bool {
        matches!(self, Self::Permanent)
    }

    /// Whether this mode permits re-sharing to others.
    pub fn allows_reshare(&self) -> bool {
        matches!(self, Self::Permanent)
    }

    /// Whether a `Timed` grant has expired.
    ///
    /// Returns `false` for non-Timed modes (they never expire this way).
    pub fn is_expired(&self, now: i64) -> bool {
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

/// A single access grant from a steward to a grantee.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessGrant {
    /// Who receives access.
    pub grantee: PlayerId,
    /// What kind of access.
    pub mode: AccessMode,
    /// When the grant was created (tick timestamp).
    pub granted_at: i64,
    /// Who created the grant (usually the artifact steward).
    pub granted_by: PlayerId,
}

/// Lifecycle status of an artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactStatus {
    /// Available for access according to grants.
    Active,
    /// Recalled by the steward — revocable/timed grants removed.
    Recalled {
        /// When the recall happened.
        recalled_at: i64,
    },
    /// Stewardship transferred to another player.
    Transferred {
        /// New steward.
        to: PlayerId,
        /// When the transfer happened.
        transferred_at: i64,
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
    pub original_steward: PlayerId,
    /// Who gave us this copy (may equal original_steward).
    pub received_from: PlayerId,
    /// When we received it (tick timestamp).
    pub received_at: i64,
    /// How we received it.
    pub received_via: ProvenanceType,
}

/// How an artifact was received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceType {
    /// Received as a co-steward (Permanent grant).
    CoStewardship,
    /// Received via stewardship transfer.
    Transfer,
}

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
        assert!(!mode.is_expired(499));
        assert!(mode.is_expired(500));
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
}
