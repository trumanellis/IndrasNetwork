//! Intention — the core unit of collaborative action within realms.
//!
//! An Intention represents something a member wants to bring into the world.
//! It can be an Intention (task for others), Need (request for help), Offering
//! (service provided), or a general Intention.
//!
//! Intentions support a Proof of Service model:
//!
//! 1. Creator posts an intention
//! 2. Members submit service claims with proof artifacts
//! 3. Creator verifies valid claims
//! 4. Creator marks intention complete
//!
//! Intentions are CRDT-synchronized across all realm members.

use indras_network::artifact::ArtifactId;
use indras_network::member::MemberId;
use crate::proof_folder::ProofFolderId;

use serde::{Deserialize, Serialize};

/// The kind/subtype of an intention.
///
/// This determines the UI presentation and workflow expectations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum IntentionKind {
    /// A task or mission for others to fulfill.
    Quest,
    /// A request for help or resources.
    Need,
    /// A service or resource being offered.
    Offering,
    /// A general intention (default).
    #[default]
    Intention,
}

impl std::fmt::Display for IntentionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntentionKind::Quest => write!(f, "Quest"),
            IntentionKind::Need => write!(f, "Need"),
            IntentionKind::Offering => write!(f, "Offering"),
            IntentionKind::Intention => write!(f, "Intention"),
        }
    }
}

impl IntentionKind {
    /// Returns an emoji icon representing this kind.
    pub fn icon(&self) -> &str {
        match self {
            IntentionKind::Quest => "\u{2694}",
            IntentionKind::Need => "\u{1F331}",
            IntentionKind::Offering => "\u{1F381}",
            IntentionKind::Intention => "\u{2728}",
        }
    }

    /// Returns a CSS class name for this kind.
    pub fn css_class(&self) -> &str {
        match self {
            IntentionKind::Quest => "type-quest",
            IntentionKind::Need => "type-need",
            IntentionKind::Offering => "type-offering",
            IntentionKind::Intention => "type-intention",
        }
    }

    /// Returns a human-readable label for this kind.
    pub fn label(&self) -> &str {
        match self {
            IntentionKind::Quest => "Quest",
            IntentionKind::Need => "Need",
            IntentionKind::Offering => "Offering",
            IntentionKind::Intention => "Intention",
        }
    }
}

/// Priority level for intentions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntentionPriority {
    /// Low priority - nice to have.
    Low,
    /// Normal priority (default).
    Normal,
    /// High priority - should be done soon.
    High,
    /// Urgent - needs immediate attention.
    Urgent,
}

impl Default for IntentionPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Unique identifier for an intention (16 bytes).
pub type IntentionId = [u8; 16];

