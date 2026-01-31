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
    AccessGrant, AccessMode, ArtifactProvenance, ArtifactStatus, GrantError, ProvenanceType,
    RevokeError, TransferError,
};
use crate::artifact::ArtifactId;
use crate::artifact_sharing::{EncryptedArtifactKey, RevocationEntry};
use crate::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single artifact entry in the owner's index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HomeArtifactEntry {
    /// Content hash (BLAKE3), used as the unique identifier.
    pub id: ArtifactId,
    /// Human-readable name.
    pub name: String,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// When the artifact was created/uploaded (tick timestamp).
    pub created_at: u64,
    /// Encrypted per-artifact key (for revocable sharing).
    pub encrypted_key: Option<EncryptedArtifactKey>,
    /// Lifecycle status.
    pub status: ArtifactStatus,
    /// Access grants for other members.
    pub grants: Vec<AccessGrant>,
    /// How we received this artifact (None if we created it).
    pub provenance: Option<ArtifactProvenance>,
}

impl HomeArtifactEntry {
    /// Check if a specific member has an active (non-expired) grant.
    pub fn has_active_grant(&self, member: &MemberId, now: u64) -> bool {
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
        self.id.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Get a short hash for display (first 8 hex chars).
    pub fn short_hash(&self) -> String {
        self.hash_hex()[..8].to_string()
    }
}

/// The home-realm artifact index document.
///
/// This is the CRDT document that replaces `ArtifactKeyRegistry`.
/// It stores all artifacts owned by a user, their grants, and
/// revocation history.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArtifactIndex {
    /// All artifacts, keyed by content hash.
    pub artifacts: HashMap<ArtifactId, HomeArtifactEntry>,
    /// Revocation audit trail.
    pub revocations: Vec<RevocationEntry>,
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
    ///
    /// Returns an error if the artifact is not found, not active,
    /// or the grantee already has access.
    pub fn grant(
        &mut self,
        id: &ArtifactId,
        grantee: MemberId,
        mode: AccessMode,
        granted_by: MemberId,
        now: u64,
    ) -> Result<(), GrantError> {
        let entry = self.artifacts.get_mut(id).ok_or(GrantError::NotFound)?;

        match &entry.status {
            ArtifactStatus::Active => {}
            ArtifactStatus::Recalled { .. } => return Err(GrantError::Recalled),
            ArtifactStatus::Transferred { .. } => return Err(GrantError::Transferred),
        }

        // Check for existing active grant
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
    ///
    /// Skips Permanent grants (they cannot be revoked). Returns an
    /// error if the artifact is not found or not active.
    pub fn revoke_access(
        &mut self,
        id: &ArtifactId,
        grantee: &MemberId,
    ) -> Result<(), RevokeError> {
        let entry = self.artifacts.get_mut(id).ok_or(RevokeError::NotFound)?;

        if !entry.status.is_active() {
            return Err(RevokeError::NotActive);
        }

        // Find the grant
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
    ///
    /// Returns `true` if the artifact was recalled, `false` if it was
    /// already recalled or not found.
    pub fn recall(&mut self, id: &ArtifactId, recalled_at: u64) -> bool {
        let entry = match self.artifacts.get_mut(id) {
            Some(e) => e,
            None => return false,
        };

        if !entry.status.is_active() {
            return false;
        }

        // Remove revocable and timed grants, keep permanent
        entry.grants.retain(|g| matches!(g.mode, AccessMode::Permanent));

        // Update status
        entry.status = ArtifactStatus::Recalled { recalled_at };

        true
    }

    /// Transfer ownership of an artifact to another member.
    ///
    /// Returns a clone of the entry for the recipient to add to their
    /// own index. The sender automatically gets a revocable access grant
    /// in the returned entry.
    pub fn transfer(
        &mut self,
        id: &ArtifactId,
        to: MemberId,
        owner: MemberId,
        now: u64,
    ) -> Result<HomeArtifactEntry, TransferError> {
        let entry = self.artifacts.get_mut(id).ok_or(TransferError::NotFound)?;

        if !entry.status.is_active() {
            return Err(TransferError::NotActive);
        }

        // Build the entry for the recipient
        let mut recipient_entry = HomeArtifactEntry {
            id: entry.id,
            name: entry.name.clone(),
            mime_type: entry.mime_type.clone(),
            size: entry.size,
            created_at: now,
            encrypted_key: entry.encrypted_key.clone(),
            status: ArtifactStatus::Active,
            grants: vec![
                // Auto-grant revocable access back to the sender
                AccessGrant {
                    grantee: owner,
                    mode: AccessMode::Revocable,
                    granted_at: now,
                    granted_by: to,
                },
            ],
            provenance: Some(ArtifactProvenance {
                original_owner: owner,
                received_from: owner,
                received_at: now,
                received_via: ProvenanceType::Transfer,
            }),
        };

        // Carry over any inherited permanent grants
        for grant in &entry.grants {
            if matches!(grant.mode, AccessMode::Permanent) && grant.grantee != to {
                recipient_entry.grants.push(grant.clone());
            }
        }

        // Mark the original as transferred
        entry.status = ArtifactStatus::Transferred {
            to,
            transferred_at: now,
        };

        Ok(recipient_entry)
    }

    /// Get all artifacts accessible by a specific member at a given time.
    ///
    /// Returns entries where the member has an active, non-expired grant.
    pub fn accessible_by(&self, member: &MemberId, now: u64) -> Vec<&HomeArtifactEntry> {
        self.artifacts
            .values()
            .filter(|entry| {
                entry.status.is_active() && entry.has_active_grant(member, now)
            })
            .collect()
    }

    /// Get artifacts accessible by ALL given members (realm view query).
    ///
    /// Returns entries where every member in the list has an active grant.
    /// This is used to compute what artifacts are visible in a realm context.
    pub fn accessible_by_all(
        &self,
        members: &[MemberId],
        now: u64,
    ) -> Vec<&HomeArtifactEntry> {
        if members.is_empty() {
            return Vec::new();
        }

        self.artifacts
            .values()
            .filter(|entry| {
                entry.status.is_active()
                    && members.iter().all(|m| entry.has_active_grant(m, now))
            })
            .collect()
    }

    /// Remove expired timed grants.
    ///
    /// Call periodically to clean up stale grants.
    pub fn gc_expired(&mut self, now: u64) {
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
        [0x42u8; 32]
    }

    fn other_id() -> ArtifactId {
        [0x43u8; 32]
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

    fn owner() -> MemberId {
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
        assert!(!index.store(test_entry())); // duplicate
    }

    #[test]
    fn test_grant_each_mode() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        // Revocable
        assert!(index.grant(&id, member_a(), AccessMode::Revocable, owner(), 100).is_ok());

        // Permanent
        assert!(index.grant(&id, member_b(), AccessMode::Permanent, owner(), 100).is_ok());

        // Timed
        assert!(index.grant(&id, member_c(), AccessMode::Timed { expires_at: 500 }, owner(), 100).is_ok());

        let entry = index.get(&id).unwrap();
        assert_eq!(entry.grants.len(), 3);
    }

    #[test]
    fn test_grant_already_granted() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Revocable, owner(), 100).unwrap();
        let result = index.grant(&id, member_a(), AccessMode::Permanent, owner(), 100);
        assert_eq!(result, Err(GrantError::AlreadyGranted));
    }

