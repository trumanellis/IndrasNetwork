//! Message types for realm communication.
//!
//! Provides simplified message types that wrap the underlying
//! messaging infrastructure.

use crate::member::{Member, MemberId};
use crate::proof_folder::ProofFolderId;
use crate::quest::QuestId;
use crate::token_of_gratitude::TokenOfGratitudeId;
use chrono::{DateTime, Utc};
use indras_core::{EventId, InterfaceId};
use serde::{Deserialize, Serialize};

/// Unique identifier for a message.
///
/// Wraps the underlying EventId from the core layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MessageId {
    /// The interface this message belongs to.
    pub interface_id: InterfaceId,
    /// The event ID within the interface.
    pub event_id: EventId,
}

impl MessageId {
    /// Create a new message ID.
    pub fn new(interface_id: InterfaceId, event_id: EventId) -> Self {
        Self {
            interface_id,
            event_id,
        }
    }
}

/// A message in a realm.
///
/// Messages are the primary unit of communication between realm members.
#[derive(Debug, Clone)]
pub struct Message {
    /// Unique identifier for this message.
    pub id: MessageId,
    /// The member who sent this message.
    pub sender: Member,
    /// The message content.
    pub content: Content,
    /// When the message was sent.
    pub timestamp: DateTime<Utc>,
    /// Optional message this is replying to.
    pub reply_to: Option<MessageId>,
}

impl Message {
    /// Create a new message.
    pub fn new(
        id: MessageId,
        sender: Member,
        content: Content,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            sender,
            content,
            timestamp,
            reply_to: None,
        }
    }

    /// Create a new message that is a reply.
    pub fn reply(
        id: MessageId,
        sender: Member,
        content: Content,
        timestamp: DateTime<Utc>,
        reply_to: MessageId,
    ) -> Self {
        Self {
            id,
            sender,
            content,
            timestamp,
            reply_to: Some(reply_to),
        }
    }
}

/// Wire format for messages that includes content and optional reply metadata.
///
/// This wrapper is used for serialization/deserialization to include
/// threading information alongside the content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    /// The actual message content.
    pub content: Content,
    /// Optional message ID this is replying to.
    pub reply_to: Option<MessageId>,
}

impl MessagePayload {
    /// Create a new message payload without a reply.
    pub fn new(content: Content) -> Self {
        Self {
            content,
            reply_to: None,
        }
    }

    /// Create a new message payload that is a reply.
    pub fn reply(content: Content, reply_to: MessageId) -> Self {
        Self {
            content,
            reply_to: Some(reply_to),
        }
    }
}