/// Generate a new random intention ID.
pub fn generate_intention_id() -> IntentionId {
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

/// A claim/proof of service for an intention.
///
/// Members submit claims to demonstrate they've completed work for an intention.
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
pub struct ServiceClaim {
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

impl ServiceClaim {
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

/// An intention - a lightweight intention or task within a realm.
///
/// Intentions support a Proof of Service model where multiple members can submit
/// claims with proof artifacts, and the creator verifies valid claims.
///
/// # Example
///
/// ```ignore
/// // Create a quest
/// let intention_id = realm.create_intention(
///     "Review design doc",
///     "Please review the attached PDF and leave comments",
///     None,
///     my_id,
/// ).await?;
///
/// // Members submit claims with proof
/// realm.submit_service_claim(intention_id, member1_id, Some(proof_artifact)).await?;
/// realm.submit_service_claim(intention_id, member2_id, Some(proof_artifact)).await?;
///
/// // Creator verifies valid claims
/// realm.verify_service_claim(intention_id, 0).await?;  // Verify first claim
///
/// // Creator marks quest complete
/// realm.complete_intention(intention_id).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Intention {
    /// Unique identifier for this intention.
    pub id: IntentionId,
    /// The kind of intention (Quest, Need, Offering, or Intention).
    #[serde(default)]
    pub kind: IntentionKind,
    /// Short title describing the intention.
    pub title: String,
    /// Detailed description of what needs to be done.
    pub description: String,
    /// Optional image artifact associated with the intention.
    pub image: Option<ArtifactId>,
    /// The member who created this intention.
    pub creator: MemberId,
    /// Service claims/proofs from members (multiple claimants supported).
    pub claims: Vec<ServiceClaim>,
    /// When the intention was created (Unix timestamp in milliseconds).
    pub created_at_millis: i64,
    /// When the intention was completed (None if not yet complete).
    pub completed_at_millis: Option<i64>,
    /// Optional deadline (Unix timestamp in milliseconds).
    #[serde(default)]
    pub deadline_millis: Option<i64>,
    /// Priority level.
    #[serde(default)]
    pub priority: IntentionPriority,
}

impl Intention {
    /// Create a new intention with default kind (`IntentionKind::Intention`).
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        image: Option<ArtifactId>,
        creator: MemberId,
    ) -> Self {
        Self {
            id: generate_intention_id(),
            kind: IntentionKind::default(),
            title: title.into(),
            description: description.into(),
            image,
            creator,
            claims: Vec::new(),
            created_at_millis: chrono::Utc::now().timestamp_millis(),
            completed_at_millis: None,
            deadline_millis: None,
            priority: IntentionPriority::default(),
        }
    }

    /// Check if this intention has any claims.
    pub fn has_claims(&self) -> bool {
        !self.claims.is_empty()
    }

    /// Check if this intention has any verified claims.
    pub fn has_verified_claims(&self) -> bool {
        self.claims.iter().any(|c| c.verified)
    }

    /// Check if this intention is complete.
    pub fn is_complete(&self) -> bool {
        self.completed_at_millis.is_some()
    }

    /// Check if this intention is open (no claims and not complete).
    pub fn is_open(&self) -> bool {
        !self.has_claims() && !self.is_complete()
    }

    /// Submit a claim/proof of service for this intention.
    ///
    /// Multiple members can submit claims for the same intention.
    /// Returns an error if the intention is already complete.
    pub fn submit_claim(
        &mut self,
        claimant: MemberId,
        proof: Option<ArtifactId>,
    ) -> Result<usize, IntentionError> {
        if self.completed_at_millis.is_some() {
            return Err(IntentionError::AlreadyComplete);
        }
        let claim = ServiceClaim::new(claimant, proof);
        self.claims.push(claim);
        Ok(self.claims.len() - 1) // Return the index of the new claim
    }

    /// Verify a specific claim by index.
    ///
    /// Only the intention creator should call this (enforced at higher level).
    /// Returns an error if the claim index is invalid.
    pub fn verify_claim(&mut self, claim_index: usize) -> Result<(), IntentionError> {
        if claim_index >= self.claims.len() {
            return Err(IntentionError::ClaimNotFound);
        }
        self.claims[claim_index].verify();
        Ok(())
    }

    /// Get all pending (unverified) claims.
    pub fn pending_claims(&self) -> Vec<&ServiceClaim> {
        self.claims.iter().filter(|c| !c.verified).collect()
    }

    /// Get all verified claims.
    pub fn verified_claims(&self) -> Vec<&ServiceClaim> {
        self.claims.iter().filter(|c| c.verified).collect()
    }

    /// Get a claim by index.
    pub fn get_claim(&self, index: usize) -> Option<&ServiceClaim> {
        self.claims.get(index)
    }

    /// Get the number of claims.
    pub fn claim_count(&self) -> usize {
        self.claims.len()
    }

    /// Mark this intention as complete.
    ///
    /// Returns an error if already complete.
    pub fn complete(&mut self) -> Result<(), IntentionError> {
        if self.completed_at_millis.is_some() {
            return Err(IntentionError::AlreadyComplete);
        }
        self.completed_at_millis = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    }

    // === Deadline & Priority ===

    /// Set a deadline for this intention.
    ///
    /// # Arguments
    ///
    /// * `deadline_millis` - Unix timestamp in milliseconds
    pub fn set_deadline(&mut self, deadline_millis: i64) {
        self.deadline_millis = Some(deadline_millis);
    }

    /// Clear the deadline for this intention.
    pub fn clear_deadline(&mut self) {
        self.deadline_millis = None;
    }

    /// Check if this intention has a deadline.
    pub fn has_deadline(&self) -> bool {
        self.deadline_millis.is_some()
    }

    /// Check if this intention is overdue (past deadline and not complete).
    pub fn is_overdue(&self) -> bool {
        if let Some(deadline) = self.deadline_millis {
            chrono::Utc::now().timestamp_millis() > deadline && !self.is_complete()
        } else {
            false
        }
    }

    /// Set the priority for this intention.
    pub fn set_priority(&mut self, priority: IntentionPriority) {
        self.priority = priority;
    }

    /// Set the title for this intention.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Set the description for this intention.
    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = description.into();
    }

    // === Legacy compatibility methods ===

    /// Check if this intention has been claimed (legacy compatibility).
    ///
    /// Returns true if there's at least one claim.
    #[deprecated(since = "0.2.0", note = "Use has_claims() instead")]
    pub fn is_claimed(&self) -> bool {
        self.has_claims()
    }

    /// Claim this intention for a member (legacy compatibility).
    ///
    /// Submits a claim without proof.
    #[deprecated(since = "0.2.0", note = "Use submit_claim() instead")]
    pub fn claim(&mut self, doer: MemberId) -> Result<(), IntentionError> {
        self.submit_claim(doer, None)?;
        Ok(())
    }

    /// Unclaim this intention (legacy compatibility).
    ///
    /// Removes all unverified claims from the member.
    #[deprecated(since = "0.2.0", note = "Claims cannot be removed in proof-of-service model")]
    pub fn unclaim(&mut self) -> Result<(), IntentionError> {
        if self.completed_at_millis.is_some() {
            return Err(IntentionError::AlreadyComplete);
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

/// Errors that can occur during intention operations.
#[derive(Debug, Clone, PartialEq)]
pub enum IntentionError {
    /// The intention has already been claimed by someone (legacy).
    AlreadyClaimed,
    /// The intention has already been completed.
    AlreadyComplete,
    /// The intention was not found.
    NotFound,
    /// The claim was not found at the specified index.
    ClaimNotFound,
    /// The caller is not authorized for this operation.
    NotAuthorized,
}

impl std::fmt::Display for IntentionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntentionError::AlreadyClaimed => write!(f, "Intention is already claimed"),
            IntentionError::AlreadyComplete => write!(f, "Intention is already complete"),
            IntentionError::NotFound => write!(f, "Intention not found"),
            IntentionError::ClaimNotFound => write!(f, "Claim not found"),
            IntentionError::NotAuthorized => write!(f, "Not authorized for this operation"),
        }
    }
}