    #[test]
    fn test_grant_on_recalled() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.recall(&id, 200);

        let result = index.grant(&id, member_a(), AccessMode::Revocable, owner(), 300);
        assert_eq!(result, Err(GrantError::Recalled));
    }

    #[test]
    fn test_grant_on_transferred() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.transfer(&id, member_a(), owner(), 200).unwrap();

        let result = index.grant(&id, member_b(), AccessMode::Revocable, owner(), 300);
        assert_eq!(result, Err(GrantError::Transferred));
    }

    #[test]
    fn test_revoke_access_removes_revocable() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Revocable, owner(), 100).unwrap();
        assert!(index.revoke_access(&id, &member_a()).is_ok());

        let entry = index.get(&id).unwrap();
        assert!(entry.grants.is_empty());
    }

    #[test]
    fn test_revoke_access_removes_timed() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Timed { expires_at: 500 }, owner(), 100).unwrap();
        assert!(index.revoke_access(&id, &member_a()).is_ok());

        let entry = index.get(&id).unwrap();
        assert!(entry.grants.is_empty());
    }

    #[test]
    fn test_revoke_access_skips_permanent() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Permanent, owner(), 100).unwrap();
        let result = index.revoke_access(&id, &member_a());
        assert_eq!(result, Err(RevokeError::CannotRevoke));

        // Grant still exists
        let entry = index.get(&id).unwrap();
        assert_eq!(entry.grants.len(), 1);
    }

    #[test]
    fn test_recall_removes_revocable_keeps_permanent() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Revocable, owner(), 100).unwrap();
        index.grant(&id, member_b(), AccessMode::Permanent, owner(), 100).unwrap();
        index.grant(&id, member_c(), AccessMode::Timed { expires_at: 500 }, owner(), 100).unwrap();

        assert!(index.recall(&id, 200));

        let entry = index.get(&id).unwrap();
        assert!(!entry.status.is_active());
        // Only permanent grant survives
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
        assert!(!index.recall(&id, 300)); // already recalled
    }

    #[test]
    fn test_transfer_returns_entry_with_auto_grant() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        let recipient_entry = index.transfer(&id, member_a(), owner(), 200).unwrap();

        // Check recipient entry
        assert_eq!(recipient_entry.id, id);
        assert_eq!(recipient_entry.name, "test.pdf");
        assert!(recipient_entry.status.is_active());

        // Sender gets revocable access back
        assert_eq!(recipient_entry.grants.len(), 1);
        assert_eq!(recipient_entry.grants[0].grantee, owner());
        assert!(matches!(recipient_entry.grants[0].mode, AccessMode::Revocable));

        // Provenance records transfer
        let prov = recipient_entry.provenance.as_ref().unwrap();
        assert_eq!(prov.original_owner, owner());
        assert_eq!(prov.received_from, owner());
        assert!(matches!(prov.received_via, ProvenanceType::Transfer));

        // Original is now transferred
        let original = index.get(&id).unwrap();
        assert!(!original.status.is_active());
        assert!(matches!(original.status, ArtifactStatus::Transferred { .. }));
    }

    #[test]
    fn test_transfer_inherits_permanent_grants() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        // Give permanent access to member_b
        index.grant(&id, member_b(), AccessMode::Permanent, owner(), 100).unwrap();
        // Give revocable access to member_c (should NOT be inherited)
        index.grant(&id, member_c(), AccessMode::Revocable, owner(), 100).unwrap();

        let recipient_entry = index.transfer(&id, member_a(), owner(), 200).unwrap();

        // Should have: auto-revocable for owner + inherited permanent for member_b
        assert_eq!(recipient_entry.grants.len(), 2);
        assert!(recipient_entry.grants.iter().any(|g| g.grantee == owner() && matches!(g.mode, AccessMode::Revocable)));
        assert!(recipient_entry.grants.iter().any(|g| g.grantee == member_b() && matches!(g.mode, AccessMode::Permanent)));
    }

    #[test]
    fn test_transfer_on_already_transferred() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.transfer(&id, member_a(), owner(), 200).unwrap();
        let result = index.transfer(&id, member_b(), owner(), 300);
        assert_eq!(result, Err(TransferError::NotActive));
    }

    #[test]
    fn test_accessible_by() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        // No grants yet
        assert!(index.accessible_by(&member_a(), 100).is_empty());

        // Add grant
        index.grant(&id, member_a(), AccessMode::Revocable, owner(), 100).unwrap();
        assert_eq!(index.accessible_by(&member_a(), 100).len(), 1);

        // member_b has no access
        assert!(index.accessible_by(&member_b(), 100).is_empty());
    }

    #[test]
    fn test_accessible_by_filters_expired() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Timed { expires_at: 500 }, owner(), 100).unwrap();

        // Before expiry
        assert_eq!(index.accessible_by(&member_a(), 499).len(), 1);

        // After expiry
        assert!(index.accessible_by(&member_a(), 500).is_empty());
    }

    #[test]
    fn test_accessible_by_all_realm_view() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        // Grant both members
        index.grant(&id, member_a(), AccessMode::Revocable, owner(), 100).unwrap();
        index.grant(&id, member_b(), AccessMode::Permanent, owner(), 100).unwrap();

        // Both have access
        let members = vec![member_a(), member_b()];
        assert_eq!(index.accessible_by_all(&members, 100).len(), 1);

        // Only one has access — realm view should be empty
        let members_with_c = vec![member_a(), member_c()];
        assert!(index.accessible_by_all(&members_with_c, 100).is_empty());
    }

    #[test]
    fn test_accessible_by_all_empty_members() {
        let index = ArtifactIndex::default();
        assert!(index.accessible_by_all(&[], 100).is_empty());
    }

    #[test]
    fn test_gc_expired() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Timed { expires_at: 500 }, owner(), 100).unwrap();
        index.grant(&id, member_b(), AccessMode::Permanent, owner(), 100).unwrap();

        // Before expiry
        assert_eq!(index.get(&id).unwrap().grants.len(), 2);

        // GC at tick 500
        index.gc_expired(500);

        // Timed grant removed, permanent kept
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
        // member_a has no grant
        let result = index.revoke_access(&id, &member_a());
        assert_eq!(result, Err(RevokeError::NotFound));
    }

    #[test]
    fn test_grant_not_found() {
        let mut index = ArtifactIndex::default();
        let fake_id = [0xFFu8; 32];
        let result = index.grant(&fake_id, member_a(), AccessMode::Revocable, owner(), 100);
        assert_eq!(result, Err(GrantError::NotFound));
    }

    #[test]
    fn test_transfer_not_found() {
        let mut index = ArtifactIndex::default();
        let fake_id = [0xFFu8; 32];
        let result = index.transfer(&fake_id, member_a(), owner(), 100);
        assert_eq!(result, Err(TransferError::NotFound));
    }

    #[test]
    fn test_recall_not_found() {
        let mut index = ArtifactIndex::default();
        let fake_id = [0xFFu8; 32];
        assert!(!index.recall(&fake_id, 100));
    }

    #[test]
    fn test_accessible_by_excludes_recalled() {
        let mut index = ArtifactIndex::default();
        index.store(test_entry());
        let id = test_id();

        index.grant(&id, member_a(), AccessMode::Permanent, owner(), 100).unwrap();

        // Before recall
        assert_eq!(index.accessible_by(&member_a(), 100).len(), 1);

        // After recall - permanent grant survives but status is Recalled
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

        // Grant on first artifact only
        index.grant(&test_id(), member_a(), AccessMode::Revocable, owner(), 100).unwrap();

        assert_eq!(index.accessible_by(&member_a(), 100).len(), 1);
        assert_eq!(index.accessible_by(&member_a(), 100)[0].name, "test.pdf");
    }
}
