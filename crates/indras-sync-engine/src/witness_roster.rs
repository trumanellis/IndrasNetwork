//! Witness roster document for tracking eligible witnesses per intention scope.
//!
//! Each intention scope (artifact) has a set of eligible witness MemberIds.
//! The document uses union merge semantics — merging two rosters produces
//! the union of witness sets per intention scope.
//!
//! # CRDT Semantics
//!
//! - Merge strategy: per-intention union of witness sets
//! - Rosters can be set, retrieved, and removed per intention scope

use indras_artifacts::artifact::ArtifactId;
use indras_network::document::DocumentSchema;
use indras_network::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// CRDT document tracking witness rosters per intention scope.
///
/// Each intention scope maps to a set of eligible witness MemberIds.
/// Merges use per-scope union of witness sets.
///
/// Uses `BTreeMap`/`BTreeSet` for deterministic serialization order,
/// preventing CRDT sync amplification from non-deterministic iteration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WitnessRosterDocument {
    /// intention_scope -> set of eligible witness MemberIds.
    rosters: BTreeMap<ArtifactId, BTreeSet<MemberId>>,
}

impl WitnessRosterDocument {
    /// Create a new empty roster document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the witness roster for an intention scope.
    ///
    /// Replaces any existing roster for this scope.
    pub fn set_roster(&mut self, intention_scope: ArtifactId, witnesses: Vec<MemberId>) {
        self.rosters
            .insert(intention_scope, witnesses.into_iter().collect());
    }

    /// Get the witness roster for an intention scope.
    pub fn get_roster(&self, intention_scope: &ArtifactId) -> Vec<MemberId> {
        self.rosters
            .get(intention_scope)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Remove the witness roster for an intention scope.
    pub fn remove_roster(&mut self, intention_scope: &ArtifactId) {
        self.rosters.remove(intention_scope);
    }

    /// Get all rosters (intention_scope -> witnesses).
    pub fn all_rosters(&self) -> &BTreeMap<ArtifactId, BTreeSet<MemberId>> {
        &self.rosters
    }

    /// Check if a member is a witness for a given intention scope.
    pub fn is_witness(&self, intention_scope: &ArtifactId, member: &MemberId) -> bool {
        self.rosters
            .get(intention_scope)
            .map_or(false, |set| set.contains(member))
    }
}

impl DocumentSchema for WitnessRosterDocument {
    /// Merge: per-intention union of witness sets.
    fn merge(&mut self, remote: Self) {
        for (scope, remote_witnesses) in remote.rosters {
            let local = self.rosters.entry(scope).or_default();
            local.extend(remote_witnesses);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member(n: u8) -> MemberId {
        [n; 32]
    }

    fn test_scope(n: u8) -> ArtifactId {
        ArtifactId::Doc([n; 32])
    }

    #[test]
    fn test_set_and_get_roster() {
        let mut doc = WitnessRosterDocument::new();
        let scope = test_scope(1);
        let witnesses = vec![test_member(1), test_member(2), test_member(3)];

        doc.set_roster(scope, witnesses.clone());
        let got = doc.get_roster(&scope);
        assert_eq!(got.len(), 3);
        for w in &witnesses {
            assert!(got.contains(w));
        }
    }

    #[test]
    fn test_get_roster_missing_scope() {
        let doc = WitnessRosterDocument::new();
        let got = doc.get_roster(&test_scope(99));
        assert!(got.is_empty());
    }

    #[test]
    fn test_remove_roster() {
        let mut doc = WitnessRosterDocument::new();
        let scope = test_scope(1);
        doc.set_roster(scope, vec![test_member(1)]);
        assert!(!doc.get_roster(&scope).is_empty());

        doc.remove_roster(&scope);
        assert!(doc.get_roster(&scope).is_empty());
    }

    #[test]
    fn test_is_witness() {
        let mut doc = WitnessRosterDocument::new();
        let scope = test_scope(1);
        doc.set_roster(scope, vec![test_member(1), test_member(2)]);

        assert!(doc.is_witness(&scope, &test_member(1)));
        assert!(doc.is_witness(&scope, &test_member(2)));
        assert!(!doc.is_witness(&scope, &test_member(3)));
    }

    #[test]
    fn test_merge_union() {
        let mut doc1 = WitnessRosterDocument::new();
        let mut doc2 = WitnessRosterDocument::new();
        let scope = test_scope(1);

        doc1.set_roster(scope, vec![test_member(1), test_member(2)]);
        doc2.set_roster(scope, vec![test_member(2), test_member(3)]);

        doc1.merge(doc2);
        let merged = doc1.get_roster(&scope);
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&test_member(1)));
        assert!(merged.contains(&test_member(2)));
        assert!(merged.contains(&test_member(3)));
    }

    #[test]
    fn test_merge_different_scopes() {
        let mut doc1 = WitnessRosterDocument::new();
        let mut doc2 = WitnessRosterDocument::new();

        doc1.set_roster(test_scope(1), vec![test_member(1)]);
        doc2.set_roster(test_scope(2), vec![test_member(2)]);

        doc1.merge(doc2);
        assert_eq!(doc1.get_roster(&test_scope(1)).len(), 1);
        assert_eq!(doc1.get_roster(&test_scope(2)).len(), 1);
        assert_eq!(doc1.all_rosters().len(), 2);
    }

    #[test]
    fn test_set_roster_deduplicates() {
        let mut doc = WitnessRosterDocument::new();
        let scope = test_scope(1);
        doc.set_roster(scope, vec![test_member(1), test_member(1), test_member(1)]);
        assert_eq!(doc.get_roster(&scope).len(), 1);
    }
}
