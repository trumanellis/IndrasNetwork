//! CRDT-backed editable chat messages with version history.
//!
//! This module provides editable chat messages where users can edit their own
//! messages with full version history preserved. Messages can be edited or
//! deleted at any time, with edit history accessible via the versions field.

use serde::{Deserialize, Serialize};

/// Unique message identifier (realm_id + tick + member_id or UUID).
pub type ChatMessageId = String;

/// A single version of a chat message.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatMessageVersion {
    /// The content at this version.
    pub content: String,
    /// Tick timestamp when this version was created.
    pub edited_at: u64,
}

/// Types of editable chat messages.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum EditableMessageType {
    /// Regular text message.
    Text,
    /// Proof artifact submitted for a quest.
    ProofSubmitted {
        quest_id: String,
        artifact_id: String,
    },
    /// Proof folder submitted for a quest.
    ProofFolderSubmitted {
        quest_id: String,
        folder_id: String,
    },
    /// Blessing given to a proof.
    BlessingGiven {
        quest_id: String,
        claimant: String,
    },
    /// Artifact was recalled/unshared (tombstone).
    ///
    /// This type is used to mark where a shared artifact used to be
    /// in the chat history. The content shows minimal metadata.
    ArtifactRecalled {
        /// Hash of the recalled artifact (hex string).
        artifact_hash: String,
        /// When the artifact was originally shared (tick).
        shared_at: u64,
    },
    /// Image shared inline in chat.
    ///
    /// For small images (< 2MB), data is embedded as base64.
    /// For large images, artifact_hash references stored content.
    Image {
        /// MIME type (image/png, image/jpeg, etc.)
        mime_type: String,
        /// For small images: base64-encoded image data.
        /// For large images: None (fetch from artifact).
        inline_data: Option<String>,
        /// For large images: hash reference to artifact storage (hex string).
        /// For small images: None.
        artifact_hash: Option<String>,
        /// Original filename.
        filename: Option<String>,
        /// Image dimensions (width, height) if known.
        dimensions: Option<(u32, u32)>,
        /// Alt text / caption.
        alt_text: Option<String>,
    },
    /// Gallery of images/videos/files shared from a folder.
    Gallery {
        /// Unique folder identifier.
        folder_id: String,
        /// Gallery title/name.
        title: Option<String>,
        /// Items in the gallery.
        items: Vec<GalleryItem>,
    },
}

/// An item in a gallery (image, video, or file).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GalleryItem {
    /// Filename.
    pub name: String,
    /// MIME type (image/png, video/mp4, etc.)
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// Base64-encoded thumbnail (for images/videos).
    pub thumbnail_data: Option<String>,
    /// Hash reference to artifact storage (hex string).
    pub artifact_hash: String,
    /// Item dimensions (width, height) if applicable.
    pub dimensions: Option<(u32, u32)>,
}

impl Default for EditableMessageType {
    fn default() -> Self {
        Self::Text
    }
}

/// A CRDT-backed chat message with version history.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EditableChatMessage {
    /// Unique message identifier.
    pub id: ChatMessageId,
    /// Realm this message belongs to.
    pub realm_id: String,
    /// Member ID of the author (only this user can edit).
    pub author: String,
    /// Original tick when the message was created.
    pub created_at: u64,
    /// Latest content (or empty if deleted).
    pub current_content: String,
    /// Version history, ordered oldest to newest.
    pub versions: Vec<ChatMessageVersion>,
    /// Whether this message has been deleted.
    pub is_deleted: bool,
    /// Type of message (text, proof, blessing, etc.).
    pub message_type: EditableMessageType,
}

impl EditableChatMessage {
    /// Create a new editable chat message.
    pub fn new(
        id: ChatMessageId,
        realm_id: String,
        author: String,
        content: String,
        created_at: u64,
        message_type: EditableMessageType,
    ) -> Self {
        Self {
            id,
            realm_id,
            author,
            created_at,
            current_content: content,
            versions: Vec::new(),
            is_deleted: false,
            message_type,
        }
    }

    /// Create a new text message.
    pub fn new_text(
        id: ChatMessageId,
        realm_id: String,
        author: String,
        content: String,
        created_at: u64,
    ) -> Self {
        Self::new(
            id,
            realm_id,
            author,
            content,
            created_at,
            EditableMessageType::Text,
        )
    }

