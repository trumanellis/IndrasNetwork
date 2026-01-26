//! Quest - lightweight collaboration intentions within realms.
//!
//! Quests support a Proof of Service model where multiple members can submit
//! claims (proofs of work) for a quest, and the quest creator verifies them.
//! This enables collaborative work with accountability:
//!
//! 1. Creator posts a quest
//! 2. Multiple members submit claims with proof artifacts
//! 3. Creator verifies valid claims
//! 4. Creator marks quest complete
//!
//! Quests are CRDT-synchronized across all realm members.

use crate::artifact::ArtifactId;
use crate::member::MemberId;
use crate::proof_folder::ProofFolderId;

use serde::{Deserialize, Serialize};

/// Unique identifier for a quest (16 bytes).
pub type QuestId = [u8; 16];

/// Generate a new random quest ID.
pub fn generate_quest_id() -> QuestId {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut id = [0u8; 16];

    // Use timestamp for first 8 bytes (uniqueness over time)
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    id[..8].copy_from_slice(&timestamp.to_le_bytes());

    // Use blake3 hash of timestamp + counter for remaining bytes (uniqueness within same nanosecond)
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let hash = blake3::hash(&[&timestamp.to_le_bytes()[..], &counter.to_le_bytes()[..]].concat());
    id[8..].copy_from_slice(&hash.as_bytes()[..8]);

    id
}

/// A claim/proof of service for a quest.
///
/// Members submit claims to demonstrate they've completed work for a quest.
/// Each claim can include an optional proof artifact (document, image, etc.)
/// or a proof folder with multiple artifacts and a narrative.
///
/// # Proof Types
///
/// - **Single artifact** (`proof` field): Legacy single-file proof
/// - **Proof folder** (`proof_folder` field): Multi-artifact folder with narrative
///
/// A claim can have either, both (unusual), or neither (no proof yet).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestClaim {
    /// Who is claiming completion
    pub claimant: MemberId,
    /// Documentation/proof artifact (optional, legacy single-artifact proof)
    pub proof: Option<ArtifactId>,
    /// Proof folder reference (optional, new multi-artifact proof with narrative)
    #[serde(default)]
    pub proof_folder: Option<ProofFolderId>,
    /// When the claim was submitted (Unix timestamp in milliseconds)
    pub submitted_at_millis: i64,
    /// Whether the creator has verified this claim
    pub verified: bool,
    /// When the claim was verified (None if not yet verified)
    pub verified_at_millis: Option<i64>,
}

impl QuestClaim {
    /// Create a new unverified claim.
    pub fn new(claimant: MemberId, proof: Option<ArtifactId>) -> Self {
        Self {
            claimant,
            proof,
            proof_folder: None,
            submitted_at_millis: chrono::Utc::now().timestamp_millis(),
            verified: false,
            verified_at_millis: None,
        }
    }

    /// Create a new unverified claim with a proof folder.
    pub fn with_proof_folder(claimant: MemberId, proof_folder: ProofFolderId) -> Self {
        Self {
            claimant,
            proof: None,
            proof_folder: Some(proof_folder),
            submitted_at_millis: chrono::Utc::now().timestamp_millis(),
            verified: false,
            verified_at_millis: None,
        }
    }

    /// Check if this claim is verified.
    pub fn is_verified(&self) -> bool {
        self.verified
    }

    /// Check if this claim has a proof folder.
    pub fn has_proof_folder(&self) -> bool {
        self.proof_folder.is_some()
    }

    /// Check if this claim has any proof (artifact or folder).
    pub fn has_proof(&self) -> bool {
        self.proof.is_some() || self.proof_folder.is_some()
    }

    /// Mark this claim as verified.
    pub fn verify(&mut self) {
        if !self.verified {
            self.verified = true;
            self.verified_at_millis = Some(chrono::Utc::now().timestamp_millis());
        }
    }

    /// Set the proof folder for this claim.
    pub fn set_proof_folder(&mut self, folder_id: ProofFolderId) {
        self.proof_folder = Some(folder_id);
    }
}

