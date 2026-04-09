//! Per-artifact access control primitives.
//!
//! Defines fine-grained access modes for artifacts:
//!
//! - **Public**: Visible to anyone without a grant
//! - **Revocable**: View-only access that can be revoked by the steward
//! - **Permanent**: Co-stewardship with download and reshare rights
//! - **Timed**: Auto-expiring access after a deadline
//! - **Transfer**: Full stewardship transfer to another player

use crate::artifact::PlayerId;
use serde::{Deserialize, Serialize};

/// How a grantee may interact with an artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessMode {
    /// Visible to anyone. The `grantee` field is ignored.
    Public,
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
            Self::Public => "public",
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

/// Check if a viewer can see an item based on its grant list.
///
/// - Any `AccessMode::Public` grant → visible to everyone
/// - Steward always sees their own items
/// - Otherwise, viewer must have a non-expired grant
pub fn can_view(
    viewer: Option<&[u8; 32]>,
    steward: &[u8; 32],
    grants: &[AccessGrant],
    now: i64,
) -> bool {
    // Public grant → anyone can see
    if grants.iter().any(|g| matches!(g.mode, AccessMode::Public)) {
        return true;
    }
    let Some(viewer) = viewer else {
        return false;
    };
    // Owner always sees everything
    if viewer == steward {
        return true;
    }
    // Check for a non-expired grant to this viewer
    grants
        .iter()
        .any(|g| g.grantee == *viewer && !g.mode.is_expired(now))
}

/// Extract non-expired grantee player IDs from a list of access grants.
///
/// Filters out `Timed` grants that have expired according to `now`, then
/// returns the unique set of grantee IDs. This is used to derive the
/// relay's contacts list from the profile artifact's grant list.
pub fn extract_contact_ids(grants: &[AccessGrant], now: i64) -> Vec<[u8; 32]> {
    let mut ids: Vec<[u8; 32]> = grants
        .iter()
        .filter(|g| !g.mode.is_expired(now))
        .map(|g| g.grantee)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
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
    fn test_public_no_download_no_reshare_never_expires() {
        let mode = AccessMode::Public;
        assert!(!mode.allows_download());
        assert!(!mode.allows_reshare());
        assert!(!mode.is_expired(0));
        assert!(!mode.is_expired(i64::MAX));
        assert_eq!(mode.label(), "public");
    }

    #[test]
    fn test_access_mode_labels() {
        assert_eq!(AccessMode::Public.label(), "public");
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
    fn test_extract_contact_ids_filters_expired() {
        let grants = vec![
            AccessGrant {
                grantee: [1u8; 32],
                mode: AccessMode::Revocable,
                granted_at: 0,
                granted_by: [0u8; 32],
            },
            AccessGrant {
                grantee: [2u8; 32],
                mode: AccessMode::Timed { expires_at: 500 },
                granted_at: 0,
                granted_by: [0u8; 32],
            },
            AccessGrant {
                grantee: [3u8; 32],
                mode: AccessMode::Permanent,
                granted_at: 0,
                granted_by: [0u8; 32],
            },
        ];

        // At tick 499, timed grant is still valid
        let ids = extract_contact_ids(&grants, 499);
        assert_eq!(ids.len(), 3);

        // At tick 500, timed grant expired
        let ids = extract_contact_ids(&grants, 500);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&[1u8; 32]));
        assert!(ids.contains(&[3u8; 32]));
    }

    #[test]
    fn test_extract_contact_ids_deduplicates() {
        let grants = vec![
            AccessGrant {
                grantee: [1u8; 32],
                mode: AccessMode::Revocable,
                granted_at: 0,
                granted_by: [0u8; 32],
            },
            AccessGrant {
                grantee: [1u8; 32],
                mode: AccessMode::Permanent,
                granted_at: 10,
                granted_by: [0u8; 32],
            },
        ];

        let ids = extract_contact_ids(&grants, 0);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_extract_contact_ids_empty() {
        let ids = extract_contact_ids(&[], 0);
        assert!(ids.is_empty());
    }

    fn make_grant(grantee: [u8; 32], mode: AccessMode) -> AccessGrant {
        AccessGrant {
            grantee,
            mode,
            granted_at: 100,
            granted_by: [0xAA; 32],
        }
    }

    #[test]
    fn test_can_view_public_visible_to_all() {
        let steward = [0x01; 32];
        let grants = vec![make_grant([0x00; 32], AccessMode::Public)];
        assert!(can_view(None, &steward, &grants, 0));
        assert!(can_view(Some(&[0x99; 32]), &steward, &grants, 0));
    }

    #[test]
    fn test_can_view_specific_grant() {
        let steward = [0x01; 32];
        let viewer = [0x02; 32];
        let grants = vec![make_grant(viewer, AccessMode::Revocable)];
        assert!(can_view(Some(&viewer), &steward, &grants, 0));
    }

    #[test]
    fn test_can_view_stranger_rejected() {
        let steward = [0x01; 32];
        let grants = vec![make_grant([0x02; 32], AccessMode::Revocable)];
        assert!(!can_view(Some(&[0x03; 32]), &steward, &grants, 0));
        assert!(!can_view(None, &steward, &grants, 0));
    }

    #[test]
    fn test_can_view_expired_rejected() {
        let steward = [0x01; 32];
        let viewer = [0x02; 32];
        let grants = vec![make_grant(viewer, AccessMode::Timed { expires_at: 50 })];
        assert!(!can_view(Some(&viewer), &steward, &grants, 100));
    }

    #[test]
    fn test_can_view_active_timed_accepted() {
        let steward = [0x01; 32];
        let viewer = [0x02; 32];
        let grants = vec![make_grant(viewer, AccessMode::Timed { expires_at: 200 })];
        assert!(can_view(Some(&viewer), &steward, &grants, 100));
    }

    #[test]
    fn test_can_view_owner_sees_everything() {
        let steward = [0x01; 32];
        assert!(can_view(Some(&steward), &steward, &[], 0));
    }
}