impl std::error::Error for IntentionError {}

/// Document schema for storing intentions in a realm.
///
/// This is used with `realm.document::<IntentionDocument>("intentions")` to get
/// a CRDT-synchronized intention list.
///
/// # CRDT Semantics
///
/// - Intentions are identified by their unique `IntentionId`
/// - Merge strategy: set-union by intention ID (no data loss on concurrent edits)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntentionDocument {
    /// All intentions in this realm.
    pub intentions: Vec<Intention>,
}

impl indras_network::document::DocumentSchema for IntentionDocument {
    fn merge(&mut self, remote: Self) {
        let mut by_id: std::collections::HashMap<IntentionId, usize> =
            self.intentions.iter().enumerate().map(|(i, q)| (q.id, i)).collect();

        for remote_intention in remote.intentions {
            if let Some(&idx) = by_id.get(&remote_intention.id) {
                let local = &mut self.intentions[idx];

                // Merge claims: union by claimant, prefer verified over unverified.
                for remote_claim in &remote_intention.claims {
                    if let Some(local_claim) = local.claims.iter_mut().find(|c| c.claimant == remote_claim.claimant) {
                        if remote_claim.verified && !local_claim.verified {
                            *local_claim = remote_claim.clone();
                        }
                    } else {
                        local.claims.push(remote_claim.clone());
                    }
                }

                // Completion: propagate non-None (prefer earliest).
                match (local.completed_at_millis, remote_intention.completed_at_millis) {
                    (None, Some(t)) => local.completed_at_millis = Some(t),
                    (Some(a), Some(b)) if b < a => local.completed_at_millis = Some(b),
                    _ => {}
                }

                // Deadline: take the later value.
                match (local.deadline_millis, remote_intention.deadline_millis) {
                    (None, Some(d)) => local.deadline_millis = Some(d),
                    (Some(a), Some(b)) if b > a => local.deadline_millis = Some(b),
                    _ => {}
                }

                // Priority: take the higher value.
                if remote_intention.priority > local.priority {
                    local.priority = remote_intention.priority;
                }
            } else {
                by_id.insert(remote_intention.id, self.intentions.len());
                self.intentions.push(remote_intention);
            }
        }
    }
}

impl IntentionDocument {
    /// Create a new empty intention document.
    pub fn new() -> Self {
        Self { intentions: Vec::new() }
    }

    /// Add an intention to the document.
    pub fn add(&mut self, intention: Intention) {
        self.intentions.push(intention);
    }

    /// Find an intention by ID.
    pub fn find(&self, id: &IntentionId) -> Option<&Intention> {
        self.intentions.iter().find(|q| &q.id == id)
    }

