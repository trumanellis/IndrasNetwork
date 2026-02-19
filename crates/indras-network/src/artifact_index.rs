//! Home-realm artifact index — the source of truth for a user's shared filesystem.
//!
//! Each user maintains an `ArtifactIndex` document in their home realm.
//! Realms become *views* over this index, filtered by "all realm members
//! have access".
//!
//! ## One blob, one metadata record
//!
//! Artifacts are never duplicated across realms. The home realm stores
//! the single blob and its `HomeArtifactEntry`, which carries the
//! grants list. Sharing to a realm simply adds grants for each member.

use crate::access::{
    AccessGrant, AccessMode, ArtifactProvenance, ArtifactStatus, GrantError,
    ProvenanceType, RevokeError, TransferError,
};
use crate::artifact::ArtifactId;
use crate::encryption::EncryptedArtifactKey;
use crate::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A geographic coordinate (WGS-84 latitude/longitude).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoLocation {
    pub lat: f64,
    pub lng: f64,
}

impl GeoLocation {
    /// Haversine distance in kilometres between two points.
    pub fn distance_km(&self, other: &GeoLocation) -> f64 {
        let r = 6371.0;
        let d_lat = (other.lat - self.lat).to_radians();
        let d_lng = (other.lng - self.lng).to_radians();
        let a = (d_lat / 2.0).sin().powi(2)
            + self.lat.to_radians().cos()
                * other.lat.to_radians().cos()
                * (d_lng / 2.0).sin().powi(2);
        r * 2.0 * a.sqrt().asin()
    }
}

/// A single artifact entry in the owner's index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HomeArtifactEntry {
    /// Content hash, used as the unique identifier.
    pub id: ArtifactId,
    /// Human-readable name.
    pub name: String,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// When the artifact was created/uploaded (tick timestamp).
    pub created_at: i64,
    /// Encrypted per-artifact key (for revocable sharing).
    pub encrypted_key: Option<EncryptedArtifactKey>,
    /// Lifecycle status.
    pub status: ArtifactStatus,
    /// Access grants for other members.
    pub grants: Vec<AccessGrant>,
    /// How we received this artifact (None if we created it).
    pub provenance: Option<ArtifactProvenance>,
    /// Geographic location where this artifact was created/tagged.
    #[serde(default)]
    pub location: Option<GeoLocation>,
}

impl HomeArtifactEntry {
    /// Check if a specific member has an active (non-expired) grant.
    pub fn has_active_grant(&self, member: &MemberId, now: i64) -> bool {
        self.grants.iter().any(|g| {
            &g.grantee == member && !g.mode.is_expired(now)
        })
    }

    /// Get the grant for a specific member, if any.
    pub fn grant_for(&self, member: &MemberId) -> Option<&AccessGrant> {
        self.grants.iter().find(|g| &g.grantee == member)
    }

    /// Get the content hash as a hex string.
    pub fn hash_hex(&self) -> String {
        self.id.bytes().iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Get a short hash for display (first 8 hex chars).
    pub fn short_hash(&self) -> String {
        self.hash_hex()[..8].to_string()
    }
}

/// The home-realm artifact index document.
///
/// This is the CRDT document that stores all artifacts owned by a user,
/// their grants, and lifecycle status.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArtifactIndex {
    /// All artifacts, keyed by content hash.
    pub artifacts: HashMap<ArtifactId, HomeArtifactEntry>,
}

impl ArtifactIndex {
    /// Store a new artifact entry.
    ///
    /// Returns `true` if the entry was added, `false` if an artifact
    /// with the same ID already exists (idempotent).
    pub fn store(&mut self, entry: HomeArtifactEntry) -> bool {
        if self.artifacts.contains_key(&entry.id) {
            return false;
        }
        self.artifacts.insert(entry.id, entry);
        true
    }

