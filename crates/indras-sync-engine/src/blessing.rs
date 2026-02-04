//! Blessing system for quest proof validation.
//!
//! Members who contributed attention to a quest can "bless" a proof submission
//! by releasing their accumulated attention as validation. The blessing system
//! tracks which attention event indices have been used for blessings.
//!
//! # Key Concepts
//!
//! - **Blessing**: Releasing accumulated attention time to validate a quest proof
//! - **Event Indices**: References into AttentionDocument.events that are being blessed
//! - **Proof Score**: Sum of blessed attention time (calculated from event durations)
//!
//! # CRDT Semantics
//!
//! - Blessings are append-only and immutable
//! - Merge strategy: union + dedupe by blessing_id
//! - Each attention event can only be blessed once per quest claim

use indras_network::member::MemberId;
use crate::quest::QuestId;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Unique identifier for a blessing (16 bytes).
pub type BlessingId = [u8; 16];

/// Generate a new unique blessing ID.
pub fn generate_blessing_id() -> BlessingId {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut id = [0u8; 16];

    // Use timestamp for first 8 bytes
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    id[..8].copy_from_slice(&timestamp.to_le_bytes());

    // Use counter for remaining bytes (uniqueness within same nanosecond)
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    id[8..].copy_from_slice(&counter.to_le_bytes());

    id
}

/// Identifier for a quest claim (quest_id + claimant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClaimId {
    /// The quest this claim belongs to.
    pub quest_id: QuestId,
    /// The member who submitted the claim.
    pub claimant: MemberId,
}

impl ClaimId {
    /// Create a new claim ID.
    pub fn new(quest_id: QuestId, claimant: MemberId) -> Self {
        Self { quest_id, claimant }
    }
}

/// A blessing - validation of a quest proof by releasing accumulated attention.
///
/// Blessings reference specific attention event indices from the AttentionDocument.
/// The duration of the blessing is calculated by looking up these events and
/// computing the time spans they represent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Blessing {
    /// Unique identifier for this blessing.
    pub blessing_id: BlessingId,
    /// The claim being blessed (quest_id + claimant).
    pub claim_id: ClaimId,
    /// The member giving the blessing.
    pub blesser: MemberId,
    /// Indices into AttentionDocument.events that are being released.
    /// These are the specific attention "packets" being blessed.
    pub event_indices: Vec<usize>,
    /// When the blessing was created (Unix timestamp in milliseconds).
    pub timestamp_millis: i64,
}

impl Blessing {
    /// Create a new blessing.
    pub fn new(
        claim_id: ClaimId,
        blesser: MemberId,
        event_indices: Vec<usize>,
    ) -> Self {
        Self {
            blessing_id: generate_blessing_id(),
            claim_id,
            blesser,
            event_indices,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Get the quest ID being blessed.
    pub fn quest_id(&self) -> QuestId {
        self.claim_id.quest_id
    }

    /// Get the claimant being blessed.
    pub fn claimant(&self) -> MemberId {
        self.claim_id.claimant
    }
}

impl PartialOrd for Blessing {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Blessing {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Sort by timestamp first, then by blessing_id for determinism
        self.timestamp_millis
            .cmp(&other.timestamp_millis)
            .then_with(|| self.blessing_id.cmp(&other.blessing_id))
    }
}

/// CRDT document for blessing tracking.
///
/// Maintains an append-only log of blessings with derived state for quick lookups.
/// The derived state is recomputable from blessings and must be rebuilt after merges.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BlessingDocument {
    /// Append-only log of blessings.
    blessings: Vec<Blessing>,
    /// Derived state: blessed event indices per (blesser, quest_id).
    /// Key = (blesser, quest_id), Value = set of blessed event indices.
    #[serde(default)]
    blessed_indices: HashMap<(MemberId, QuestId), HashSet<usize>>,
}

impl BlessingDocument {
    /// Create a new empty blessing document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a blessing for a claim.
    ///
    /// # Arguments
    ///
    /// * `claim_id` - The claim being blessed
    /// * `blesser` - The member giving the blessing
    /// * `event_indices` - Indices into AttentionDocument.events to bless
    ///
    /// # Returns
    ///
    /// The blessing ID if successful, or an error if validation fails.
    pub fn bless_claim(
        &mut self,
        claim_id: ClaimId,
        blesser: MemberId,
        event_indices: Vec<usize>,
    ) -> Result<BlessingId, BlessingError> {
        // Check for double-blessing (same attention events used twice)
        let key = (blesser, claim_id.quest_id);
        let already_blessed = self.blessed_indices.get(&key).cloned().unwrap_or_default();

        for &idx in &event_indices {
            if already_blessed.contains(&idx) {
                return Err(BlessingError::EventAlreadyBlessed {
                    event_index: idx,
                    quest_id: claim_id.quest_id,
                });
            }
        }

        // Create the blessing
        let blessing = Blessing::new(claim_id, blesser, event_indices.clone());
        let blessing_id = blessing.blessing_id;

        // Update derived state
        let entry = self.blessed_indices.entry(key).or_default();
        for idx in &event_indices {
            entry.insert(*idx);
        }

        // Append blessing
        self.blessings.push(blessing);

        Ok(blessing_id)
    }