    /// Find an intention by ID (mutable).
    pub fn find_mut(&mut self, id: &IntentionId) -> Option<&mut Intention> {
        self.intentions.iter_mut().find(|q| &q.id == id)
    }

    /// Get all open intentions (unclaimed and incomplete).
    pub fn open_intentions(&self) -> Vec<&Intention> {
        self.intentions.iter().filter(|q| q.is_open()).collect()
    }

    /// Get all intentions with claims but not yet complete.
    pub fn in_progress_intentions(&self) -> Vec<&Intention> {
        self.intentions
            .iter()
            .filter(|q| q.has_claims() && !q.is_complete())
            .collect()
    }

    /// Get all completed intentions.
    pub fn completed_intentions(&self) -> Vec<&Intention> {
        self.intentions.iter().filter(|q| q.is_complete()).collect()
    }

    /// Get intentions created by a specific member.
    pub fn intentions_by_creator(&self, creator: &MemberId) -> Vec<&Intention> {
        self.intentions.iter().filter(|q| &q.creator == creator).collect()
    }

    /// Get intentions with claims by a specific member.
    pub fn intentions_by_claimant(&self, claimant: &MemberId) -> Vec<&Intention> {
        self.intentions
            .iter()
            .filter(|q| q.claims.iter().any(|c| &c.claimant == claimant))
            .collect()
    }

    /// Get intentions with verified claims.
    pub fn intentions_with_verified_claims(&self) -> Vec<&Intention> {
        self.intentions
            .iter()
            .filter(|q| q.has_verified_claims())
            .collect()
    }

    /// Get intentions with pending (unverified) claims.
    pub fn intentions_with_pending_claims(&self) -> Vec<&Intention> {
        self.intentions
            .iter()
            .filter(|q| !q.pending_claims().is_empty())
            .collect()
    }

    /// Get intentions sorted by priority (highest first), excluding completed.
    pub fn intentions_by_priority(&self) -> Vec<&Intention> {
        let mut items: Vec<_> = self.intentions.iter().filter(|q| !q.is_complete()).collect();
        items.sort_by(|a, b| b.priority.cmp(&a.priority));
        items
    }

    /// Get overdue intentions (past deadline and not complete).
    pub fn overdue_intentions(&self) -> Vec<&Intention> {
        self.intentions.iter().filter(|q| q.is_overdue()).collect()
    }

    /// Remove an intention by ID. Returns the removed intention if found.
    pub fn remove(&mut self, id: &IntentionId) -> Option<Intention> {
        if let Some(pos) = self.intentions.iter().position(|q| &q.id == id) {
            Some(self.intentions.remove(pos))
        } else {
            None
        }
    }

    /// Get the number of intentions.
    pub fn intention_count(&self) -> usize {
        self.intentions.len()
    }