/// Content of a message.
///
/// Supports various content types including text, binary data,
/// artifacts (files), reactions, system messages, and quest-related content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Content {
    /// Plain text message.
    Text(String),

    /// Binary data with MIME type.
    Binary {
        mime_type: String,
        data: Vec<u8>,
    },

    /// Reference to a shared artifact.
    Artifact(ArtifactRef),

    /// Reaction to another message.
    Reaction {
        target: MessageId,
        emoji: String,
    },

    /// System message (join, leave, etc.).
    System(String),

    /// Proof submitted for a quest claim.
    ///
    /// Posted automatically when a member submits a quest claim with proof.
    ProofSubmitted {
        /// The quest being claimed.
        quest_id: QuestId,
        /// The member submitting the proof.
        claimant: MemberId,
        /// The artifact serving as proof.
        artifact: ArtifactRef,
    },

    /// Blessing given to a quest proof.
    ///
    /// Posted automatically when a member blesses a proof submission
    /// by releasing their accumulated attention.
    BlessingGiven {
        /// The quest being blessed.
        quest_id: QuestId,
        /// The member who submitted the proof.
        claimant: MemberId,
        /// The member giving the blessing.
        blesser: MemberId,
        /// Indices into AttentionDocument.events being released.
        event_indices: Vec<usize>,
    },

    /// Proof folder submitted for review.
    ///
    /// Posted automatically when a member submits a proof folder for a quest.
    /// This notification is only sent when the claimant explicitly submits
    /// the folder (not during draft editing).
    ProofFolderSubmitted {
        /// The quest this proof is for.
        quest_id: QuestId,
        /// The member submitting the proof.
        claimant: MemberId,
        /// The proof folder ID.
        folder_id: ProofFolderId,
        /// Preview of the narrative (first ~100 chars).
        narrative_preview: String,
        /// Number of artifacts in the folder.
        artifact_count: usize,
    },

    /// Artifact was recalled/unshared.
    ///
    /// Posted automatically when a member recalls a previously shared artifact.
    /// This serves as a tombstone in the chat history, indicating that an
    /// artifact existed but has been removed.
    ArtifactRecalled {
        /// The artifact hash (for reference).
        artifact_hash: [u8; 32],
        /// Who originally shared the artifact.
        sharer: MemberId,
        /// When the artifact was originally shared (tick timestamp).
        shared_at: u64,
        /// When it was recalled (tick timestamp).
        recalled_at: u64,
    },

    /// Inline image content.
    ///
    /// For images under the size threshold (~2MB), the image data is
    /// embedded directly as base64. For larger images, use InlineArtifact.
    Image {
        /// MIME type (image/png, image/jpeg, image/gif, image/webp, etc.)
        mime_type: String,
        /// Base64-encoded image data.
        data: String,
        /// Original filename.
        filename: Option<String>,
        /// Image dimensions (width, height) if known.
        dimensions: Option<(u32, u32)>,
        /// Alt text / caption.
        alt_text: Option<String>,
    },

    /// Artifact displayed inline in chat.
    ///
    /// For large images or other artifacts that should display inline
    /// (rather than as a download link), this variant references the
    /// artifact with display hints.
    InlineArtifact {
        /// Reference to the artifact.
        artifact: ArtifactRef,
        /// Whether to display inline (true) or as download link (false).
        display_inline: bool,
        /// Alt text / caption.
        alt_text: Option<String>,
    },

    /// Gallery of images/videos/files from a folder.
    ///
    /// Allows sharing multiple files at once with thumbnail previews.
    Gallery {
        /// Unique folder identifier.
        folder_id: String,
        /// Gallery title/name.
        title: Option<String>,
        /// Items in the gallery with thumbnails.
        items: Vec<GalleryItemRef>,
    },

    /// Gratitude pledged to a quest as a bounty.
    ///
    /// Posted automatically when a steward pledges a Token of Gratitude to a quest.
    GratitudePledged {
        /// The token being pledged.
        token_id: TokenOfGratitudeId,
        /// The steward pledging the token.
        pledger: MemberId,
        /// The quest the token is pledged to.
        target_quest_id: QuestId,
    },

    /// Gratitude released to a proof submitter (steward transfer).
    ///
    /// Posted automatically when a pledged token is released to a new steward.
    GratitudeReleased {
        /// The token being released.
        token_id: TokenOfGratitudeId,
        /// Who is releasing the token.
        from_steward: MemberId,
        /// Who is receiving the token.
        to_steward: MemberId,
        /// The quest the token was pledged to.
        target_quest_id: QuestId,
    },

    /// Gratitude pledge withdrawn by the steward.
    ///
    /// Posted automatically when a steward withdraws a token pledge from a quest.
    GratitudeWithdrawn {
        /// The token being withdrawn.
        token_id: TokenOfGratitudeId,
        /// The steward withdrawing the pledge.
        steward: MemberId,
        /// The quest the token was pledged to.
        target_quest_id: QuestId,
    },
}

/// Reference to an item in a gallery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryItemRef {
    /// Filename.
    pub name: String,
    /// MIME type.
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// Base64-encoded thumbnail (for images/videos).
    pub thumbnail_data: Option<String>,
    /// Content hash reference to artifact storage.
    pub hash: [u8; 32],
    /// Item dimensions (width, height) if applicable.
    pub dimensions: Option<(u32, u32)>,
}

impl Content {
    /// Get the text content if this is a text message.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Check if this is a text message.
    pub fn is_text(&self) -> bool {
        matches!(self, Content::Text(_))
    }

    /// Check if this is an artifact reference.
    pub fn is_artifact(&self) -> bool {
        matches!(self, Content::Artifact(_))
    }

    /// Check if this is a reaction.
    pub fn is_reaction(&self) -> bool {
        matches!(self, Content::Reaction { .. })
    }

    /// Check if this is a system message.
    pub fn is_system(&self) -> bool {
        matches!(self, Content::System(_))
    }

    /// Check if this is a proof submission message.
    pub fn is_proof_submitted(&self) -> bool {
        matches!(self, Content::ProofSubmitted { .. })
    }

    /// Check if this is a blessing message.
    pub fn is_blessing_given(&self) -> bool {
        matches!(self, Content::BlessingGiven { .. })
    }

    /// Check if this is a proof folder submitted message.
    pub fn is_proof_folder_submitted(&self) -> bool {
        matches!(self, Content::ProofFolderSubmitted { .. })
    }