    /// Create an artifact recalled tombstone message.
    ///
    /// This is used to mark where a shared artifact used to be in chat.
    /// The content is a human-readable tombstone message.
    pub fn new_artifact_recalled(
        id: ChatMessageId,
        realm_id: String,
        sharer: String,
        artifact_hash: String,
        shared_at: u64,
        recalled_at: u64,
    ) -> Self {
        let content = format!("[Recalled artifact shared at tick {}]", shared_at);
        Self::new(
            id,
            realm_id,
            sharer,
            content,
            recalled_at,
            EditableMessageType::ArtifactRecalled {
                artifact_hash,
                shared_at,
            },
        )
    }

    /// Check if this is an artifact recalled tombstone.
    pub fn is_artifact_recalled(&self) -> bool {
        matches!(self.message_type, EditableMessageType::ArtifactRecalled { .. })
    }

    /// Create a new inline image message.
    ///
    /// For small images, pass the base64-encoded data in `inline_data`.
    /// For large images stored as artifacts, pass the artifact hash.
    pub fn new_image(
        id: ChatMessageId,
        realm_id: String,
        author: String,
        created_at: u64,
        mime_type: String,
        inline_data: Option<String>,
        artifact_hash: Option<String>,
        filename: Option<String>,
        dimensions: Option<(u32, u32)>,
        alt_text: Option<String>,
    ) -> Self {
        let content = alt_text.clone().unwrap_or_else(|| "[Image]".to_string());
        Self::new(
            id,
            realm_id,
            author,
            content,
            created_at,
            EditableMessageType::Image {
                mime_type,
                inline_data,
                artifact_hash,
                filename,
                dimensions,
                alt_text,
            },
        )
    }

    /// Create a new gallery message.
    pub fn new_gallery(
        id: ChatMessageId,
        realm_id: String,
        author: String,
        created_at: u64,
        folder_id: String,
        title: Option<String>,
        items: Vec<GalleryItem>,
    ) -> Self {
        let content = title.clone().unwrap_or_else(|| format!("[Gallery: {} items]", items.len()));
        Self::new(
            id,
            realm_id,
            author,
            content,
            created_at,
            EditableMessageType::Gallery {
                folder_id,
                title,
                items,
            },
        )
    }

    /// Check if this is an inline image message.
    pub fn is_image(&self) -> bool {
        matches!(self.message_type, EditableMessageType::Image { .. })
    }

    /// Check if this is a gallery message.
    pub fn is_gallery(&self) -> bool {
        matches!(self.message_type, EditableMessageType::Gallery { .. })
    }

    /// Get image data URL if this is an embedded image.
    pub fn image_data_url(&self) -> Option<String> {
        match &self.message_type {
            EditableMessageType::Image { mime_type, inline_data: Some(data), .. } => {
                Some(format!("data:{};base64,{}", mime_type, data))
            }
            _ => None,
        }
    }

    /// Edit the message content.
    ///
    /// Saves the current version to history before updating content.
    /// Returns true if the edit was applied, false if nothing changed.
    pub fn edit(&mut self, new_content: String, tick: u64) -> bool {
        if self.is_deleted || self.current_content == new_content {
            return false;
        }

        // Save current version to history
        self.versions.push(ChatMessageVersion {
            content: self.current_content.clone(),
            edited_at: tick,
        });

        self.current_content = new_content;
        true
    }

    /// Delete the message (soft delete with history preserved).
    ///
    /// Saves the current version to history and marks as deleted.
    /// Returns true if the delete was applied, false if already deleted.
    pub fn delete(&mut self, tick: u64) -> bool {
        if self.is_deleted {
            return false;
        }

        // Save current version to history
        self.versions.push(ChatMessageVersion {
            content: self.current_content.clone(),
            edited_at: tick,
        });

        self.current_content = String::new();
        self.is_deleted = true;
        true
    }

    /// Check if this message has been edited.
    pub fn is_edited(&self) -> bool {
        !self.versions.is_empty()
    }

    /// Get total version count (current + history).
    pub fn version_count(&self) -> usize {
        self.versions.len() + 1
    }

    /// Check if the given member can edit this message.
    pub fn can_edit(&self, member_id: &str) -> bool {
        self.author == member_id && !self.is_deleted
    }