/// A quest - a lightweight intention or task within a realm.
///
/// Quests support a Proof of Service model where multiple members can submit
/// claims with proof artifacts, and the creator verifies valid claims.
///
/// # Example
///
/// ```ignore
/// // Create a quest
/// let quest_id = realm.create_quest(
///     "Review design doc",
///     "Please review the attached PDF and leave comments",
///     None,
///     my_id,
/// ).await?;
///
/// // Members submit claims with proof
/// realm.submit_quest_claim(quest_id, member1_id, Some(proof_artifact)).await?;
/// realm.submit_quest_claim(quest_id, member2_id, Some(proof_artifact)).await?;
///
/// // Creator verifies valid claims
/// realm.verify_quest_claim(quest_id, 0).await?;  // Verify first claim
///
/// // Creator marks quest complete
/// realm.complete_quest(quest_id).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Quest {
    /// Unique identifier for this quest.
    pub id: QuestId,
    /// Short title describing the quest.
    pub title: String,
    /// Detailed description of what needs to be done.
    pub description: String,
    /// Optional image artifact associated with the quest.
    pub image: Option<ArtifactId>,
    /// The member who created this quest.
    pub creator: MemberId,
    /// Claims/proofs of service from members (multiple claimants supported).
    pub claims: Vec<QuestClaim>,
    /// When the quest was created (Unix timestamp in milliseconds).
    pub created_at_millis: i64,
    /// When the quest was completed (None if not yet complete).
    pub completed_at_millis: Option<i64>,
}

impl Quest {
    /// Create a new quest.
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        image: Option<ArtifactId>,
        creator: MemberId,
    ) -> Self {
        Self {
            id: generate_quest_id(),
            title: title.into(),
            description: description.into(),
            image,
            creator,
            claims: Vec::new(),
            created_at_millis: chrono::Utc::now().timestamp_millis(),
            completed_at_millis: None,
        }
    }

    /// Check if this quest has any claims.
    pub fn has_claims(&self) -> bool {
        !self.claims.is_empty()
    }

    /// Check if this quest has any verified claims.
    pub fn has_verified_claims(&self) -> bool {
        self.claims.iter().any(|c| c.verified)
    }

    /// Check if this quest is complete.
    pub fn is_complete(&self) -> bool {
        self.completed_at_millis.is_some()
    }

    /// Check if this quest is open (no claims and not complete).
    pub fn is_open(&self) -> bool {
        !self.has_claims() && !self.is_complete()
    }

    /// Submit a claim/proof of service for this quest.
    ///
    /// Multiple members can submit claims for the same quest.
    /// Returns an error if the quest is already complete.
    pub fn submit_claim(
        &mut self,
        claimant: MemberId,
        proof: Option<ArtifactId>,
    ) -> Result<usize, QuestError> {
        if self.completed_at_millis.is_some() {
            return Err(QuestError::AlreadyComplete);
        }
        let claim = QuestClaim::new(claimant, proof);
        self.claims.push(claim);
        Ok(self.claims.len() - 1) // Return the index of the new claim
    }

    /// Verify a specific claim by index.
    ///
    /// Only the quest creator should call this (enforced at higher level).
    /// Returns an error if the claim index is invalid.
    pub fn verify_claim(&mut self, claim_index: usize) -> Result<(), QuestError> {
        if claim_index >= self.claims.len() {
            return Err(QuestError::ClaimNotFound);
        }
        self.claims[claim_index].verify();
        Ok(())
    }

    /// Get all pending (unverified) claims.
    pub fn pending_claims(&self) -> Vec<&QuestClaim> {
        self.claims.iter().filter(|c| !c.verified).collect()
    }

    /// Get all verified claims.
    pub fn verified_claims(&self) -> Vec<&QuestClaim> {
        self.claims.iter().filter(|c| c.verified).collect()
    }

    /// Get a claim by index.
    pub fn get_claim(&self, index: usize) -> Option<&QuestClaim> {
        self.claims.get(index)
    }

    /// Get the number of claims.
    pub fn claim_count(&self) -> usize {
        self.claims.len()
    }

    /// Mark this quest as complete.
    ///
    /// Returns an error if already complete.
    pub fn complete(&mut self) -> Result<(), QuestError> {
        if self.completed_at_millis.is_some() {
            return Err(QuestError::AlreadyComplete);
        }
        self.completed_at_millis = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    }

    // === Legacy compatibility methods ===

    /// Check if this quest has been claimed (legacy compatibility).
    ///
    /// Returns true if there's at least one claim.
    #[deprecated(since = "0.2.0", note = "Use has_claims() instead")]
    pub fn is_claimed(&self) -> bool {
        self.has_claims()
    }

    /// Claim this quest for a member (legacy compatibility).
    ///
    /// Submits a claim without proof.
    #[deprecated(since = "0.2.0", note = "Use submit_claim() instead")]
    pub fn claim(&mut self, doer: MemberId) -> Result<(), QuestError> {
        self.submit_claim(doer, None)?;
        Ok(())
    }

    /// Unclaim this quest (legacy compatibility).
    ///
    /// Removes all unverified claims from the member.
    #[deprecated(since = "0.2.0", note = "Claims cannot be removed in proof-of-service model")]
    pub fn unclaim(&mut self) -> Result<(), QuestError> {
        if self.completed_at_millis.is_some() {
            return Err(QuestError::AlreadyComplete);
        }
        // Remove all unverified claims
        self.claims.retain(|c| c.verified);
        Ok(())
    }

    /// Get the doer (legacy compatibility).
    ///
    /// Returns the first claimant if any claims exist.
    #[deprecated(since = "0.2.0", note = "Use claims field directly")]
    pub fn doer(&self) -> Option<MemberId> {
        self.claims.first().map(|c| c.claimant)
    }
}