    /// Get an artifact entry by ID.
    pub fn get(&self, id: &ArtifactId) -> Option<&HomeArtifactEntry> {
        self.artifacts.get(id)
    }

    /// Get a mutable reference to an artifact entry.
    pub fn get_mut(&mut self, id: &ArtifactId) -> Option<&mut HomeArtifactEntry> {
        self.artifacts.get_mut(id)
    }

    /// Grant access to an artifact for a specific member.
    pub fn grant(
        &mut self,
        id: &ArtifactId,
        grantee: MemberId,
        mode: AccessMode,
        granted_by: MemberId,
        now: i64,
    ) -> Result<(), GrantError> {
        let entry = self.artifacts.get_mut(id).ok_or(GrantError::NotFound)?;

        match &entry.status {
            ArtifactStatus::Active => {}
            ArtifactStatus::Recalled { .. } => return Err(GrantError::Recalled),
            ArtifactStatus::Transferred { .. } => return Err(GrantError::Transferred),
        }

        if entry.has_active_grant(&grantee, now) {
            return Err(GrantError::AlreadyGranted);
        }

        entry.grants.push(AccessGrant {
            grantee,
            mode,
            granted_at: now,
            granted_by,
        });

        Ok(())
    }

    /// Revoke a specific member's access to an artifact.
    pub fn revoke_access(
        &mut self,
        id: &ArtifactId,
        grantee: &MemberId,
    ) -> Result<(), RevokeError> {
        let entry = self.artifacts.get_mut(id).ok_or(RevokeError::NotFound)?;

        if !entry.status.is_active() {
            return Err(RevokeError::NotActive);
        }

        let grant_idx = entry
            .grants
            .iter()
            .position(|g| &g.grantee == grantee);

        match grant_idx {
            Some(idx) => {
                if matches!(entry.grants[idx].mode, AccessMode::Permanent) {
                    return Err(RevokeError::CannotRevoke);
                }
                entry.grants.remove(idx);
                Ok(())
            }
            None => Err(RevokeError::NotFound),
        }
    }

    /// Recall an artifact — remove all revocable/timed grants, keep permanent.
    pub fn recall(&mut self, id: &ArtifactId, recalled_at: i64) -> bool {
        let entry = match self.artifacts.get_mut(id) {
            Some(e) => e,
            None => return false,
        };

        if !entry.status.is_active() {
            return false;
        }

        entry.grants.retain(|g| matches!(g.mode, AccessMode::Permanent));
        entry.status = ArtifactStatus::Recalled { recalled_at };

        true
    }

    /// Transfer ownership of an artifact to another member.
    pub fn transfer(
        &mut self,
        id: &ArtifactId,
        to: MemberId,
        steward: MemberId,
        now: i64,
    ) -> Result<HomeArtifactEntry, TransferError> {
        let entry = self.artifacts.get_mut(id).ok_or(TransferError::NotFound)?;

        if !entry.status.is_active() {
            return Err(TransferError::NotActive);
        }

        let mut recipient_entry = HomeArtifactEntry {
            id: entry.id,
            name: entry.name.clone(),
            mime_type: entry.mime_type.clone(),
            size: entry.size,
            created_at: now,
            encrypted_key: entry.encrypted_key.clone(),
            status: ArtifactStatus::Active,
            grants: vec![
                AccessGrant {
                    grantee: steward,
                    mode: AccessMode::Revocable,
                    granted_at: now,
                    granted_by: to,
                },
            ],
            provenance: Some(ArtifactProvenance {
                original_steward: steward,
                received_from: steward,
                received_at: now,
                received_via: ProvenanceType::Transfer,
            }),
            location: entry.location.clone(),
        };

        // Carry over inherited permanent grants
        for grant in &entry.grants {
            if matches!(grant.mode, AccessMode::Permanent) && grant.grantee != to {
                recipient_entry.grants.push(grant.clone());
            }
        }

        entry.status = ArtifactStatus::Transferred {
            to,
            transferred_at: now,
        };

        Ok(recipient_entry)
    }