    /// Get all versions including current, ordered oldest to newest.
    pub fn all_versions(&self) -> Vec<ChatMessageVersion> {
        let mut all = self.versions.clone();
        if !self.is_deleted {
            all.push(ChatMessageVersion {
                content: self.current_content.clone(),
                edited_at: self.created_at, // Current version's "edited_at" is approximated
            });
        }
        all
    }
}

/// The realm chat document containing all messages.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RealmChatDocument {
    /// All messages in the realm, ordered by creation time.
    pub messages: Vec<EditableChatMessage>,
}

impl RealmChatDocument {
    /// Create a new empty chat document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new message to the chat.
    pub fn add_message(&mut self, message: EditableChatMessage) {
        self.messages.push(message);
    }

    /// Find a message by ID.
    pub fn get_message(&self, id: &str) -> Option<&EditableChatMessage> {
        self.messages.iter().find(|m| m.id == id)
    }

    /// Find a message by ID (mutable).
    pub fn get_message_mut(&mut self, id: &str) -> Option<&mut EditableChatMessage> {
        self.messages.iter_mut().find(|m| m.id == id)
    }

    /// Edit a message if the member is the author.
    ///
    /// Returns true if the edit was applied.
    pub fn edit_message(&mut self, id: &str, member_id: &str, new_content: String, tick: u64) -> bool {
        if let Some(msg) = self.get_message_mut(id) {
            if msg.can_edit(member_id) {
                return msg.edit(new_content, tick);
            }
        }
        false
    }

    /// Delete a message if the member is the author.
    ///
    /// Returns true if the delete was applied.
    pub fn delete_message(&mut self, id: &str, member_id: &str, tick: u64) -> bool {
        if let Some(msg) = self.get_message_mut(id) {
            if msg.can_edit(member_id) {
                return msg.delete(tick);
            }
        }
        false
    }

    /// Get all non-deleted messages.
    pub fn visible_messages(&self) -> Vec<&EditableChatMessage> {
        self.messages.iter().filter(|m| !m.is_deleted).collect()
    }

    /// Get message count (including deleted).
    pub fn total_count(&self) -> usize {
        self.messages.len()
    }

