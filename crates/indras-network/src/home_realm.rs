//! Home Realm - personal realm for each user with multi-device sync.
//!
//! The HomeRealm is a special personal realm that:
//! - Is deterministically derived from the user's member ID
//! - Supports multi-device sync (same user can access from multiple devices)
//! - Is eagerly created when the network initializes
//! - Contains personal documents like notes and quests
//!
//! Unlike shared realms, the home realm is unique to each user and doesn't
//! require invite codes to access from other devices.

use crate::access::AccessMode;
use crate::artifact::{Artifact, ArtifactId};
use crate::artifact_index::{ArtifactIndex, HomeArtifactEntry};
use crate::document::Document;
use crate::error::{IndraError, Result};
use crate::member::{Member, MemberId};
use crate::network::RealmId;
use crate::note::{Note, NoteDocument, NoteId};
use crate::quest::{Quest, QuestDocument, QuestId};

use indras_core::{InterfaceId, PeerIdentity};
use indras_node::IndrasNode;
use indras_storage::ContentRef;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::debug;

/// Generate a deterministic home realm ID from a member ID.
///
/// The same member_id always produces the same realm_id, enabling
/// multi-device sync - all devices belonging to the same user will
/// access the same home realm.
///
/// # Example
///
/// ```ignore
/// let realm_id = home_realm_id(my_member_id);
/// // This realm ID will be the same on all my devices
/// ```
pub fn home_realm_id(member_id: MemberId) -> RealmId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"home-realm-v1:");
    hasher.update(&member_id);
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Metadata for artifacts stored in the home realm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeArtifactMetadata {
    /// Human-readable name for the artifact.
    pub name: String,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Size in bytes.
    pub size: u64,
}

/// Type alias for backward compatibility.
pub type LegacyHomeArtifactMetadata = HomeArtifactMetadata;

/// A wrapper around the home realm providing personal document management.
///
/// The HomeRealm is a personal realm unique to each user, containing:
/// - Personal quests and tasks
/// - Notes and documents
/// - Stored artifacts (images, files, etc.)
///
/// # Example
///
/// ```ignore
/// // Get the home realm (eagerly created on network init)
/// let home = network.home_realm().await?;
///
/// // Create a personal note
/// let notes = home.notes().await?;
/// notes.update(|doc| {
///     doc.create_note("My Note", "# Hello\n\nContent here", my_id, vec![]);
/// }).await?;
///
/// // Create a personal quest
/// let quest_id = home.create_quest("Personal Task", "Do something", None).await?;
/// ```
pub struct HomeRealm {
    /// The realm ID (deterministically derived from member ID).
    id: RealmId,
    /// Reference to the underlying node.
    node: Arc<IndrasNode>,
    /// Our own member ID.
    self_id: MemberId,
}

impl HomeRealm {
    /// Create a new HomeRealm wrapper.
    ///
    /// This is called internally by IndrasNetwork during initialization.
    pub(crate) async fn new(
        id: RealmId,
        node: Arc<IndrasNode>,
        self_id: MemberId,
    ) -> Result<Self> {
        Ok(Self { id, node, self_id })
    }

    /// Get the home realm ID.
    pub fn id(&self) -> RealmId {
        self.id
    }

    /// Get our member ID.
    pub fn member_id(&self) -> MemberId {
        self.self_id
    }

    // ============================================================
    // Documents
    // ============================================================

    /// Get the quests document for this home realm.
    ///
    /// Personal quests are private and not shared with others.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let quests = home.quests().await?;
    /// let open = quests.read().await.open_quests();
    /// println!("Open personal quests: {}", open.len());
    /// ```
    pub async fn quests(&self) -> Result<Document<QuestDocument>> {
        Document::new(self.id, "quests".to_string(), Arc::clone(&self.node)).await
    }

    /// Get the notes document for this home realm.
    ///
    /// Personal notes with markdown content.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let notes = home.notes().await?;
    /// notes.update(|doc| {
    ///     doc.create_note("Meeting Notes", "# Project Update\n\n- Item 1", my_id, vec!["work".into()]);
    /// }).await?;
    /// ```
    pub async fn notes(&self) -> Result<Document<NoteDocument>> {
        Document::new(self.id, "notes".to_string(), Arc::clone(&self.node)).await
    }

