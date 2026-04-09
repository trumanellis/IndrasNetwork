//! Grant-based visibility checks for profile fields and content artifacts.
//!
//! Delegates to the canonical [`indras_artifacts::access::can_view`] for all
//! visibility decisions. Profile fields and content artifacts are just artifacts
//! with grant lists — there is one unified access model.

pub use indras_artifacts::access::can_view;

use crate::{ContentArtifact, ProfileFieldArtifact};

/// Filter profile fields by viewer access.
pub fn visible_fields<'a>(
    viewer: Option<&[u8; 32]>,
    steward: &[u8; 32],
    fields: &'a [ProfileFieldArtifact],
    now: i64,
) -> Vec<&'a ProfileFieldArtifact> {
    fields
        .iter()
        .filter(|f| can_view(viewer, steward, &f.grants, now))
        .collect()
}

/// Filter content artifacts by viewer access.
pub fn visible_artifacts<'a>(
    viewer: Option<&[u8; 32]>,
    steward: &[u8; 32],
    artifacts: &'a [ContentArtifact],
    now: i64,
) -> Vec<&'a ContentArtifact> {
    artifacts
        .iter()
        .filter(|a| can_view(viewer, steward, &a.grants, now))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_artifacts::access::{AccessGrant, AccessMode};

    fn make_grant(grantee: [u8; 32], mode: AccessMode) -> AccessGrant {
        AccessGrant {
            grantee,
            mode,
            granted_at: 100,
            granted_by: [0xAA; 32],
        }
    }

    #[test]
    fn public_grant_visible_to_all() {
        let steward = [0x01; 32];
        let grants = vec![make_grant([0x00; 32], AccessMode::Public)];
        // Anonymous viewer
        assert!(can_view(None, &steward, &grants, 0));
        // Random viewer
        assert!(can_view(Some(&[0x99; 32]), &steward, &grants, 0));
    }

    #[test]
    fn specific_grant_visible_to_grantee() {
        let steward = [0x01; 32];
        let viewer = [0x02; 32];
        let grants = vec![make_grant(viewer, AccessMode::Revocable)];
        assert!(can_view(Some(&viewer), &steward, &grants, 0));
    }

    #[test]
    fn stranger_cannot_see_non_public() {
        let steward = [0x01; 32];
        let stranger = [0x03; 32];
        let grants = vec![make_grant([0x02; 32], AccessMode::Revocable)];
        assert!(!can_view(Some(&stranger), &steward, &grants, 0));
    }

    #[test]
    fn anonymous_cannot_see_non_public() {
        let steward = [0x01; 32];
        let grants = vec![make_grant([0x02; 32], AccessMode::Revocable)];
        assert!(!can_view(None, &steward, &grants, 0));
    }

    #[test]
    fn expired_grant_rejected() {
        let steward = [0x01; 32];
        let viewer = [0x02; 32];
        let grants = vec![make_grant(viewer, AccessMode::Timed { expires_at: 50 })];
        // now=100, expires_at=50 → expired
        assert!(!can_view(Some(&viewer), &steward, &grants, 100));
    }

    #[test]
    fn active_timed_grant_accepted() {
        let steward = [0x01; 32];
        let viewer = [0x02; 32];
        let grants = vec![make_grant(viewer, AccessMode::Timed { expires_at: 200 })];
        // now=100, expires_at=200 → still active
        assert!(can_view(Some(&viewer), &steward, &grants, 100));
    }

    #[test]
    fn owner_sees_everything() {
        let steward = [0x01; 32];
        // Even with no grants at all
        assert!(can_view(Some(&steward), &steward, &[], 0));
    }

    #[test]
    fn filter_fields_by_access() {
        let steward = [0x01; 32];
        let viewer = [0x02; 32];
        let fields = vec![
            ProfileFieldArtifact {
                field_name: "display_name".to_string(),
                display_value: "Alice".to_string(),
                grants: vec![make_grant([0x00; 32], AccessMode::Public)],
            },
            ProfileFieldArtifact {
                field_name: "bio".to_string(),
                display_value: "Secret bio".to_string(),
                grants: vec![make_grant([0x05; 32], AccessMode::Revocable)],
            },
        ];
        let visible = visible_fields(Some(&viewer), &steward, &fields, 0);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].field_name, "display_name");
    }

    #[test]
    fn filter_artifacts_by_access() {
        let steward = [0x01; 32];
        let artifacts = vec![
            ContentArtifact {
                artifact_id: [0xA0; 32],
                name: "public.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
                size: 100,
                created_at: 0,
                grants: vec![make_grant([0x00; 32], AccessMode::Public)],
            },
            ContentArtifact {
                artifact_id: [0xA1; 32],
                name: "private.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
                size: 200,
                created_at: 0,
                grants: vec![make_grant([0x02; 32], AccessMode::Revocable)],
            },
        ];
        // Anonymous sees only public
        let visible = visible_artifacts(None, &steward, &artifacts, 0);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "public.txt");
    }
}
