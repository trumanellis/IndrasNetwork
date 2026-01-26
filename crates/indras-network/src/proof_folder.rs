//! Proof Folder - CRDT-synchronized documentation folders for quest proofs.
//!
//! When a realm member submits a proof of service for a quest, they create
//! a proof folder containing:
//! - A markdown narrative explaining what was done
//! - Supporting artifacts (photos, videos, documents)
//!
//! Proof folders have a draft/submitted lifecycle:
//! - **Draft**: Folder is being edited, syncs to peers but no notification
//! - **Submitted**: Folder is finalized, triggers chat notification
//!
//! A single claimant can submit multiple proof folders over the life of a quest,
//! documenting different sub-parts of the work.

use crate::artifact::ArtifactId;
use crate::member::MemberId;
use crate::quest::QuestId;
use serde::{Deserialize, Serialize};

/// Unique identifier for a proof folder (16 bytes).
pub type ProofFolderId = [u8; 16];

/// Generate a new random proof folder ID.
pub fn generate_proof_folder_id() -> ProofFolderId {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut id = [0u8; 16];

    // Use timestamp for first 8 bytes (uniqueness over time)
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    id[..8].copy_from_slice(&timestamp.to_le_bytes());

    // Use blake3 hash of timestamp + counter for remaining bytes
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let hash = blake3::hash(&[&timestamp.to_le_bytes()[..], &counter.to_le_bytes()[..]].concat());
    id[8..].copy_from_slice(&hash.as_bytes()[..8]);

    id
}

/// Status of a proof folder in its lifecycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ProofFolderStatus {
    /// Still being edited by claimant. Syncs to peers but no chat notification.
    #[default]
    Draft,
    /// Submitted for review. Triggers chat notification to all realm members.
    Submitted,
}

impl ProofFolderStatus {
    /// Check if this is a draft.
    pub fn is_draft(&self) -> bool {
        matches!(self, ProofFolderStatus::Draft)
    }

    /// Check if this is submitted.
    pub fn is_submitted(&self) -> bool {
        matches!(self, ProofFolderStatus::Submitted)
    }
}

/// Metadata for an artifact within a proof folder.
///
/// Each artifact in a proof folder has associated metadata for display
/// and organization purposes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProofFolderArtifact {
    /// Content hash (BLAKE3) - links to the actual blob in storage.
    pub artifact_id: ArtifactId,
    /// Original filename.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type if known (e.g., "image/jpeg", "video/mp4").
    pub mime_type: Option<String>,
    /// Optional caption/description for this artifact.
    pub caption: Option<String>,
    /// When added to folder (Unix timestamp in milliseconds).
    pub added_at_millis: i64,
}