    /// Get all blessings for a specific claim.
    pub fn blessings_for_claim(&self, claim_id: &ClaimId) -> Vec<&Blessing> {
        self.blessings
            .iter()
            .filter(|b| &b.claim_id == claim_id)
            .collect()
    }

    /// Get all blessings for a quest (all claimants).
    pub fn blessings_for_quest(&self, quest_id: &QuestId) -> Vec<&Blessing> {
        self.blessings
            .iter()
            .filter(|b| &b.claim_id.quest_id == quest_id)
            .collect()
    }

    /// Get all blessings given by a specific member.
    pub fn blessings_by_member(&self, member: &MemberId) -> Vec<&Blessing> {
        self.blessings
            .iter()
            .filter(|b| &b.blesser == member)
            .collect()
    }

    /// Get the set of blessed event indices for a (blesser, quest_id) pair.
    pub fn blessed_event_indices(&self, blesser: &MemberId, quest_id: &QuestId) -> HashSet<usize> {
        self.blessed_indices
            .get(&(*blesser, *quest_id))
            .cloned()
            .unwrap_or_default()
    }

    /// Get unblessed event indices from a set of candidate indices.
    ///
    /// This is useful for determining which attention events are still
    /// available for a member to bless on a specific quest.
    pub fn unblessed_event_indices(
        &self,
        blesser: &MemberId,
        quest_id: &QuestId,
        candidate_indices: &[usize],
    ) -> Vec<usize> {
        let blessed = self.blessed_event_indices(blesser, quest_id);
        candidate_indices
            .iter()
            .filter(|idx| !blessed.contains(idx))
            .copied()
            .collect()
    }

    /// Check if a specific event index has been blessed.
    pub fn is_event_blessed(&self, blesser: &MemberId, quest_id: &QuestId, event_index: usize) -> bool {
        self.blessed_indices
            .get(&(*blesser, *quest_id))
            .map(|set| set.contains(&event_index))
            .unwrap_or(false)
    }

    /// Get the total number of blessings.
    pub fn blessing_count(&self) -> usize {
        self.blessings.len()
    }

    /// Get all blessings (for inspection/debugging).
    pub fn blessings(&self) -> &[Blessing] {
        &self.blessings
    }

    /// Rebuild derived state from the blessing log.
    ///
    /// This must be called after CRDT merges to ensure consistency.
    pub fn rebuild_derived_state(&mut self) {
        self.blessed_indices.clear();

        // Sort blessings by timestamp for deterministic processing
        self.blessings.sort();

        // Replay blessings to rebuild blessed_indices
        for blessing in &self.blessings {
            let key = (blessing.blesser, blessing.claim_id.quest_id);
            let entry = self.blessed_indices.entry(key).or_default();
            for idx in &blessing.event_indices {
                entry.insert(*idx);
            }
        }
    }

