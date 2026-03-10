//! Resolve [`ViewLevel`] from artifact grant lists.
//!
//! The profile is stored as an artifact in the [`ArtifactIndex`]. Its grant
//! list determines who sees Connections-level fields. This module provides
//! a pure function to map from grant state to view level.

use indras_artifacts::AccessGrant;
use indras_profile::ViewLevel;

/// Resolve the [`ViewLevel`] for a viewer based on the profile artifact's grant list.
///
/// - Steward (profile owner) → [`ViewLevel::Owner`]
/// - Has active, non-expired grant → [`ViewLevel::Connection`]
/// - No grant → [`ViewLevel::Public`]
pub fn resolve_view_level(
    viewer: &[u8; 32],
    steward: &[u8; 32],
    grants: &[AccessGrant],
    now: i64,
) -> ViewLevel {
    if viewer == steward {
        return ViewLevel::Owner;
    }
    for grant in grants {
        if grant.grantee == *viewer && !grant.mode.is_expired(now) {
            return ViewLevel::Connection;
        }
    }
    ViewLevel::Public
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_artifacts::AccessMode;

    fn make_grant(grantee: [u8; 32], mode: AccessMode) -> AccessGrant {
        AccessGrant {
            grantee,
            mode,
            granted_at: 100,
            granted_by: [0xAA; 32],
        }
    }

    #[test]
    fn steward_gets_owner() {
        let steward = [0x01; 32];
        let level = resolve_view_level(&steward, &steward, &[], 0);
        assert_eq!(level, ViewLevel::Owner);
    }

    #[test]
    fn grantee_gets_connection() {
        let viewer = [0x02; 32];
        let steward = [0x01; 32];
        let grants = vec![make_grant(viewer, AccessMode::Revocable)];
        let level = resolve_view_level(&viewer, &steward, &grants, 0);
        assert_eq!(level, ViewLevel::Connection);
    }

    #[test]
    fn permanent_grant_gives_connection() {
        let viewer = [0x02; 32];
        let steward = [0x01; 32];
        let grants = vec![make_grant(viewer, AccessMode::Permanent)];
        let level = resolve_view_level(&viewer, &steward, &grants, 0);
        assert_eq!(level, ViewLevel::Connection);
    }

    #[test]
    fn stranger_gets_public() {
        let viewer = [0x03; 32];
        let steward = [0x01; 32];
        let grants = vec![make_grant([0x02; 32], AccessMode::Revocable)];
        let level = resolve_view_level(&viewer, &steward, &grants, 0);
        assert_eq!(level, ViewLevel::Public);
    }

    #[test]
    fn expired_timed_grant_gives_public() {
        let viewer = [0x02; 32];
        let steward = [0x01; 32];
        let grants = vec![make_grant(viewer, AccessMode::Timed { expires_at: 50 })];
        // now=100, expires_at=50 → expired
        let level = resolve_view_level(&viewer, &steward, &grants, 100);
        assert_eq!(level, ViewLevel::Public);
    }

    #[test]
    fn active_timed_grant_gives_connection() {
        let viewer = [0x02; 32];
        let steward = [0x01; 32];
        let grants = vec![make_grant(viewer, AccessMode::Timed { expires_at: 200 })];
        // now=100, expires_at=200 → still active
        let level = resolve_view_level(&viewer, &steward, &grants, 100);
        assert_eq!(level, ViewLevel::Connection);
    }
}