    /// Check if this is an artifact recalled (tombstone) message.
    pub fn is_artifact_recalled(&self) -> bool {
        matches!(self, Content::ArtifactRecalled { .. })
    }

    /// Get the artifact hash if this is an artifact recalled message.
    pub fn artifact_hash(&self) -> Option<&[u8; 32]> {
        match self {
            Content::ArtifactRecalled { artifact_hash, .. } => Some(artifact_hash),
            _ => None,
        }
    }

    /// Check if this is a gratitude pledged message.
    pub fn is_gratitude_pledged(&self) -> bool {
        matches!(self, Content::GratitudePledged { .. })
    }

    /// Check if this is a gratitude released message.
    pub fn is_gratitude_released(&self) -> bool {
        matches!(self, Content::GratitudeReleased { .. })
    }

    /// Check if this is a gratitude withdrawn message.
    pub fn is_gratitude_withdrawn(&self) -> bool {
        matches!(self, Content::GratitudeWithdrawn { .. })
    }

    /// Get the quest ID if this is a proof, blessing, or gratitude message.
    pub fn quest_id(&self) -> Option<&QuestId> {
        match self {
            Content::ProofSubmitted { quest_id, .. } => Some(quest_id),
            Content::BlessingGiven { quest_id, .. } => Some(quest_id),
            Content::ProofFolderSubmitted { quest_id, .. } => Some(quest_id),
            Content::GratitudePledged { target_quest_id, .. } => Some(target_quest_id),
            Content::GratitudeReleased { target_quest_id, .. } => Some(target_quest_id),
            Content::GratitudeWithdrawn { target_quest_id, .. } => Some(target_quest_id),
            _ => None,
        }
    }

    /// Check if this is an inline image message.
    pub fn is_image(&self) -> bool {
        matches!(self, Content::Image { .. })
    }

    /// Check if this is an inline artifact message.
    pub fn is_inline_artifact(&self) -> bool {
        matches!(self, Content::InlineArtifact { .. })
    }

    /// Check if this is a gallery message.
    pub fn is_gallery(&self) -> bool {
        matches!(self, Content::Gallery { .. })
    }

    /// Check if this is any kind of inline displayable image.
    ///
    /// Returns true for embedded images and inline artifacts with image MIME types.
    pub fn is_inline_image(&self) -> bool {
        match self {
            Content::Image { .. } => true,
            Content::InlineArtifact { display_inline, artifact, .. } => {
                *display_inline && artifact.mime_type.as_ref()
                    .map(|m| m.starts_with("image/"))
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Get image data URL if this is an embedded image.
    ///
    /// Returns a `data:` URL that can be used directly in an `<img>` tag.
    pub fn as_image_data_url(&self) -> Option<String> {
        match self {
            Content::Image { mime_type, data, .. } => {
                Some(format!("data:{};base64,{}", mime_type, data))
            }
            _ => None,
        }
    }

    /// Get image dimensions if this content has them.
    pub fn image_dimensions(&self) -> Option<(u32, u32)> {
        match self {
            Content::Image { dimensions, .. } => *dimensions,
            Content::InlineArtifact { .. } => None, // Could be fetched from artifact metadata
            _ => None,
        }
    }

    /// Get the alt text if this content has one.
    pub fn alt_text(&self) -> Option<&str> {
        match self {
            Content::Image { alt_text, .. } => alt_text.as_deref(),
            Content::InlineArtifact { alt_text, .. } => alt_text.as_deref(),
            Content::Gallery { title, .. } => title.as_deref(),
            _ => None,
        }
    }
}

// Ergonomic conversions for Content
impl From<&str> for Content {
    fn from(s: &str) -> Self {
        Content::Text(s.to_string())
    }
}

impl From<String> for Content {
    fn from(s: String) -> Self {
        Content::Text(s)
    }
}

impl From<ArtifactRef> for Content {
    fn from(artifact: ArtifactRef) -> Self {
        Content::Artifact(artifact)
    }
}

/// Reference to a shared artifact in a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Artifact name.
    pub name: String,
    /// Artifact size in bytes.
    pub size: u64,
    /// Content hash (BLAKE3).
    pub hash: [u8; 32],
    /// MIME type if known.
    pub mime_type: Option<String>,
}

/// Builder for creating messages with content.
pub struct MessageBuilder {
    content: Content,
    reply_to: Option<MessageId>,
}

impl MessageBuilder {
    /// Create a new message builder with text content.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: Content::Text(text.into()),
            reply_to: None,
        }
    }