/// Errors that can occur during quest operations.
#[derive(Debug, Clone, PartialEq)]
pub enum QuestError {
    /// The quest has already been claimed by someone (legacy).
    AlreadyClaimed,
    /// The quest has already been completed.
    AlreadyComplete,
    /// The quest was not found.
    NotFound,
    /// The claim was not found at the specified index.
    ClaimNotFound,
}

impl std::fmt::Display for QuestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuestError::AlreadyClaimed => write!(f, "Quest is already claimed"),
            QuestError::AlreadyComplete => write!(f, "Quest is already complete"),
            QuestError::NotFound => write!(f, "Quest not found"),
            QuestError::ClaimNotFound => write!(f, "Claim not found"),
        }
    }
}

impl std::error::Error for QuestError {}

/// Document schema for storing quests in a realm.
///
/// This is used with `realm.document::<QuestDocument>("quests")` to get
/// a CRDT-synchronized quest list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuestDocument {
    /// All quests in this realm.
    pub quests: Vec<Quest>,
}

impl QuestDocument {
    /// Create a new empty quest document.
    pub fn new() -> Self {
        Self { quests: Vec::new() }
    }

    /// Add a quest to the document.
    pub fn add(&mut self, quest: Quest) {
        self.quests.push(quest);
    }

    /// Find a quest by ID.
    pub fn find(&self, id: &QuestId) -> Option<&Quest> {
        self.quests.iter().find(|q| &q.id == id)
    }

    /// Find a quest by ID (mutable).
    pub fn find_mut(&mut self, id: &QuestId) -> Option<&mut Quest> {
        self.quests.iter_mut().find(|q| &q.id == id)
    }

    /// Get all open quests (unclaimed and incomplete).
    pub fn open_quests(&self) -> Vec<&Quest> {
        self.quests.iter().filter(|q| q.is_open()).collect()
    }