    /// Merge another document into this one (CRDT merge).
    ///
    /// Uses union of blessings + dedupe by blessing_id.
    /// Automatically rebuilds derived state after merge.
    pub fn merge(&mut self, other: &BlessingDocument) {
        // Collect all blessings
        let mut all_blessings: Vec<Blessing> = self.blessings.clone();

        // Add blessings from other that we don't have (dedupe by blessing_id)
        for blessing in &other.blessings {
            if !all_blessings.iter().any(|b| b.blessing_id == blessing.blessing_id) {
                all_blessings.push(blessing.clone());
            }
        }

        // Sort by timestamp then blessing_id for deterministic ordering
        all_blessings.sort();

        self.blessings = all_blessings;
        self.rebuild_derived_state();
    }

    /// Get total blessed event count for a claim.
    ///
    /// This counts the total number of attention events that have been
    /// blessed for this claim across all blessers.
    pub fn total_blessed_events(&self, claim_id: &ClaimId) -> usize {
        self.blessings_for_claim(claim_id)
            .iter()
            .map(|b| b.event_indices.len())
            .sum()
    }

    /// Get unique blessers for a claim.
    pub fn blessers_for_claim(&self, claim_id: &ClaimId) -> Vec<MemberId> {
        let mut blessers: Vec<MemberId> = self
            .blessings_for_claim(claim_id)
            .iter()
            .map(|b| b.blesser)
            .collect();
        blessers.sort();
        blessers.dedup();
        blessers
    }
}

/// Errors that can occur during blessing operations.
#[derive(Debug, Clone, PartialEq)]
pub enum BlessingError {
    /// The attention event has already been blessed for this quest.
    EventAlreadyBlessed {
        event_index: usize,
        quest_id: QuestId,
    },
    /// The member has no attention events available to bless.
    NoAttentionAvailable,
    /// The quest claim was not found.
    ClaimNotFound,
    /// The member doesn't own the specified attention events.
    NotOwnerOfEvents,
}

impl std::fmt::Display for BlessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlessingError::EventAlreadyBlessed { event_index, .. } => {
                write!(f, "Attention event {} has already been blessed for this quest", event_index)
            }
            BlessingError::NoAttentionAvailable => {
                write!(f, "No attention events available to bless")
            }
            BlessingError::ClaimNotFound => write!(f, "Quest claim not found"),
            BlessingError::NotOwnerOfEvents => {
                write!(f, "Member does not own the specified attention events")
            }
        }
    }
}