    /// Get all artifacts accessible by a specific member at a given time.
    pub fn accessible_by(&self, member: &MemberId, now: i64) -> Vec<&HomeArtifactEntry> {
        self.artifacts
            .values()
            .filter(|entry| {
                entry.status.is_active() && entry.has_active_grant(member, now)
            })
            .collect()
    }


    /// Remove expired timed grants.
    pub fn gc_expired(&mut self, now: i64) {
        for entry in self.artifacts.values_mut() {
            entry.grants.retain(|g| !g.mode.is_expired(now));
        }
    }

    /// Get count of active (non-recalled, non-transferred) artifacts.
    pub fn active_count(&self) -> usize {
        self.artifacts
            .values()
            .filter(|e| e.status.is_active())
            .count()
    }

    /// Get all active artifacts owned by this index.
    pub fn active_artifacts(&self) -> impl Iterator<Item = &HomeArtifactEntry> {
        self.artifacts.values().filter(|e| e.status.is_active())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id() -> ArtifactId {
        ArtifactId::Blob([0x42u8; 32])
    }

    fn other_id() -> ArtifactId {
        ArtifactId::Blob([0x43u8; 32])
    }

    fn member_a() -> MemberId {
        [1u8; 32]
    }

    fn member_b() -> MemberId {
        [2u8; 32]
    }

    fn member_c() -> MemberId {
        [3u8; 32]
    }

    fn steward() -> MemberId {
        [0xFFu8; 32]
    }

    fn test_entry() -> HomeArtifactEntry {
        HomeArtifactEntry {
            id: test_id(),
            name: "test.pdf".to_string(),
            mime_type: Some("application/pdf".to_string()),
            size: 1024,
            created_at: 100,
            encrypted_key: None,
            status: ArtifactStatus::Active,
            grants: Vec::new(),
            provenance: None,
            location: None,
        }
    }

    #[test]
    fn test_store_and_get() {
        let mut index = ArtifactIndex::default();
        let entry = test_entry();
        let id = entry.id;

        assert!(index.store(entry));
        assert!(index.get(&id).is_some());
        assert_eq!(index.get(&id).unwrap().name, "test.pdf");
    }

    #[test]
    fn test_store_idempotent() {
        let mut index = ArtifactIndex::default();
        assert!(index.store(test_entry()));
        assert!(!index.store(test_entry()));
    }

    #[test]
    fn test_grant_each_mode() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        assert!(index.grant(&id, member_a(), AccessMode::Revocable, steward(), 100).is_ok());
        assert!(index.grant(&id, member_b(), AccessMode::Permanent, steward(), 100).is_ok());
        assert!(index.grant(&id, member_c(), AccessMode::Timed { expires_at: 500 }, steward(), 100).is_ok());

        let entry = index.get(&id).unwrap();
        assert_eq!(entry.grants.len(), 3);
    }