    // ============================================================
    // Quest convenience methods
    // ============================================================

    /// Create a new personal quest.
    ///
    /// # Arguments
    ///
    /// * `title` - Short title describing the quest
    /// * `description` - Detailed description
    /// * `image` - Optional artifact ID for an image
    ///
    /// # Example
    ///
    /// ```ignore
    /// let quest_id = home.create_quest(
    ///     "Read book",
    ///     "Finish reading 'The Pragmatic Programmer'",
    ///     None,
    /// ).await?;
    /// ```
    pub async fn create_quest(
        &self,
        title: impl Into<String>,
        description: impl Into<String>,
        image: Option<ArtifactId>,
    ) -> Result<QuestId> {
        let quest = Quest::new(title, description, image, self.self_id);
        let quest_id = quest.id;

        let doc = self.quests().await?;
        doc.update(|d| {
            d.add(quest);
        })
        .await?;

        Ok(quest_id)
    }

    /// Complete a personal quest.
    pub async fn complete_quest(&self, quest_id: QuestId) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                let _ = quest.complete();
            }
        })
        .await?;

        Ok(())
    }

    // ============================================================
    // Note convenience methods
    // ============================================================

    /// Create a new personal note.
    ///
    /// # Arguments
    ///
    /// * `title` - Title of the note
    /// * `content` - Markdown content
    /// * `tags` - Optional tags for organization
    ///
    /// # Example
    ///
    /// ```ignore
    /// let note_id = home.create_note(
    ///     "Meeting Notes",
    ///     "# Project Update\n\n- Item 1\n- Item 2",
    ///     vec!["work".into(), "meeting".into()],
    /// ).await?;
    /// ```
    pub async fn create_note(
        &self,
        title: impl Into<String>,
        content: impl Into<String>,
        tags: Vec<String>,
    ) -> Result<NoteId> {
        let note = Note::with_tags(title, content, self.self_id, tags);
        let note_id = note.id;

        let doc = self.notes().await?;
        doc.update(|d| {
            d.add(note);
        })
        .await?;

        Ok(note_id)
    }

    /// Update an existing note's content.
    pub async fn update_note(
        &self,
        note_id: NoteId,
        content: impl Into<String>,
    ) -> Result<()> {
        let content = content.into();
        let doc = self.notes().await?;
        doc.update(|d| {
            if let Some(note) = d.find_mut(&note_id) {
                note.update_content(content);
            }
        })
        .await?;

        Ok(())
    }

    /// Delete a note.
    pub async fn delete_note(&self, note_id: NoteId) -> Result<Option<Note>> {
        let mut removed = None;
        let doc = self.notes().await?;
        doc.update(|d| {
            removed = d.remove(&note_id);
        })
        .await?;

        Ok(removed)
    }

    // ============================================================
    // Artifacts
    // ============================================================

    /// Share an artifact (file) in the home realm.
    ///
    /// This stores the artifact data in blob storage and returns
    /// an artifact ID that can be used to retrieve it later.
    ///
    /// # Arguments
    ///
    /// * `data` - The raw file data
    /// * `metadata` - Metadata about the artifact
    ///
    /// # Example
    ///
    /// ```ignore
    /// let image_data = std::fs::read("photo.png")?;
    /// let artifact_id = home.share_artifact(
    ///     image_data,
    ///     HomeArtifactMetadata {
    ///         name: "photo.png".to_string(),
    ///         mime_type: Some("image/png".to_string()),
    ///         size: 12345,
    ///     },
    /// ).await?;
    /// ```
    pub async fn share_artifact(
        &self,
        data: Vec<u8>,
        metadata: HomeArtifactMetadata,
    ) -> Result<ArtifactId> {
        // Compute BLAKE3 hash for artifact ID
        let hash = blake3::hash(&data);
        let id: ArtifactId = *hash.as_bytes();

        // Store in blob storage
        let _content_ref = self
            .node
            .storage()
            .store_blob(&data)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to store blob: {}", e)))?;

        debug!(
            artifact_id = %hex::encode(&id[..8]),
            name = %metadata.name,
            size = metadata.size,
            "Stored artifact in home realm"
        );

        Ok(id)
    }

    /// Share a file from the filesystem.
    ///
    /// Convenience method that reads the file and shares it.
    pub async fn share_file(&self, path: impl AsRef<Path>) -> Result<Artifact> {
        let path = path.as_ref();

        // Read the file
        let file_data = tokio::fs::read(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file: {}", e)))?;

        // Get filename
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Get file size
        let size = file_data.len() as u64;

        // Guess MIME type from extension
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(guess_mime_type);

        // Share the artifact
        let id = self
            .share_artifact(
                file_data,
                HomeArtifactMetadata {
                    name: name.clone(),
                    mime_type: mime_type.clone(),
                    size,
                },
            )
            .await?;

        Ok(Artifact {
            id,
            name,
            size,
            mime_type,
            sharer: Member::new(*self.node.identity()),
            owner: self.node.identity().as_bytes().try_into().expect("identity bytes"),
            shared_at: chrono::Utc::now(),
            is_encrypted: false,
            sharing_status: crate::artifact_sharing::SharingStatus::Shared,
        })
    }

    /// Get an artifact by ID.
    ///
    /// Retrieves the raw data for a previously shared artifact.
    pub async fn get_artifact(&self, id: &ArtifactId) -> Result<Vec<u8>> {
        // Create a content reference (we don't know the exact size, use 0 as placeholder)
        let content_ref = ContentRef::new(*id, 0);

        let data = self
            .node
            .storage()
            .resolve_blob(&content_ref)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to fetch artifact: {}", e)))?;

        Ok(data.to_vec())
    }

    // ============================================================
    // Shared Filesystem (ArtifactIndex)
    // ============================================================

    /// Get the artifact index document for this home realm.
    ///
    /// The artifact index is the source of truth for all artifacts
    /// owned by this user.
    pub async fn artifact_index(&self) -> Result<Document<ArtifactIndex>> {
        Document::new(self.id, "artifacts".to_string(), Arc::clone(&self.node)).await
    }

    /// Upload an artifact to the home realm filesystem.
    ///
    /// Reads the file, computes its BLAKE3 hash, stores the blob,
    /// and adds an entry to the ArtifactIndex with no grants.
    /// Idempotent: if the hash already exists, returns the existing entry ID.
    pub async fn upload(&self, path: impl AsRef<Path>) -> Result<ArtifactId> {
        let path = path.as_ref();

        // Read the file
        let file_data = tokio::fs::read(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file: {}", e)))?;

        // Compute BLAKE3 hash
        let hash = blake3::hash(&file_data);
        let id: ArtifactId = *hash.as_bytes();

        // Store blob
        let _content_ref = self
            .node
            .storage()
            .store_blob(&file_data)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to store blob: {}", e)))?;

        // Get filename and metadata
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let size = file_data.len() as u64;
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(guess_mime_type);

        // Add to artifact index (idempotent)
        let doc = self.artifact_index().await?;
        doc.update(|index| {
            let entry = HomeArtifactEntry {
                id,
                name,
                mime_type,
                size,
                created_at: 0, // Will be set by caller or default tick
                encrypted_key: None,
                status: crate::access::ArtifactStatus::Active,
                grants: Vec::new(),
                provenance: None,
            };
            index.store(entry);
        })
        .await?;

        debug!(
            artifact_id = %hex::encode(&id[..8]),
            "Uploaded artifact to home realm filesystem"
        );

        Ok(id)
    }

    /// Grant access to an artifact for a specific member.
    pub async fn grant_access(
        &self,
        id: &ArtifactId,
        grantee: MemberId,
        mode: AccessMode,
    ) -> Result<()> {
        let doc = self.artifact_index().await?;
        let self_id = self.self_id;
        let mut result = Ok(());
        doc.update(|index| {
            if let Err(e) = index.grant(id, grantee, mode.clone(), self_id, 0) {
                result = Err(IndraError::Artifact(format!("Grant failed: {}", e)));
            }
        })
        .await?;
        result
    }

    /// Revoke a member's access to an artifact.
    pub async fn revoke_access(
        &self,
        id: &ArtifactId,
        grantee: &MemberId,
    ) -> Result<()> {
        let doc = self.artifact_index().await?;
        let grantee = *grantee;
        let mut result = Ok(());
        doc.update(|index| {
            if let Err(e) = index.revoke_access(id, &grantee) {
                result = Err(IndraError::Artifact(format!("Revoke failed: {}", e)));
            }
        })
        .await?;
        result
    }

    /// Recall an artifact — remove all revocable/timed grants, keep permanent.
    pub async fn recall(&self, id: &ArtifactId) -> Result<bool> {
        let doc = self.artifact_index().await?;
        let mut recalled = false;
        doc.update(|index| {
            recalled = index.recall(id, 0);
        })
        .await?;

        if recalled {
            // TODO: Delete the blob locally once storage API supports delete_blob
            // let content_ref = indras_storage::ContentRef::new(*id, 0);
            // let _ = self.node.storage().delete_blob(&content_ref).await;
        }

        Ok(recalled)
    }

    /// Transfer ownership of an artifact to another member.
    ///
    /// Returns the entry for the recipient to add to their index.
    pub async fn transfer(
        &self,
        id: &ArtifactId,
        to: MemberId,
    ) -> Result<HomeArtifactEntry> {
        let doc = self.artifact_index().await?;
        let self_id = self.self_id;
        let mut transfer_result: std::result::Result<HomeArtifactEntry, crate::access::TransferError> = Err(crate::access::TransferError::NotFound);
        doc.update(|index| {
            transfer_result = index.transfer(id, to, self_id, 0);
        })
        .await?;

        transfer_result.map_err(|e| IndraError::Artifact(format!("Transfer failed: {}", e)))
    }

    /// Get all artifacts shared with a specific member.
    pub async fn shared_with(&self, member: &MemberId) -> Result<Vec<HomeArtifactEntry>> {
        let doc = self.artifact_index().await?;
        let data = doc.read().await;
        Ok(data.accessible_by(member, 0).into_iter().cloned().collect())
    }

    // ============================================================
    // Escape hatches
    // ============================================================

    /// Access the underlying node.
    pub fn node(&self) -> &IndrasNode {
        &self.node
    }
}

impl Clone for HomeRealm {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            node: Arc::clone(&self.node),
            self_id: self.self_id,
        }
    }
}