impl ProofFolderArtifact {
    /// Create a new artifact entry.
    pub fn new(
        artifact_id: ArtifactId,
        name: impl Into<String>,
        size: u64,
        mime_type: Option<String>,
    ) -> Self {
        Self {
            artifact_id,
            name: name.into(),
            size,
            mime_type,
            caption: None,
            added_at_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Create a new artifact entry with a caption.
    pub fn with_caption(
        artifact_id: ArtifactId,
        name: impl Into<String>,
        size: u64,
        mime_type: Option<String>,
        caption: impl Into<String>,
    ) -> Self {
        Self {
            artifact_id,
            name: name.into(),
            size,
            mime_type,
            caption: Some(caption.into()),
            added_at_millis: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// A proof folder - documentation of quest fulfillment.
///
/// Proof folders contain a narrative explanation and supporting artifacts
/// (photos, videos, documents) that demonstrate work completed for a quest.
///
/// # Lifecycle
///
/// 1. Created in Draft status via `realm.create_proof_folder()`
/// 2. Claimant adds narrative and artifacts
/// 3. Claimant submits via `realm.submit_proof_folder()`
/// 4. Status changes to Submitted, chat notification posted
///
/// # Example
///
/// ```ignore
/// // Create a proof folder for a quest
/// let folder_id = realm.create_proof_folder(quest_id, my_id).await?;
///
/// // Add narrative
/// realm.update_proof_folder_narrative(folder_id, "## Work completed\n\nI did the thing...").await?;
///
/// // Add supporting photos
/// let photo_artifact = ProofFolderArtifact::new(photo_hash, "before.jpg", 1024, Some("image/jpeg".into()));
/// realm.add_artifact_to_proof_folder(folder_id, photo_artifact).await?;
///
/// // Submit for review (triggers chat notification)
/// realm.submit_proof_folder(folder_id).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProofFolder {
    /// Unique identifier for this proof folder.
    pub id: ProofFolderId,
    /// The quest this proof is for.
    pub quest_id: QuestId,
    /// The member who created this proof folder.
    pub claimant: MemberId,
    /// Markdown narrative explaining what was done.
    pub narrative: String,
    /// Supporting artifacts (photos, videos, documents).
    pub artifacts: Vec<ProofFolderArtifact>,
    /// Current status (Draft or Submitted).
    pub status: ProofFolderStatus,
    /// When the folder was created (Unix timestamp in milliseconds).
    pub created_at_millis: i64,
    /// When the folder was submitted (None if still draft).
    pub submitted_at_millis: Option<i64>,
}

impl ProofFolder {
    /// Create a new proof folder in draft status.
    pub fn new(quest_id: QuestId, claimant: MemberId) -> Self {
        Self {
            id: generate_proof_folder_id(),
            quest_id,
            claimant,
            narrative: String::new(),
            artifacts: Vec::new(),
            status: ProofFolderStatus::Draft,
            created_at_millis: chrono::Utc::now().timestamp_millis(),
            submitted_at_millis: None,
        }
    }

    /// Check if this folder is in draft status.
    pub fn is_draft(&self) -> bool {
        self.status.is_draft()
    }

    /// Check if this folder has been submitted.
    pub fn is_submitted(&self) -> bool {
        self.status.is_submitted()
    }

    /// Set the narrative content.
    ///
    /// Returns an error if the folder is not in draft status.
    pub fn set_narrative(&mut self, narrative: impl Into<String>) -> Result<(), ProofFolderError> {
        if !self.is_draft() {
            return Err(ProofFolderError::NotDraft);
        }
        self.narrative = narrative.into();
        Ok(())
    }

    /// Add an artifact to this folder.
    ///
    /// Returns an error if the folder is not in draft status.
    pub fn add_artifact(&mut self, artifact: ProofFolderArtifact) -> Result<(), ProofFolderError> {
        if !self.is_draft() {
            return Err(ProofFolderError::NotDraft);
        }
        self.artifacts.push(artifact);
        Ok(())
    }

    /// Remove an artifact from this folder by artifact ID.
    ///
    /// Returns an error if the folder is not in draft status or artifact not found.
    pub fn remove_artifact(&mut self, artifact_id: &ArtifactId) -> Result<(), ProofFolderError> {
        if !self.is_draft() {
            return Err(ProofFolderError::NotDraft);
        }
        let initial_len = self.artifacts.len();
        self.artifacts.retain(|a| &a.artifact_id != artifact_id);
        if self.artifacts.len() == initial_len {
            return Err(ProofFolderError::ArtifactNotFound);
        }
        Ok(())
    }

    /// Submit this folder for review.
    ///
    /// Changes status to Submitted. This action is irreversible.
    /// Returns an error if already submitted.
    pub fn submit(&mut self) -> Result<(), ProofFolderError> {
        if self.is_submitted() {
            return Err(ProofFolderError::AlreadySubmitted);
        }
        self.status = ProofFolderStatus::Submitted;
        self.submitted_at_millis = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    }

    /// Get a preview of the narrative (first ~100 chars).
    pub fn narrative_preview(&self) -> String {
        let chars: String = self.narrative.chars().take(100).collect();
        if self.narrative.len() > 100 {
            format!("{}...", chars)
        } else {
            chars
        }
    }

    /// Get the number of artifacts in this folder.
    pub fn artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    /// Find an artifact by ID.
    pub fn find_artifact(&self, artifact_id: &ArtifactId) -> Option<&ProofFolderArtifact> {
        self.artifacts.iter().find(|a| &a.artifact_id == artifact_id)
    }
}

/// Errors that can occur during proof folder operations.
#[derive(Debug, Clone, PartialEq)]
pub enum ProofFolderError {
    /// The folder is not in draft status.
    NotDraft,
    /// The folder has already been submitted.
    AlreadySubmitted,
    /// The folder was not found.
    NotFound,
    /// The artifact was not found in the folder.
    ArtifactNotFound,
}

impl std::fmt::Display for ProofFolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProofFolderError::NotDraft => write!(f, "Proof folder is not in draft status"),
            ProofFolderError::AlreadySubmitted => write!(f, "Proof folder has already been submitted"),
            ProofFolderError::NotFound => write!(f, "Proof folder not found"),
            ProofFolderError::ArtifactNotFound => write!(f, "Artifact not found in proof folder"),
        }
    }
}

impl std::error::Error for ProofFolderError {}

/// Document schema for storing proof folders in a realm.
///
/// This is used with `realm.document::<ProofFolderDocument>("proof_folders")` to get
/// a CRDT-synchronized proof folder collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProofFolderDocument {
    /// All proof folders in this realm.
    pub folders: Vec<ProofFolder>,
}

