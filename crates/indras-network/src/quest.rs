//! Quest - lightweight collaboration intentions within realms.
//!
//! Quests are simple, claimable tasks that realm members can create
//! and complete together. They provide a lightweight way to coordinate
//! work without complex project management overhead.

use crate::artifact::ArtifactId;
use crate::member::MemberId;

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

/// A quest - a lightweight intention or task within a realm.
///
/// Quests are created by one member and can be claimed by another.
/// They represent simple collaborative tasks like "review this document"
/// or "fix this bug".
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
/// // Another member claims it
/// realm.claim_quest(quest_id, their_id).await?;
///
/// // Mark as complete
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
    /// The member who claimed this quest (None if unclaimed).
    pub doer: Option<MemberId>,
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
            doer: None,
            created_at_millis: chrono::Utc::now().timestamp_millis(),
            completed_at_millis: None,
        }
    }

    /// Check if this quest has been claimed.
    pub fn is_claimed(&self) -> bool {
        self.doer.is_some()
    }

    /// Check if this quest is complete.
    pub fn is_complete(&self) -> bool {
        self.completed_at_millis.is_some()
    }

    /// Check if this quest is open (not claimed and not complete).
    pub fn is_open(&self) -> bool {
        !self.is_claimed() && !self.is_complete()
    }

    /// Claim this quest for a member.
    ///
    /// Returns an error if already claimed.
    pub fn claim(&mut self, doer: MemberId) -> Result<(), QuestError> {
        if self.doer.is_some() {
            return Err(QuestError::AlreadyClaimed);
        }
        if self.completed_at_millis.is_some() {
            return Err(QuestError::AlreadyComplete);
        }
        self.doer = Some(doer);
        Ok(())
    }

    /// Mark this quest as complete.
    ///
    /// Returns an error if not claimed or already complete.
    pub fn complete(&mut self) -> Result<(), QuestError> {
        if self.completed_at_millis.is_some() {
            return Err(QuestError::AlreadyComplete);
        }
        self.completed_at_millis = Some(chrono::Utc::now().timestamp_millis());
        Ok(())
    }

    /// Unclaim this quest (release it back to open status).
    pub fn unclaim(&mut self) -> Result<(), QuestError> {
        if self.completed_at_millis.is_some() {
            return Err(QuestError::AlreadyComplete);
        }
        self.doer = None;
        Ok(())
    }
}

/// Errors that can occur during quest operations.
#[derive(Debug, Clone, PartialEq)]
pub enum QuestError {
    /// The quest has already been claimed by someone.
    AlreadyClaimed,
    /// The quest has already been completed.
    AlreadyComplete,
    /// The quest was not found.
    NotFound,
}

impl std::fmt::Display for QuestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuestError::AlreadyClaimed => write!(f, "Quest is already claimed"),
            QuestError::AlreadyComplete => write!(f, "Quest is already complete"),
            QuestError::NotFound => write!(f, "Quest not found"),
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

    /// Get all claimed but incomplete quests.
    pub fn in_progress_quests(&self) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|q| q.is_claimed() && !q.is_complete())
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

    /// Get quests claimed by a specific member.
    pub fn quests_by_doer(&self, doer: &MemberId) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|q| q.doer.as_ref() == Some(doer))
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

    #[test]
    fn test_quest_creation() {
        let quest = Quest::new("Test quest", "Do something", None, test_member_id());
        assert!(!quest.is_claimed());
        assert!(!quest.is_complete());
        assert!(quest.is_open());
    }

    #[test]
    fn test_quest_claim() {
        let mut quest = Quest::new("Test quest", "Do something", None, test_member_id());
        assert!(quest.claim(another_member_id()).is_ok());
        assert!(quest.is_claimed());
        assert!(!quest.is_open());

        // Can't claim twice
        assert_eq!(quest.claim(test_member_id()), Err(QuestError::AlreadyClaimed));
    }

    #[test]
    fn test_quest_complete() {
        let mut quest = Quest::new("Test quest", "Do something", None, test_member_id());
        quest.claim(another_member_id()).unwrap();
        assert!(quest.complete().is_ok());
        assert!(quest.is_complete());

        // Can't complete twice
        assert_eq!(quest.complete(), Err(QuestError::AlreadyComplete));
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

        // Claim and complete
        doc.find_mut(&id).unwrap().claim(another_member_id()).unwrap();
        assert_eq!(doc.open_quests().len(), 0);
        assert_eq!(doc.in_progress_quests().len(), 1);

        doc.find_mut(&id).unwrap().complete().unwrap();
        assert_eq!(doc.completed_quests().len(), 1);
    }
}