    /// Create a new message builder with binary content.
    pub fn binary(mime_type: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            content: Content::Binary {
                mime_type: mime_type.into(),
                data,
            },
            reply_to: None,
        }
    }

    /// Create a reaction to another message.
    pub fn reaction(target: MessageId, emoji: impl Into<String>) -> Self {
        Self {
            content: Content::Reaction {
                target,
                emoji: emoji.into(),
            },
            reply_to: None,
        }
    }

    /// Set this message as a reply to another.
    pub fn reply_to(mut self, msg_id: MessageId) -> Self {
        self.reply_to = Some(msg_id);
        self
    }

    /// Get the content.
    pub fn build(self) -> (Content, Option<MessageId>) {
        (self.content, self.reply_to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_from_str() {
        let content: Content = "Hello, world!".into();
        assert!(content.is_text());
        assert_eq!(content.as_text(), Some("Hello, world!"));
    }

    #[test]
    fn test_content_from_string() {
        let content: Content = String::from("Hello").into();
        assert!(content.is_text());
    }

    #[test]
    fn test_message_builder() {
        use indras_core::{EventId, InterfaceId};
        let msg_id = MessageId::new(
            InterfaceId::new([0u8; 32]),
            EventId::new(0, 0),
        );
        let (content, reply_to) = MessageBuilder::text("Hello")
            .reply_to(msg_id)
            .build();

        assert!(content.is_text());
        assert!(reply_to.is_some());
    }

    #[test]
    fn test_content_image() {
        let content = Content::Image {
            mime_type: "image/png".to_string(),
            data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string(),
            filename: Some("test.png".to_string()),
            dimensions: Some((1, 1)),
            alt_text: Some("A test image".to_string()),
        };

        assert!(content.is_image());
        assert!(content.is_inline_image());
        assert!(!content.is_gallery());
        assert_eq!(content.image_dimensions(), Some((1, 1)));
        assert_eq!(content.alt_text(), Some("A test image"));

        let data_url = content.as_image_data_url();
        assert!(data_url.is_some());
        assert!(data_url.unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_content_inline_artifact_image() {
        let artifact = ArtifactRef {
            name: "photo.jpg".to_string(),
            size: 1024 * 1024,
            hash: [0u8; 32],
            mime_type: Some("image/jpeg".to_string()),
        };

        let content = Content::InlineArtifact {
            artifact,
            display_inline: true,
            alt_text: Some("A photo".to_string()),
        };

        assert!(content.is_inline_artifact());
        assert!(content.is_inline_image());
        assert!(!content.is_image()); // Different from embedded Image
        assert_eq!(content.alt_text(), Some("A photo"));
        assert!(content.as_image_data_url().is_none()); // No embedded data
    }

    #[test]
    fn test_content_inline_artifact_not_image() {
        let artifact = ArtifactRef {
            name: "document.pdf".to_string(),
            size: 1024,
            hash: [0u8; 32],
            mime_type: Some("application/pdf".to_string()),
        };

        let content = Content::InlineArtifact {
            artifact,
            display_inline: true,
            alt_text: None,
        };

        assert!(content.is_inline_artifact());
        assert!(!content.is_inline_image()); // PDF is not an image
    }

    #[test]
    fn test_content_gallery() {
        let items = vec![
            GalleryItemRef {
                name: "photo1.jpg".to_string(),
                mime_type: "image/jpeg".to_string(),
                size: 102400,
                thumbnail_data: Some("thumb1".to_string()),
                hash: [1u8; 32],
                dimensions: Some((800, 600)),
            },
            GalleryItemRef {
                name: "photo2.png".to_string(),
                mime_type: "image/png".to_string(),
                size: 204800,
                thumbnail_data: Some("thumb2".to_string()),
                hash: [2u8; 32],
                dimensions: Some((1024, 768)),
            },
        ];

        let content = Content::Gallery {
            folder_id: "folder-123".to_string(),
            title: Some("Vacation Photos".to_string()),
            items,
        };

        assert!(content.is_gallery());
        assert!(!content.is_image());
        assert!(!content.is_inline_image());
        assert_eq!(content.alt_text(), Some("Vacation Photos"));

        if let Content::Gallery { folder_id, title, items } = content {
            assert_eq!(folder_id, "folder-123");
            assert_eq!(title, Some("Vacation Photos".to_string()));
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "photo1.jpg");
            assert_eq!(items[1].dimensions, Some((1024, 768)));
        }
    }
}
