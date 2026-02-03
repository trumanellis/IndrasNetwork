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
    AccessGrant, AccessMode, ArtifactProvenance, ArtifactStatus, GrantError, HolonicError,
    ProvenanceType, RevokeError, TransferError,
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
    /// Parent artifact this is a part of (None if top-level).
    #[serde(default)]
    pub parent: Option<ArtifactId>,
    /// Child artifacts that compose this holon (empty if leaf).
    #[serde(default)]
    pub children: Vec<ArtifactId>,
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

        // Build the entry for the recipient (holonic tree transfers atomically)
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
            parent: entry.parent,
            children: entry.children.clone(),
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

    // ============================================================
    // Holonic composition operations
    // ============================================================

    /// Compose existing artifacts under a parent holon.
    ///
    /// Groups the given child artifacts under the specified parent.
    /// All children must exist, be active, and have no existing parent.
    /// The parent must exist and be active.
    pub fn compose(
        &mut self,
        parent_id: &ArtifactId,
        child_ids: &[ArtifactId],
    ) -> Result<(), HolonicError> {
        // Validate parent exists and is active
        let parent = self.artifacts.get(parent_id).ok_or(HolonicError::NotFound)?;
        if !parent.status.is_active() {
            return Err(HolonicError::NotActive);
        }

        // Validate all children exist, are active, and have no parent
        for child_id in child_ids {
            let child = self.artifacts.get(child_id).ok_or(HolonicError::NotFound)?;
            if !child.status.is_active() {
                return Err(HolonicError::NotActive);
            }
            if child.parent.is_some() {
                return Err(HolonicError::AlreadyHasParent);
            }
            // Check that child is not an ancestor of parent (cycle detection)
            if self.is_ancestor_of(child_id, parent_id) {
                return Err(HolonicError::CycleDetected);
            }
            // Check child is not the parent itself
            if child_id == parent_id {
                return Err(HolonicError::CycleDetected);
            }
        }

        // All validations passed — apply mutations
        for child_id in child_ids {
            if let Some(child) = self.artifacts.get_mut(child_id) {
                child.parent = Some(*parent_id);
            }
        }
        if let Some(parent) = self.artifacts.get_mut(parent_id) {
            for child_id in child_ids {
                if !parent.children.contains(child_id) {
                    parent.children.push(*child_id);
                }
            }
        }

        Ok(())
    }

    /// Decompose a holon — detach all children, making them top-level.
    ///
    /// Inherited grants from the parent are materialized as explicit grants
    /// on each detached child.
    pub fn decompose(&mut self, parent_id: &ArtifactId) -> Result<Vec<ArtifactId>, HolonicError> {
        let parent = self.artifacts.get(parent_id).ok_or(HolonicError::NotFound)?;
        if !parent.status.is_active() {
            return Err(HolonicError::NotActive);
        }

        let child_ids = parent.children.clone();
        let parent_grants = parent.grants.clone();

        // Detach each child and materialize inherited grants
        for child_id in &child_ids {
            if let Some(child) = self.artifacts.get_mut(child_id) {
                child.parent = None;
                // Materialize parent grants onto child
                for grant in &parent_grants {
                    if !child.grants.iter().any(|g| g.grantee == grant.grantee) {
                        child.grants.push(grant.clone());
                    }
                }
            }
        }

        // Clear parent's children list
        if let Some(parent) = self.artifacts.get_mut(parent_id) {
            parent.children.clear();
        }

        Ok(child_ids)
    }

    /// Attach a single artifact as a child of an existing holon.
    ///
    /// The child must have no existing parent (single-parent invariant).
    /// Rejects cycles.
    pub fn attach_child(
        &mut self,
        parent_id: &ArtifactId,
        child_id: &ArtifactId,
    ) -> Result<(), HolonicError> {
        if parent_id == child_id {
            return Err(HolonicError::CycleDetected);
        }

        // Validate parent
        let parent = self.artifacts.get(parent_id).ok_or(HolonicError::NotFound)?;
        if !parent.status.is_active() {
            return Err(HolonicError::NotActive);
        }

        // Validate child
        let child = self.artifacts.get(child_id).ok_or(HolonicError::NotFound)?;
        if !child.status.is_active() {
            return Err(HolonicError::NotActive);
        }
        if child.parent.is_some() {
            return Err(HolonicError::AlreadyHasParent);
        }

        // Cycle detection: ensure child is not an ancestor of parent
        if self.is_ancestor_of(child_id, parent_id) {
            return Err(HolonicError::CycleDetected);
        }

        // Apply mutations
        if let Some(child) = self.artifacts.get_mut(child_id) {
            child.parent = Some(*parent_id);
        }
        if let Some(parent) = self.artifacts.get_mut(parent_id) {
            if !parent.children.contains(child_id) {
                parent.children.push(*child_id);
            }
        }

        Ok(())
    }

    /// Detach a child from a holon, making it top-level.
    ///
    /// Inherited grants from the parent are materialized as explicit grants
    /// on the detached child (grant materialization).
    pub fn detach_child(
        &mut self,
        parent_id: &ArtifactId,
        child_id: &ArtifactId,
    ) -> Result<(), HolonicError> {
        let parent = self.artifacts.get(parent_id).ok_or(HolonicError::NotFound)?;
        if !parent.status.is_active() {
            return Err(HolonicError::NotActive);
        }

        if !parent.children.contains(child_id) {
            return Err(HolonicError::NotAChild);
        }

        let parent_grants = parent.grants.clone();

        // Remove child from parent's children list
        if let Some(parent) = self.artifacts.get_mut(parent_id) {
            parent.children.retain(|id| id != child_id);
        }

        // Clear child's parent and materialize inherited grants
        if let Some(child) = self.artifacts.get_mut(child_id) {
            child.parent = None;
            for grant in &parent_grants {
                if !child.grants.iter().any(|g| g.grantee == grant.grantee) {
                    child.grants.push(grant.clone());
                }
            }
        }

        Ok(())
    }

    /// Get immediate children of an artifact.
    pub fn children_of(&self, id: &ArtifactId) -> Vec<&HomeArtifactEntry> {
        match self.artifacts.get(id) {
            Some(entry) => entry
                .children
                .iter()
                .filter_map(|child_id| self.artifacts.get(child_id))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Get the parent holon of an artifact.
    pub fn parent_of(&self, id: &ArtifactId) -> Option<&HomeArtifactEntry> {
        self.artifacts
            .get(id)
            .and_then(|entry| entry.parent.as_ref())
            .and_then(|parent_id| self.artifacts.get(parent_id))
    }

    /// Walk up the holon chain from an artifact to the root.
    ///
    /// Returns ancestors in order from immediate parent to root.
    pub fn ancestors(&self, id: &ArtifactId) -> Vec<&HomeArtifactEntry> {
        let mut result = Vec::new();
        let mut current_id = self
            .artifacts
            .get(id)
            .and_then(|e| e.parent);

        while let Some(pid) = current_id {
            if let Some(entry) = self.artifacts.get(&pid) {
                result.push(entry);
                current_id = entry.parent;
            } else {
                break;
            }
        }

        result
    }

    /// Recursive depth-first traversal of all sub-artifacts.
    pub fn descendants(&self, id: &ArtifactId) -> Vec<&HomeArtifactEntry> {
        let mut result = Vec::new();
        self.collect_descendants(id, &mut result);
        result
    }

    /// Internal recursive helper for descendants.
    fn collect_descendants<'a>(
        &'a self,
        id: &ArtifactId,
        result: &mut Vec<&'a HomeArtifactEntry>,
    ) {
        if let Some(entry) = self.artifacts.get(id) {
            for child_id in &entry.children {
                if let Some(child) = self.artifacts.get(child_id) {
                    result.push(child);
                    self.collect_descendants(child_id, result);
                }
            }
        }
    }

    /// Get the nesting depth of an artifact (0 = top-level).
    pub fn depth(&self, id: &ArtifactId) -> usize {
        self.ancestors(id).len()
    }

    /// True if the artifact has no children.
    pub fn is_leaf(&self, id: &ArtifactId) -> bool {
        self.artifacts
            .get(id)
            .map(|e| e.children.is_empty())
            .unwrap_or(true)
    }

    /// True if the artifact has no parent.
    pub fn is_root(&self, id: &ArtifactId) -> bool {
        self.artifacts
            .get(id)
            .map(|e| e.parent.is_none())
            .unwrap_or(true)
    }

    /// Recursive sum of all descendant sizes (including self).
    pub fn holon_size(&self, id: &ArtifactId) -> u64 {
        let own_size = self
            .artifacts
            .get(id)
            .map(|e| e.size)
            .unwrap_or(0);

        let descendant_size: u64 = self
            .descendants(id)
            .iter()
            .map(|e| e.size)
            .sum();

        own_size + descendant_size
    }

    /// Check if `ancestor_id` is an ancestor of `descendant_id`.
    ///
    /// Used internally for cycle detection.
    fn is_ancestor_of(&self, ancestor_id: &ArtifactId, descendant_id: &ArtifactId) -> bool {
        let mut current_id = self
            .artifacts
            .get(descendant_id)
            .and_then(|e| e.parent);

        while let Some(pid) = current_id {
            if pid == *ancestor_id {
                return true;
            }
            current_id = self
                .artifacts
                .get(&pid)
                .and_then(|e| e.parent);
        }

        false
    }

    /// Check if a member has access to an artifact, including inherited
    /// access from ancestor holons.
    ///
    /// Grant inheritance: granting access to a parent holon implicitly
    /// grants access to all descendants. This method walks up the
    /// ancestor chain checking for grants.
    pub fn has_access_with_inheritance(&self, id: &ArtifactId, member: &MemberId, now: u64) -> bool {
        // Check direct grant
        if let Some(entry) = self.artifacts.get(id) {
            if entry.has_active_grant(member, now) {
                return true;
            }
        }

        // Check ancestor grants (grant inheritance downward)
        for ancestor in self.ancestors(id) {
            if ancestor.status.is_active() && ancestor.has_active_grant(member, now) {
                return true;
            }
        }

        false
    }

    /// Recall an artifact and cascade to all descendants.
    ///
    /// When a parent holon is recalled, all its descendants are also recalled.
    pub fn recall_cascade(&mut self, id: &ArtifactId, recalled_at: u64) -> Vec<ArtifactId> {
        let mut recalled = Vec::new();

        // Collect all descendant IDs first
        let descendant_ids: Vec<ArtifactId> = self
            .descendants(id)
            .iter()
            .map(|e| e.id)
            .collect();

        // Recall the parent
        if self.recall(id, recalled_at) {
            recalled.push(*id);
        }

        // Recall all descendants
        for desc_id in descendant_ids {
            if self.recall(&desc_id, recalled_at) {
                recalled.push(desc_id);
            }
        }

        recalled
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
            parent: None,
            children: Vec::new(),
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

    // ============================================================
    // Holonic tests
    // ============================================================

    fn id_a() -> ArtifactId { [0x0Au8; 32] }
    fn id_b() -> ArtifactId { [0x0Bu8; 32] }
    fn id_c() -> ArtifactId { [0x0Cu8; 32] }
    fn id_d() -> ArtifactId { [0x0Du8; 32] }

    fn make_entry(id: ArtifactId, name: &str, size: u64) -> HomeArtifactEntry {
        HomeArtifactEntry {
            id,
            name: name.to_string(),
            mime_type: None,
            size,
            created_at: 100,
            encrypted_key: None,
            status: ArtifactStatus::Active,
            grants: Vec::new(),
            provenance: None,
            parent: None,
            children: Vec::new(),
        }
    }

    #[test]
    fn test_compose_and_children_of() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));
        index.store(make_entry(id_b(), "child1", 200));
        index.store(make_entry(id_c(), "child2", 300));

        index.compose(&id_a(), &[id_b(), id_c()]).unwrap();

        let children = index.children_of(&id_a());
        assert_eq!(children.len(), 2);

        let parent_entry = index.get(&id_a()).unwrap();
        assert_eq!(parent_entry.children.len(), 2);

        let child_b = index.get(&id_b()).unwrap();
        assert_eq!(child_b.parent, Some(id_a()));

        let child_c = index.get(&id_c()).unwrap();
        assert_eq!(child_c.parent, Some(id_a()));
    }

    #[test]
    fn test_decompose_round_trip() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));
        index.store(make_entry(id_b(), "child1", 200));
        index.store(make_entry(id_c(), "child2", 300));

        // Grant on parent
        index.grant(&id_a(), member_a(), AccessMode::Revocable, owner(), 100).unwrap();

        // Compose
        index.compose(&id_a(), &[id_b(), id_c()]).unwrap();

        // Decompose
        let detached = index.decompose(&id_a()).unwrap();
        assert_eq!(detached.len(), 2);

        // Children are now top-level
        assert!(index.get(&id_b()).unwrap().parent.is_none());
        assert!(index.get(&id_c()).unwrap().parent.is_none());

        // Parent has no children
        assert!(index.get(&id_a()).unwrap().children.is_empty());

        // Grants materialized onto children
        assert!(index.get(&id_b()).unwrap().has_active_grant(&member_a(), 100));
        assert!(index.get(&id_c()).unwrap().has_active_grant(&member_a(), 100));
    }

    #[test]
    fn test_attach_detach_child() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));
        index.store(make_entry(id_b(), "child", 200));

        // Attach
        index.attach_child(&id_a(), &id_b()).unwrap();
        assert_eq!(index.get(&id_b()).unwrap().parent, Some(id_a()));
        assert_eq!(index.children_of(&id_a()).len(), 1);

        // Detach
        index.detach_child(&id_a(), &id_b()).unwrap();
        assert!(index.get(&id_b()).unwrap().parent.is_none());
        assert!(index.children_of(&id_a()).is_empty());
    }

    #[test]
    fn test_cycle_detection_rejects_cycle() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "a", 100));
        index.store(make_entry(id_b(), "b", 100));
        index.store(make_entry(id_c(), "c", 100));

        // A -> B -> C
        index.attach_child(&id_a(), &id_b()).unwrap();
        index.attach_child(&id_b(), &id_c()).unwrap();

        // Trying C -> A would create a cycle
        // First detach C from B so it has no parent
        index.detach_child(&id_b(), &id_c()).unwrap();
        // Now try to attach A under C (C -> A), but A is ancestor...
        // Actually A has no parent, so we need a different test:
        // Let's try to make B the parent of A (B is child of A)
        let result = index.attach_child(&id_b(), &id_a());
        assert_eq!(result, Err(HolonicError::CycleDetected));
    }

    #[test]
    fn test_self_parent_rejected() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "a", 100));

        let result = index.attach_child(&id_a(), &id_a());
        assert_eq!(result, Err(HolonicError::CycleDetected));
    }

    #[test]
    fn test_already_has_parent() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent1", 100));
        index.store(make_entry(id_b(), "parent2", 100));
        index.store(make_entry(id_c(), "child", 100));

        index.attach_child(&id_a(), &id_c()).unwrap();

        // Can't attach to a second parent
        let result = index.attach_child(&id_b(), &id_c());
        assert_eq!(result, Err(HolonicError::AlreadyHasParent));
    }

    #[test]
    fn test_ancestors_walk() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "root", 100));
        index.store(make_entry(id_b(), "middle", 100));
        index.store(make_entry(id_c(), "leaf", 100));

        index.attach_child(&id_a(), &id_b()).unwrap();
        index.attach_child(&id_b(), &id_c()).unwrap();

        let ancestors = index.ancestors(&id_c());
        assert_eq!(ancestors.len(), 2);
        assert_eq!(ancestors[0].id, id_b()); // immediate parent
        assert_eq!(ancestors[1].id, id_a()); // root
    }

    #[test]
    fn test_descendants_traversal() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "root", 100));
        index.store(make_entry(id_b(), "child1", 200));
        index.store(make_entry(id_c(), "child2", 300));
        index.store(make_entry(id_d(), "grandchild", 400));

        index.attach_child(&id_a(), &id_b()).unwrap();
        index.attach_child(&id_a(), &id_c()).unwrap();
        index.attach_child(&id_b(), &id_d()).unwrap();

        let descendants = index.descendants(&id_a());
        assert_eq!(descendants.len(), 3);
        // Should include b, d (under b), and c
        let desc_ids: Vec<ArtifactId> = descendants.iter().map(|e| e.id).collect();
        assert!(desc_ids.contains(&id_b()));
        assert!(desc_ids.contains(&id_c()));
        assert!(desc_ids.contains(&id_d()));
    }

    #[test]
    fn test_depth() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "root", 100));
        index.store(make_entry(id_b(), "child", 100));
        index.store(make_entry(id_c(), "grandchild", 100));

        index.attach_child(&id_a(), &id_b()).unwrap();
        index.attach_child(&id_b(), &id_c()).unwrap();

        assert_eq!(index.depth(&id_a()), 0);
        assert_eq!(index.depth(&id_b()), 1);
        assert_eq!(index.depth(&id_c()), 2);
    }

    #[test]
    fn test_is_leaf_and_is_root() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "root", 100));
        index.store(make_entry(id_b(), "leaf", 100));

        index.attach_child(&id_a(), &id_b()).unwrap();

        assert!(index.is_root(&id_a()));
        assert!(!index.is_leaf(&id_a()));
        assert!(!index.is_root(&id_b()));
        assert!(index.is_leaf(&id_b()));
    }

    #[test]
    fn test_holon_size() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "root", 100));
        index.store(make_entry(id_b(), "child1", 200));
        index.store(make_entry(id_c(), "child2", 300));

        index.compose(&id_a(), &[id_b(), id_c()]).unwrap();

        assert_eq!(index.holon_size(&id_a()), 600); // 100 + 200 + 300
        assert_eq!(index.holon_size(&id_b()), 200); // leaf, own size only
    }

    #[test]
    fn test_grant_inheritance_via_ancestor() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));
        index.store(make_entry(id_b(), "child", 200));

        index.attach_child(&id_a(), &id_b()).unwrap();

        // Grant on parent only
        index.grant(&id_a(), member_a(), AccessMode::Revocable, owner(), 100).unwrap();

        // Child has no direct grant
        assert!(!index.get(&id_b()).unwrap().has_active_grant(&member_a(), 100));

        // But has inherited access
        assert!(index.has_access_with_inheritance(&id_b(), &member_a(), 100));

        // member_b has no access at all
        assert!(!index.has_access_with_inheritance(&id_b(), &member_b(), 100));
    }

    #[test]
    fn test_recall_cascade() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));
        index.store(make_entry(id_b(), "child1", 200));
        index.store(make_entry(id_c(), "child2", 300));

        index.compose(&id_a(), &[id_b(), id_c()]).unwrap();

        let recalled = index.recall_cascade(&id_a(), 500);
        assert_eq!(recalled.len(), 3);

        assert!(!index.get(&id_a()).unwrap().status.is_active());
        assert!(!index.get(&id_b()).unwrap().status.is_active());
        assert!(!index.get(&id_c()).unwrap().status.is_active());
    }

    #[test]
    fn test_detach_materializes_grants() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));
        index.store(make_entry(id_b(), "child", 200));

        index.attach_child(&id_a(), &id_b()).unwrap();
        index.grant(&id_a(), member_a(), AccessMode::Revocable, owner(), 100).unwrap();

        // Before detach, child has no direct grant
        assert!(!index.get(&id_b()).unwrap().has_active_grant(&member_a(), 100));

        // Detach materializes the grant
        index.detach_child(&id_a(), &id_b()).unwrap();
        assert!(index.get(&id_b()).unwrap().has_active_grant(&member_a(), 100));
    }

    #[test]
    fn test_detach_not_a_child() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));
        index.store(make_entry(id_b(), "other", 200));

        let result = index.detach_child(&id_a(), &id_b());
        assert_eq!(result, Err(HolonicError::NotAChild));
    }

    #[test]
    fn test_compose_not_found() {
        let mut index = ArtifactIndex::default();
        index.store(make_entry(id_a(), "parent", 100));

        let fake_id = [0xFFu8; 32];
        let result = index.compose(&id_a(), &[fake_id]);
        assert_eq!(result, Err(HolonicError::NotFound));
    }

    #[test]
    fn test_compose_parent_not_found() {
        let mut index = ArtifactIndex::default();
        let fake_id = [0xFFu8; 32];
        let result = index.compose(&fake_id, &[]);
        assert_eq!(result, Err(HolonicError::NotFound));
    }

    // ============================================================
    // Original tests continue
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