    #[test]
    fn test_grant_already_granted() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Revocable, steward(), 100).unwrap();
        let result = index.grant(&id, member_a(), AccessMode::Permanent, steward(), 100);
        assert_eq!(result, Err(GrantError::AlreadyGranted));
    }

    #[test]
    fn test_grant_on_recalled() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.recall(&id, 200);

        let result = index.grant(&id, member_a(), AccessMode::Revocable, steward(), 300);
        assert_eq!(result, Err(GrantError::Recalled));
    }

    #[test]
    fn test_grant_on_transferred() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.transfer(&id, member_a(), steward(), 200).unwrap();

        let result = index.grant(&id, member_b(), AccessMode::Revocable, steward(), 300);
        assert_eq!(result, Err(GrantError::Transferred));
    }

    #[test]
    fn test_revoke_access_removes_revocable() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Revocable, steward(), 100).unwrap();
        assert!(index.revoke_access(&id, &member_a()).is_ok());

        let entry = index.get(&id).unwrap();
        assert!(entry.grants.is_empty());
    }

    #[test]
    fn test_revoke_access_removes_timed() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Timed { expires_at: 500 }, steward(), 100).unwrap();
        assert!(index.revoke_access(&id, &member_a()).is_ok());

        let entry = index.get(&id).unwrap();
        assert!(entry.grants.is_empty());
    }

    #[test]
    fn test_revoke_access_skips_permanent() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Permanent, steward(), 100).unwrap();
        let result = index.revoke_access(&id, &member_a());
        assert_eq!(result, Err(RevokeError::CannotRevoke));

        let entry = index.get(&id).unwrap();
        assert_eq!(entry.grants.len(), 1);
    }

    #[test]
    fn test_recall_removes_revocable_keeps_permanent() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Revocable, steward(), 100).unwrap();
        index.grant(&id, member_b(), AccessMode::Permanent, steward(), 100).unwrap();
        index.grant(&id, member_c(), AccessMode::Timed { expires_at: 500 }, steward(), 100).unwrap();

        assert!(index.recall(&id, 200));

        let entry = index.get(&id).unwrap();
        assert!(!entry.status.is_active());
        assert_eq!(entry.grants.len(), 1);
        assert_eq!(entry.grants[0].grantee, member_b());
        assert!(matches!(entry.grants[0].mode, AccessMode::Permanent));
    }

    #[test]
    fn test_recall_idempotent() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        assert!(index.recall(&id, 200));
        assert!(!index.recall(&id, 300));
    }

    #[test]
    fn test_transfer_returns_entry_with_auto_grant() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        let recipient_entry = index.transfer(&id, member_a(), steward(), 200).unwrap();

        assert_eq!(recipient_entry.id, id);
        assert_eq!(recipient_entry.name, "test.pdf");
        assert!(recipient_entry.status.is_active());

        assert_eq!(recipient_entry.grants.len(), 1);
        assert_eq!(recipient_entry.grants[0].grantee, steward());
        assert!(matches!(recipient_entry.grants[0].mode, AccessMode::Revocable));

        let prov = recipient_entry.provenance.as_ref().unwrap();
        assert_eq!(prov.original_steward, steward());
        assert_eq!(prov.received_from, steward());
        assert!(matches!(prov.received_via, ProvenanceType::Transfer));

        let original = index.get(&id).unwrap();
        assert!(!original.status.is_active());
        assert!(matches!(original.status, ArtifactStatus::Transferred { .. }));
    }

    #[test]
    fn test_transfer_inherits_permanent_grants() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_b(), AccessMode::Permanent, steward(), 100).unwrap();
        index.grant(&id, member_c(), AccessMode::Revocable, steward(), 100).unwrap();

        let recipient_entry = index.transfer(&id, member_a(), steward(), 200).unwrap();

        assert_eq!(recipient_entry.grants.len(), 2);
        assert!(recipient_entry.grants.iter().any(|g| g.grantee == steward() && matches!(g.mode, AccessMode::Revocable)));
        assert!(recipient_entry.grants.iter().any(|g| g.grantee == member_b() && matches!(g.mode, AccessMode::Permanent)));
    }

    #[test]
    fn test_transfer_on_already_transferred() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.transfer(&id, member_a(), steward(), 200).unwrap();
        let result = index.transfer(&id, member_b(), steward(), 300);
        assert_eq!(result, Err(TransferError::NotActive));
    }

    #[test]
    fn test_accessible_by() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        assert!(index.accessible_by(&member_a(), 100).is_empty());

        index.grant(&id, member_a(), AccessMode::Revocable, steward(), 100).unwrap();
        assert_eq!(index.accessible_by(&member_a(), 100).len(), 1);

        assert!(index.accessible_by(&member_b(), 100).is_empty());
    }

    #[test]
    fn test_accessible_by_filters_expired() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Timed { expires_at: 500 }, steward(), 100).unwrap();

        assert_eq!(index.accessible_by(&member_a(), 499).len(), 1);
        assert!(index.accessible_by(&member_a(), 500).is_empty());
    }


    #[test]
    fn test_gc_expired() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Timed { expires_at: 500 }, steward(), 100).unwrap();
        index.grant(&id, member_b(), AccessMode::Permanent, steward(), 100).unwrap();

        assert_eq!(index.get(&id).unwrap().grants.len(), 2);

        index.gc_expired(500);

        assert_eq!(index.get(&id).unwrap().grants.len(), 1);
        assert_eq!(index.get(&id).unwrap().grants[0].grantee, member_b());
    }

    #[test]
    fn test_active_count() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());

        let mut entry2 = test_entry();
        entry2.id = other_id();
        index.store(entry2);

        assert_eq!(index.active_count(), 2);

        index.recall(&test_id(), 200);
        assert_eq!(index.active_count(), 1);
    }

    #[test]
    fn test_hash_hex() {
        let entry = test_entry();
        assert_eq!(entry.hash_hex().len(), 64);
        assert!(entry.hash_hex().chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(entry.short_hash().len(), 8);
    }

    // ============================================================
    // Tree composition tests
    // ============================================================

    #[test]
    fn test_revoke_nonexistent_artifact() {
        let mut index = ArtifactIndex::default();
        let id = test_id();
        let result = index.revoke_access(&id, &member_a());
        assert_eq!(result, Err(RevokeError::NotFound));
    }

    #[test]
    fn test_revoke_nonexistent_grant() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();
        let result = index.revoke_access(&id, &member_a());
        assert_eq!(result, Err(RevokeError::NotFound));
    }

    #[test]
    fn test_grant_not_found() {
        let mut index = ArtifactIndex::default();
        let fake_id = ArtifactId::Blob([0xFFu8; 32]);
        let result = index.grant(&fake_id, member_a(), AccessMode::Revocable, steward(), 100);
        assert_eq!(result, Err(GrantError::NotFound));
    }

    #[test]
    fn test_transfer_not_found() {
        let mut index = ArtifactIndex::default();
        let fake_id = ArtifactId::Blob([0xFFu8; 32]);
        let result = index.transfer(&fake_id, member_a(), steward(), 100);
        assert_eq!(result, Err(TransferError::NotFound));
    }

    #[test]
    fn test_recall_not_found() {
        let mut index = ArtifactIndex::default();
        let fake_id = ArtifactId::Blob([0xFFu8; 32]);
        assert!(!index.recall(&fake_id, 100));
    }

    #[test]
    fn test_accessible_by_excludes_recalled() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Permanent, steward(), 100).unwrap();

        assert_eq!(index.accessible_by(&member_a(), 100).len(), 1);

        index.recall(&id, 200);
        assert!(index.accessible_by(&member_a(), 200).is_empty());
    }

    #[test]
    fn test_multiple_artifacts_independent() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());

        let mut entry2 = test_entry();
        entry2.id = other_id();
        entry2.name = "other.pdf".to_string();
        index.store(entry2);

        index.grant(&test_id(), member_a(), AccessMode::Revocable, steward(), 100).unwrap();

        assert_eq!(index.accessible_by(&member_a(), 100).len(), 1);
        assert_eq!(index.accessible_by(&member_a(), 100)[0].name, "test.pdf");
    }

    #[test]
    fn test_geolocation_haversine() {
        // London to Paris ≈ 343 km
        let london = GeoLocation { lat: 51.5074, lng: -0.1278 };
        let paris = GeoLocation { lat: 48.8566, lng: 2.3522 };
        let d = london.distance_km(&paris);
        assert!((d - 343.0).abs() < 5.0, "London-Paris distance was {d} km");

        // Same point should be 0
        assert_eq!(london.distance_km(&london), 0.0);
    }
}