    /// Search intentions by title or description.
    pub fn search(&self, query: &str) -> Vec<&Intention> {
        let query_lower = query.to_lowercase();
        self.intentions
            .iter()
            .filter(|q| {
                q.title.to_lowercase().contains(&query_lower)
                    || q.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Legacy compatibility: Get intentions claimed by a specific member.
    #[deprecated(since = "0.2.0", note = "Use intentions_by_claimant() instead")]
    pub fn intentions_by_doer(&self, doer: &MemberId) -> Vec<&Intention> {
        self.intentions_by_claimant(doer)
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
    fn test_intention_creation() {
        let intention = Intention::new("Test quest", "Do something", None, test_member_id());
        assert!(!intention.has_claims());
        assert!(!intention.is_complete());
        assert!(intention.is_open());
        assert_eq!(intention.claim_count(), 0);
    }

    #[test]
    fn test_intention_claim_new_model() {
        let mut intention = Intention::new("Test quest", "Do something", None, test_member_id());

        // Submit first claim
        let claim_idx = intention.submit_claim(another_member_id(), None).unwrap();
        assert_eq!(claim_idx, 0);
        assert!(intention.has_claims());
        assert!(!intention.is_open());
        assert_eq!(intention.claim_count(), 1);

        // Submit second claim (multiple claims allowed)
        let claim_idx2 = intention.submit_claim(third_member_id(), Some(ArtifactId::Blob([42u8; 32]))).unwrap();
        assert_eq!(claim_idx2, 1);
        assert_eq!(intention.claim_count(), 2);

        // Both claims should be unverified
        assert!(!intention.has_verified_claims());
        assert_eq!(intention.pending_claims().len(), 2);
        assert_eq!(intention.verified_claims().len(), 0);
    }

    #[test]
    fn test_intention_verify_claim() {
        let mut intention = Intention::new("Test quest", "Do something", None, test_member_id());

        intention.submit_claim(another_member_id(), None).unwrap();
        intention.submit_claim(third_member_id(), Some(ArtifactId::Blob([42u8; 32]))).unwrap();

        // Verify first claim
        assert!(intention.verify_claim(0).is_ok());
        assert!(intention.has_verified_claims());
        assert_eq!(intention.pending_claims().len(), 1);
        assert_eq!(intention.verified_claims().len(), 1);

        // Verify second claim
        assert!(intention.verify_claim(1).is_ok());
        assert_eq!(intention.pending_claims().len(), 0);
        assert_eq!(intention.verified_claims().len(), 2);

        // Invalid claim index
        assert_eq!(intention.verify_claim(5), Err(IntentionError::ClaimNotFound));
    }

    #[test]
    fn test_intention_complete() {
        let mut intention = Intention::new("Test quest", "Do something", None, test_member_id());
        intention.submit_claim(another_member_id(), None).unwrap();
        intention.verify_claim(0).unwrap();

        assert!(intention.complete().is_ok());
        assert!(intention.is_complete());

        // Can't complete twice
        assert_eq!(intention.complete(), Err(IntentionError::AlreadyComplete));

        // Can't add claims after completion
        assert_eq!(
            intention.submit_claim(third_member_id(), None),
            Err(IntentionError::AlreadyComplete)
        );
    }

    #[test]
    fn test_intention_claim_struct() {
        let claim = ServiceClaim::new(another_member_id(), Some(ArtifactId::Blob([42u8; 32])));
        assert!(!claim.is_verified());
        assert!(claim.verified_at_millis.is_none());
        assert!(claim.proof.is_some());

        let mut claim2 = ServiceClaim::new(third_member_id(), None);
        claim2.verify();
        assert!(claim2.is_verified());
        assert!(claim2.verified_at_millis.is_some());
    }

    #[test]
    fn test_intention_document() {
        let mut doc = IntentionDocument::new();
        let intention = Intention::new("Test quest", "Do something", None, test_member_id());
        let id = intention.id;
        doc.add(intention);

        assert!(doc.find(&id).is_some());
        assert_eq!(doc.open_intentions().len(), 1);
        assert_eq!(doc.completed_intentions().len(), 0);

        // Submit claims
        doc.find_mut(&id)
            .unwrap()
            .submit_claim(another_member_id(), None)
            .unwrap();
        assert_eq!(doc.open_intentions().len(), 0);
        assert_eq!(doc.in_progress_intentions().len(), 1);
        assert_eq!(doc.intentions_with_pending_claims().len(), 1);

        // Verify and complete
        doc.find_mut(&id).unwrap().verify_claim(0).unwrap();
        assert_eq!(doc.intentions_with_verified_claims().len(), 1);

        doc.find_mut(&id).unwrap().complete().unwrap();
        assert_eq!(doc.completed_intentions().len(), 1);
    }

    #[test]
    fn test_intention_document_by_claimant() {
        let mut doc = IntentionDocument::new();

        let mut quest1 = Intention::new("Intention 1", "Do something", None, test_member_id());
        quest1.submit_claim(another_member_id(), None).unwrap();
        let id1 = quest1.id;

        let mut quest2 = Intention::new("Intention 2", "Do something else", None, test_member_id());
        quest2.submit_claim(third_member_id(), None).unwrap();

        doc.add(quest1);
        doc.add(quest2);

        let by_claimant = doc.intentions_by_claimant(&another_member_id());
        assert_eq!(by_claimant.len(), 1);
        assert_eq!(by_claimant[0].id, id1);
    }

    // Legacy compatibility tests
    #[test]
    #[allow(deprecated)]
    fn test_legacy_claim_method() {
        let mut intention = Intention::new("Test quest", "Do something", None, test_member_id());
        assert!(intention.claim(another_member_id()).is_ok());
        assert!(intention.is_claimed());
    }

    #[test]
    #[allow(deprecated)]
    fn test_legacy_doer_method() {
        let mut intention = Intention::new("Test quest", "Do something", None, test_member_id());
        assert!(intention.doer().is_none());
        intention.submit_claim(another_member_id(), None).unwrap();
        assert_eq!(intention.doer(), Some(another_member_id()));
    }
}