    /// Get visible message count (excluding deleted).
    pub fn visible_count(&self) -> usize {
        self.messages.iter().filter(|m| !m.is_deleted).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_message() {
        let msg = EditableChatMessage::new_text(
            "msg-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            "Hello, world!".to_string(),
            100,
        );

        assert_eq!(msg.id, "msg-1");
        assert_eq!(msg.current_content, "Hello, world!");
        assert!(!msg.is_edited());
        assert_eq!(msg.version_count(), 1);
    }

    #[test]
    fn test_edit_message() {
        let mut msg = EditableChatMessage::new_text(
            "msg-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            "Hello, world!".to_string(),
            100,
        );

        let edited = msg.edit("Hello, updated!".to_string(), 200);
        assert!(edited);
        assert_eq!(msg.current_content, "Hello, updated!");
        assert!(msg.is_edited());
        assert_eq!(msg.version_count(), 2);
        assert_eq!(msg.versions[0].content, "Hello, world!");
        assert_eq!(msg.versions[0].edited_at, 200);
    }

    #[test]
    fn test_edit_same_content() {
        let mut msg = EditableChatMessage::new_text(
            "msg-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            "Hello, world!".to_string(),
            100,
        );

        let edited = msg.edit("Hello, world!".to_string(), 200);
        assert!(!edited);
        assert!(!msg.is_edited());
    }

    #[test]
    fn test_delete_message() {
        let mut msg = EditableChatMessage::new_text(
            "msg-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            "Hello, world!".to_string(),
            100,
        );

        let deleted = msg.delete(200);
        assert!(deleted);
        assert!(msg.is_deleted);
        assert!(msg.current_content.is_empty());
        assert!(msg.is_edited());
        assert_eq!(msg.versions[0].content, "Hello, world!");
    }

    #[test]
    fn test_cannot_edit_deleted() {
        let mut msg = EditableChatMessage::new_text(
            "msg-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            "Hello, world!".to_string(),
            100,
        );

        msg.delete(200);
        let edited = msg.edit("New content".to_string(), 300);
        assert!(!edited);
    }

    #[test]
    fn test_can_edit_permission() {
        let msg = EditableChatMessage::new_text(
            "msg-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            "Hello, world!".to_string(),
            100,
        );

        assert!(msg.can_edit("alice"));
        assert!(!msg.can_edit("bob"));
    }

    #[test]
    fn test_realm_chat_document() {
        let mut doc = RealmChatDocument::new();

        let msg1 = EditableChatMessage::new_text(
            "msg-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            "First message".to_string(),
            100,
        );
        doc.add_message(msg1);

        let msg2 = EditableChatMessage::new_text(
            "msg-2".to_string(),
            "realm-1".to_string(),
            "bob".to_string(),
            "Second message".to_string(),
            200,
        );
        doc.add_message(msg2);

        assert_eq!(doc.total_count(), 2);
        assert_eq!(doc.visible_count(), 2);

        // Alice edits her message
        let edited = doc.edit_message("msg-1", "alice", "Edited first message".to_string(), 300);
        assert!(edited);

        // Bob cannot edit Alice's message
        let edited = doc.edit_message("msg-1", "bob", "Hacked!".to_string(), 400);
        assert!(!edited);

        // Alice deletes her message
        let deleted = doc.delete_message("msg-1", "alice", 500);
        assert!(deleted);
        assert_eq!(doc.visible_count(), 1);
    }

    #[test]
    fn test_new_image_message_embedded() {
        let msg = EditableChatMessage::new_image(
            "img-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            100,
            "image/png".to_string(),
            Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string()),
            None,
            Some("test.png".to_string()),
            Some((1, 1)),
            Some("A test image".to_string()),
        );

        assert_eq!(msg.id, "img-1");
        assert!(msg.is_image());
        assert!(!msg.is_gallery());
        assert_eq!(msg.current_content, "A test image");

        // Check data URL generation
        let data_url = msg.image_data_url();
        assert!(data_url.is_some());
        assert!(data_url.unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_new_image_message_artifact_backed() {
        let msg = EditableChatMessage::new_image(
            "img-2".to_string(),
            "realm-1".to_string(),
            "bob".to_string(),
            200,
            "image/jpeg".to_string(),
            None, // No embedded data
            Some("abc123def456".to_string()), // Artifact hash
            Some("large_photo.jpg".to_string()),
            Some((1920, 1080)),
            None,
        );

        assert!(msg.is_image());
        assert_eq!(msg.current_content, "[Image]"); // Default content when no alt_text

        // No data URL for artifact-backed images
        assert!(msg.image_data_url().is_none());
    }

    #[test]
    fn test_new_gallery_message() {
        let items = vec![
            GalleryItem {
                name: "photo1.jpg".to_string(),
                mime_type: "image/jpeg".to_string(),
                size: 102400,
                thumbnail_data: Some("thumb1data".to_string()),
                artifact_hash: "hash1".to_string(),
                dimensions: Some((800, 600)),
            },
            GalleryItem {
                name: "photo2.png".to_string(),
                mime_type: "image/png".to_string(),
                size: 204800,
                thumbnail_data: Some("thumb2data".to_string()),
                artifact_hash: "hash2".to_string(),
                dimensions: Some((1024, 768)),
            },
        ];

        let msg = EditableChatMessage::new_gallery(
            "gallery-1".to_string(),
            "realm-1".to_string(),
            "alice".to_string(),
            300,
            "folder-abc".to_string(),
            Some("Vacation Photos".to_string()),
            items,
        );

        assert!(msg.is_gallery());
        assert!(!msg.is_image());
        assert_eq!(msg.current_content, "Vacation Photos");

        if let EditableMessageType::Gallery { folder_id, title, items } = &msg.message_type {
            assert_eq!(folder_id, "folder-abc");
            assert_eq!(title.as_deref(), Some("Vacation Photos"));
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "photo1.jpg");
            assert_eq!(items[1].dimensions, Some((1024, 768)));
        } else {
            panic!("Expected Gallery message type");
        }
    }

    #[test]
    fn test_gallery_default_title() {
        let items = vec![
            GalleryItem {
                name: "file.txt".to_string(),
                mime_type: "text/plain".to_string(),
                size: 100,
                thumbnail_data: None,
                artifact_hash: "hash".to_string(),
                dimensions: None,
            },
        ];

        let msg = EditableChatMessage::new_gallery(
            "gallery-2".to_string(),
            "realm-1".to_string(),
            "bob".to_string(),
            400,
            "folder-xyz".to_string(),
            None, // No title
            items,
        );

        assert_eq!(msg.current_content, "[Gallery: 1 items]");
    }
}