impl ProofFolderDocument {
    /// Create a new empty proof folder document.
    pub fn new() -> Self {
        Self { folders: Vec::new() }
    }

    /// Add a proof folder to the document.
    pub fn add(&mut self, folder: ProofFolder) {
        self.folders.push(folder);
    }

    /// Find a proof folder by ID.
    pub fn find(&self, id: &ProofFolderId) -> Option<&ProofFolder> {
        self.folders.iter().find(|f| &f.id == id)
    }

    /// Find a proof folder by ID (mutable).
    pub fn find_mut(&mut self, id: &ProofFolderId) -> Option<&mut ProofFolder> {
        self.folders.iter_mut().find(|f| &f.id == id)
    }

    /// Get all proof folders for a specific quest.
    pub fn folders_for_quest(&self, quest_id: &QuestId) -> Vec<&ProofFolder> {
        self.folders.iter().filter(|f| &f.quest_id == quest_id).collect()
    }

    /// Get all proof folders by a specific claimant.
    pub fn folders_by_claimant(&self, claimant: &MemberId) -> Vec<&ProofFolder> {
        self.folders.iter().filter(|f| &f.claimant == claimant).collect()
    }

    /// Get all draft folders.
    pub fn draft_folders(&self) -> Vec<&ProofFolder> {
        self.folders.iter().filter(|f| f.is_draft()).collect()
    }

    /// Get all submitted folders.
    pub fn submitted_folders(&self) -> Vec<&ProofFolder> {
        self.folders.iter().filter(|f| f.is_submitted()).collect()
    }

    /// Get draft folders for a specific quest.
    pub fn draft_folders_for_quest(&self, quest_id: &QuestId) -> Vec<&ProofFolder> {
        self.folders
            .iter()
            .filter(|f| &f.quest_id == quest_id && f.is_draft())
            .collect()
    }