impl std::fmt::Debug for HomeRealm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HomeRealm")
            .field("id", &hex::encode(&self.id.as_bytes()[..8]))
            .field("member_id", &hex::encode(&self.self_id[..8]))
            .finish()
    }
}

/// Guess MIME type from file extension.
fn guess_mime_type(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        // Documents
        "pdf" => "application/pdf",
        "md" => "text/markdown",
        "txt" => "text/plain",
        // Default
        _ => "application/octet-stream",
    }
    .to_string()
}

// Simple hex encoding for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
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
    fn test_home_realm_id_deterministic() {
        // Same member_id should always produce the same realm_id
        let id1 = home_realm_id(test_member_id());
        let id2 = home_realm_id(test_member_id());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_home_realm_id_unique() {
        // Different member_ids should produce different realm_ids
        let id1 = home_realm_id(test_member_id());
        let id2 = home_realm_id(another_member_id());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_home_realm_id_prefix() {
        // Verify the hash is based on the expected prefix
        let member_id = test_member_id();
        let expected_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"home-realm-v1:");
            hasher.update(&member_id);
            hasher.finalize()
        };

        let realm_id = home_realm_id(member_id);
        assert_eq!(realm_id.as_bytes(), expected_hash.as_bytes());
    }

    #[test]
    fn test_artifact_metadata_serialization() {
        let metadata = HomeArtifactMetadata {
            name: "test.png".to_string(),
            mime_type: Some("image/png".to_string()),
            size: 12345,
        };

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&metadata).unwrap();
        let deserialized: HomeArtifactMetadata = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(metadata.name, deserialized.name);
        assert_eq!(metadata.mime_type, deserialized.mime_type);
        assert_eq!(metadata.size, deserialized.size);
    }
}