    /// Get all quests with claims but not yet complete.
    pub fn in_progress_quests(&self) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|q| q.has_claims() && !q.is_complete())
            .collect()
    }

    /// Get all completed quests.
    pub fn completed_quests(&self) -> Vec<&Quest> {
        self.quests.iter().filter(|q| q.is_complete()).collect()
    }

    /// Get quests created by a specific member.
    pub fn quests_by_creator(&self, creator: &MemberId) -> Vec<&Quest> {
        self.quests.iter().filter(|q| &q.creator == creator).collect()
    }

    /// Get quests with claims by a specific member.
    pub fn quests_by_claimant(&self, claimant: &MemberId) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|q| q.claims.iter().any(|c| &c.claimant == claimant))
            .collect()
    }

    /// Get quests with verified claims.
    pub fn quests_with_verified_claims(&self) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|q| q.has_verified_claims())
            .collect()
    }

    /// Get quests with pending (unverified) claims.
    pub fn quests_with_pending_claims(&self) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|q| !q.pending_claims().is_empty())
            .collect()
    }

    /// Legacy compatibility: Get quests claimed by a specific member.
    #[deprecated(since = "0.2.0", note = "Use quests_by_claimant() instead")]
    pub fn quests_by_doer(&self, doer: &MemberId) -> Vec<&Quest> {
        self.quests_by_claimant(doer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member_id() -> MemberId {
        [1u8; 32]
    }

    fn another_member_id() -> MemberId {
        [2u8; 32]
    }

    fn third_member_id() -> MemberId {
        [3u8; 32]
    }

    #[test]
    fn test_quest_creation() {
        let quest = Quest::new("Test quest", "Do something", None, test_member_id());
        assert!(!quest.has_claims());
        assert!(!quest.is_complete());
        assert!(quest.is_open());
        assert_eq!(quest.claim_count(), 0);
    }

    #[test]
    fn test_quest_claim_new_model() {
        let mut quest = Quest::new("Test quest", "Do something", None, test_member_id());

        // Submit first claim
        let claim_idx = quest.submit_claim(another_member_id(), None).unwrap();
        assert_eq!(claim_idx, 0);
        assert!(quest.has_claims());
        assert!(!quest.is_open());
        assert_eq!(quest.claim_count(), 1);

        // Submit second claim (multiple claims allowed)
        let claim_idx2 = quest.submit_claim(third_member_id(), Some([42u8; 32])).unwrap();
        assert_eq!(claim_idx2, 1);
        assert_eq!(quest.claim_count(), 2);

        // Both claims should be unverified
        assert!(!quest.has_verified_claims());
        assert_eq!(quest.pending_claims().len(), 2);
        assert_eq!(quest.verified_claims().len(), 0);
    }

    #[test]
    fn test_quest_verify_claim() {
        let mut quest = Quest::new("Test quest", "Do something", None, test_member_id());

        quest.submit_claim(another_member_id(), None).unwrap();
        quest.submit_claim(third_member_id(), Some([42u8; 32])).unwrap();

        // Verify first claim
        assert!(quest.verify_claim(0).is_ok());
        assert!(quest.has_verified_claims());
        assert_eq!(quest.pending_claims().len(), 1);
        assert_eq!(quest.verified_claims().len(), 1);

        // Verify second claim
        assert!(quest.verify_claim(1).is_ok());
        assert_eq!(quest.pending_claims().len(), 0);
        assert_eq!(quest.verified_claims().len(), 2);

        // Invalid claim index
        assert_eq!(quest.verify_claim(5), Err(QuestError::ClaimNotFound));
    }

    #[test]
    fn test_quest_complete() {
        let mut quest = Quest::new("Test quest", "Do something", None, test_member_id());
        quest.submit_claim(another_member_id(), None).unwrap();
        quest.verify_claim(0).unwrap();

        assert!(quest.complete().is_ok());
        assert!(quest.is_complete());

        // Can't complete twice
        assert_eq!(quest.complete(), Err(QuestError::AlreadyComplete));

        // Can't add claims after completion
        assert_eq!(
            quest.submit_claim(third_member_id(), None),
            Err(QuestError::AlreadyComplete)
        );
    }

    #[test]
    fn test_quest_claim_struct() {
        let claim = QuestClaim::new(another_member_id(), Some([42u8; 32]));
        assert!(!claim.is_verified());
        assert!(claim.verified_at_millis.is_none());
        assert!(claim.proof.is_some());

        let mut claim2 = QuestClaim::new(third_member_id(), None);
        claim2.verify();
        assert!(claim2.is_verified());
        assert!(claim2.verified_at_millis.is_some());
    }

    #[test]
    fn test_quest_document() {
        let mut doc = QuestDocument::new();
        let quest = Quest::new("Test quest", "Do something", None, test_member_id());
        let id = quest.id;
        doc.add(quest);

        assert!(doc.find(&id).is_some());
        assert_eq!(doc.open_quests().len(), 1);
        assert_eq!(doc.completed_quests().len(), 0);

        // Submit claims
        doc.find_mut(&id)
            .unwrap()
            .submit_claim(another_member_id(), None)
            .unwrap();
        assert_eq!(doc.open_quests().len(), 0);
        assert_eq!(doc.in_progress_quests().len(), 1);
        assert_eq!(doc.quests_with_pending_claims().len(), 1);

        // Verify and complete
        doc.find_mut(&id).unwrap().verify_claim(0).unwrap();
        assert_eq!(doc.quests_with_verified_claims().len(), 1);

        doc.find_mut(&id).unwrap().complete().unwrap();
        assert_eq!(doc.completed_quests().len(), 1);
    }

    #[test]
    fn test_quest_document_by_claimant() {
        let mut doc = QuestDocument::new();

        let mut quest1 = Quest::new("Quest 1", "Do something", None, test_member_id());
        quest1.submit_claim(another_member_id(), None).unwrap();
        let id1 = quest1.id;

        let mut quest2 = Quest::new("Quest 2", "Do something else", None, test_member_id());
        quest2.submit_claim(third_member_id(), None).unwrap();

        doc.add(quest1);
        doc.add(quest2);

        let by_claimant = doc.quests_by_claimant(&another_member_id());
        assert_eq!(by_claimant.len(), 1);
        assert_eq!(by_claimant[0].id, id1);
    }

    // Legacy compatibility tests
    #[test]
    #[allow(deprecated)]
    fn test_legacy_claim_method() {
        let mut quest = Quest::new("Test quest", "Do something", None, test_member_id());
        assert!(quest.claim(another_member_id()).is_ok());
        assert!(quest.is_claimed());
    }

    #[test]
    #[allow(deprecated)]
    fn test_legacy_doer_method() {
        let mut quest = Quest::new("Test quest", "Do something", None, test_member_id());
        assert!(quest.doer().is_none());
        quest.submit_claim(another_member_id(), None).unwrap();
        assert_eq!(quest.doer(), Some(another_member_id()));
    }
}