    /// Get submitted folders for a specific quest.
    pub fn submitted_folders_for_quest(&self, quest_id: &QuestId) -> Vec<&ProofFolder> {
        self.folders
            .iter()
            .filter(|f| &f.quest_id == quest_id && f.is_submitted())
            .collect()
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

    fn test_quest_id() -> QuestId {
        [1u8; 16]
    }

    fn another_quest_id() -> QuestId {
        [2u8; 16]
    }

    fn test_artifact_id() -> ArtifactId {
        [42u8; 32]
    }

    fn another_artifact_id() -> ArtifactId {
        [43u8; 32]
    }

    #[test]
    fn test_proof_folder_creation() {
        let folder = ProofFolder::new(test_quest_id(), test_member_id());
        assert!(folder.is_draft());
        assert!(!folder.is_submitted());
        assert!(folder.narrative.is_empty());
        assert_eq!(folder.artifact_count(), 0);
    }

    #[test]
    fn test_proof_folder_set_narrative() {
        let mut folder = ProofFolder::new(test_quest_id(), test_member_id());
        assert!(folder.set_narrative("# My Work\n\nI did the thing.").is_ok());
        assert_eq!(folder.narrative, "# My Work\n\nI did the thing.");
    }

    #[test]
    fn test_proof_folder_add_artifact() {
        let mut folder = ProofFolder::new(test_quest_id(), test_member_id());
        let artifact = ProofFolderArtifact::new(
            test_artifact_id(),
            "photo.jpg",
            1024,
            Some("image/jpeg".into()),
        );
        assert!(folder.add_artifact(artifact).is_ok());
        assert_eq!(folder.artifact_count(), 1);
    }

    #[test]
    fn test_proof_folder_remove_artifact() {
        let mut folder = ProofFolder::new(test_quest_id(), test_member_id());
        let artifact = ProofFolderArtifact::new(
            test_artifact_id(),
            "photo.jpg",
            1024,
            Some("image/jpeg".into()),
        );
        folder.add_artifact(artifact).unwrap();

        assert!(folder.remove_artifact(&test_artifact_id()).is_ok());
        assert_eq!(folder.artifact_count(), 0);

        // Can't remove non-existent artifact
        assert_eq!(
            folder.remove_artifact(&test_artifact_id()),
            Err(ProofFolderError::ArtifactNotFound)
        );
    }

    #[test]
    fn test_proof_folder_submit() {
        let mut folder = ProofFolder::new(test_quest_id(), test_member_id());
        folder.set_narrative("Did the work").unwrap();

        assert!(folder.submit().is_ok());
        assert!(folder.is_submitted());
        assert!(folder.submitted_at_millis.is_some());

        // Can't submit twice
        assert_eq!(folder.submit(), Err(ProofFolderError::AlreadySubmitted));
    }

    #[test]
    fn test_proof_folder_draft_only_operations() {
        let mut folder = ProofFolder::new(test_quest_id(), test_member_id());
        folder.submit().unwrap();

        // Can't modify after submission
        assert_eq!(
            folder.set_narrative("New narrative"),
            Err(ProofFolderError::NotDraft)
        );

        let artifact = ProofFolderArtifact::new(
            test_artifact_id(),
            "photo.jpg",
            1024,
            None,
        );
        assert_eq!(folder.add_artifact(artifact), Err(ProofFolderError::NotDraft));

        assert_eq!(
            folder.remove_artifact(&test_artifact_id()),
            Err(ProofFolderError::NotDraft)
        );
    }

    #[test]
    fn test_proof_folder_narrative_preview() {
        let mut folder = ProofFolder::new(test_quest_id(), test_member_id());

        // Short narrative
        folder.set_narrative("Short text").unwrap();
        assert_eq!(folder.narrative_preview(), "Short text");

        // Long narrative
        let long_text = "a".repeat(150);
        folder.set_narrative(&long_text).unwrap();
        let preview = folder.narrative_preview();
        assert!(preview.ends_with("..."));
        assert!(preview.len() <= 103); // 100 chars + "..."
    }

    #[test]
    fn test_proof_folder_artifact_with_caption() {
        let artifact = ProofFolderArtifact::with_caption(
            test_artifact_id(),
            "photo.jpg",
            1024,
            Some("image/jpeg".into()),
            "Before starting the work",
        );
        assert_eq!(artifact.caption, Some("Before starting the work".into()));
    }

    #[test]
    fn test_proof_folder_document() {
        let mut doc = ProofFolderDocument::new();

        let mut folder1 = ProofFolder::new(test_quest_id(), test_member_id());
        let id1 = folder1.id;
        folder1.submit().unwrap();

        let folder2 = ProofFolder::new(test_quest_id(), another_member_id());
        let id2 = folder2.id;

        let folder3 = ProofFolder::new(another_quest_id(), test_member_id());

        doc.add(folder1);
        doc.add(folder2);
        doc.add(folder3);

        // Find by ID
        assert!(doc.find(&id1).is_some());
        assert!(doc.find(&id2).is_some());

        // Folders for quest
        assert_eq!(doc.folders_for_quest(&test_quest_id()).len(), 2);
        assert_eq!(doc.folders_for_quest(&another_quest_id()).len(), 1);

        // Folders by claimant
        assert_eq!(doc.folders_by_claimant(&test_member_id()).len(), 2);
        assert_eq!(doc.folders_by_claimant(&another_member_id()).len(), 1);

        // Draft vs submitted
        assert_eq!(doc.draft_folders().len(), 2);
        assert_eq!(doc.submitted_folders().len(), 1);

        // Draft folders for quest
        assert_eq!(doc.draft_folders_for_quest(&test_quest_id()).len(), 1);
        assert_eq!(doc.submitted_folders_for_quest(&test_quest_id()).len(), 1);
    }

    #[test]
    fn test_generate_proof_folder_id_uniqueness() {
        let id1 = generate_proof_folder_id();
        let id2 = generate_proof_folder_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_proof_folder_serialization() {
        let mut folder = ProofFolder::new(test_quest_id(), test_member_id());
        folder.set_narrative("Test narrative").unwrap();
        let artifact = ProofFolderArtifact::new(
            test_artifact_id(),
            "test.jpg",
            1024,
            Some("image/jpeg".into()),
        );
        folder.add_artifact(artifact).unwrap();

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&folder).unwrap();
        let deserialized: ProofFolder = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(folder.id, deserialized.id);
        assert_eq!(folder.narrative, deserialized.narrative);
        assert_eq!(folder.artifact_count(), deserialized.artifact_count());
    }

    #[test]
    fn test_proof_folder_document_serialization() {
        let mut doc = ProofFolderDocument::new();
        doc.add(ProofFolder::new(test_quest_id(), test_member_id()));

        let bytes = postcard::to_allocvec(&doc).unwrap();
        let deserialized: ProofFolderDocument = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(doc.folders.len(), deserialized.folders.len());
    }
}