impl std::error::Error for BlessingError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member_id(n: u8) -> MemberId {
        [n; 32]
    }

    fn test_quest_id(n: u8) -> QuestId {
        [n; 16]
    }

    #[test]
    fn test_blessing_id_generation() {
        let id1 = generate_blessing_id();
        let id2 = generate_blessing_id();
        assert_ne!(id1, id2, "Blessing IDs should be unique");
    }

    #[test]
    fn test_blessing_creation() {
        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser = test_member_id(2);
        let claim_id = ClaimId::new(quest_id, claimant);

        let blessing = Blessing::new(claim_id, blesser, vec![0, 1, 2]);

        assert_eq!(blessing.quest_id(), quest_id);
        assert_eq!(blessing.claimant(), claimant);
        assert_eq!(blessing.blesser, blesser);
        assert_eq!(blessing.event_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_bless_claim() {
        let mut doc = BlessingDocument::new();
        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser = test_member_id(2);
        let claim_id = ClaimId::new(quest_id, claimant);

        // First blessing should succeed
        let result = doc.bless_claim(claim_id, blesser, vec![0, 1, 2]);
        assert!(result.is_ok());
        assert_eq!(doc.blessing_count(), 1);

        // Same events should fail (already blessed)
        let result = doc.bless_claim(claim_id, blesser, vec![1]);
        assert!(matches!(result, Err(BlessingError::EventAlreadyBlessed { .. })));

        // Different events should succeed
        let result = doc.bless_claim(claim_id, blesser, vec![3, 4]);
        assert!(result.is_ok());
        assert_eq!(doc.blessing_count(), 2);
    }

    #[test]
    fn test_blessings_for_claim() {
        let mut doc = BlessingDocument::new();
        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser1 = test_member_id(2);
        let blesser2 = test_member_id(3);
        let claim_id = ClaimId::new(quest_id, claimant);

        doc.bless_claim(claim_id, blesser1, vec![0, 1]).unwrap();
        doc.bless_claim(claim_id, blesser2, vec![2, 3]).unwrap();

        let blessings = doc.blessings_for_claim(&claim_id);
        assert_eq!(blessings.len(), 2);

        let blessers = doc.blessers_for_claim(&claim_id);
        assert_eq!(blessers.len(), 2);
    }

    #[test]
    fn test_blessed_event_indices() {
        let mut doc = BlessingDocument::new();
        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser = test_member_id(2);
        let claim_id = ClaimId::new(quest_id, claimant);

        doc.bless_claim(claim_id, blesser, vec![0, 2, 4]).unwrap();

        let blessed = doc.blessed_event_indices(&blesser, &quest_id);
        assert!(blessed.contains(&0));
        assert!(!blessed.contains(&1));
        assert!(blessed.contains(&2));
        assert!(!blessed.contains(&3));
        assert!(blessed.contains(&4));
    }

    #[test]
    fn test_unblessed_event_indices() {
        let mut doc = BlessingDocument::new();
        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser = test_member_id(2);
        let claim_id = ClaimId::new(quest_id, claimant);

        doc.bless_claim(claim_id, blesser, vec![0, 2]).unwrap();

        let unblessed = doc.unblessed_event_indices(&blesser, &quest_id, &[0, 1, 2, 3, 4]);
        assert_eq!(unblessed, vec![1, 3, 4]);
    }

    #[test]
    fn test_document_merge() {
        let mut doc1 = BlessingDocument::new();
        let mut doc2 = BlessingDocument::new();

        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser1 = test_member_id(2);
        let blesser2 = test_member_id(3);
        let claim_id = ClaimId::new(quest_id, claimant);

        doc1.bless_claim(claim_id, blesser1, vec![0, 1]).unwrap();
        doc2.bless_claim(claim_id, blesser2, vec![2, 3]).unwrap();

        // Merge doc2 into doc1
        doc1.merge(&doc2);

        // Should have both blessings
        assert_eq!(doc1.blessing_count(), 2);
        assert_eq!(doc1.blessings_for_claim(&claim_id).len(), 2);
    }

    #[test]
    fn test_merge_deduplication() {
        let mut doc1 = BlessingDocument::new();
        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser = test_member_id(2);
        let claim_id = ClaimId::new(quest_id, claimant);

        doc1.bless_claim(claim_id, blesser, vec![0, 1]).unwrap();

        // Clone doc1 to doc2
        let doc2 = doc1.clone();

        // Merge should not duplicate
        doc1.merge(&doc2);
        assert_eq!(doc1.blessing_count(), 1);
    }

    #[test]
    fn test_self_blessing_allowed() {
        let mut doc = BlessingDocument::new();
        let quest_id = test_quest_id(1);
        let member = test_member_id(1);
        let claim_id = ClaimId::new(quest_id, member);

        // Member can bless their own claim
        let result = doc.bless_claim(claim_id, member, vec![0, 1]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_different_quests_independent() {
        let mut doc = BlessingDocument::new();
        let quest1 = test_quest_id(1);
        let quest2 = test_quest_id(2);
        let claimant = test_member_id(1);
        let blesser = test_member_id(2);
        let claim1 = ClaimId::new(quest1, claimant);
        let claim2 = ClaimId::new(quest2, claimant);

        // Same event indices can be blessed for different quests
        doc.bless_claim(claim1, blesser, vec![0, 1]).unwrap();
        let result = doc.bless_claim(claim2, blesser, vec![0, 1]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_total_blessed_events() {
        let mut doc = BlessingDocument::new();
        let quest_id = test_quest_id(1);
        let claimant = test_member_id(1);
        let blesser1 = test_member_id(2);
        let blesser2 = test_member_id(3);
        let claim_id = ClaimId::new(quest_id, claimant);

        doc.bless_claim(claim_id, blesser1, vec![0, 1, 2]).unwrap();
        doc.bless_claim(claim_id, blesser2, vec![3, 4]).unwrap();

        assert_eq!(doc.total_blessed_events(&claim_id), 5);
    }
}
