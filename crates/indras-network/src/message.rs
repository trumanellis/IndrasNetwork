//! Message types for realm communication.
//!
//! Provides simplified message types that wrap the underlying
//! messaging infrastructure.

use crate::member::{Member, MemberId};
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

    /// Extension content from app layers.
    ///
    /// Apps built on Indra's Network (like SyncEngine) serialize their
    /// domain-specific content types into this variant for transport.
    Extension {
        /// Type identifier for the extension (e.g., "indras-sync-engine/v1").
        type_id: String,
        /// Serialized payload (typically postcard-encoded).
        payload: Vec<u8>,
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

    /// Access granted to an artifact.
    ///
    /// Sent when permanent access is granted or ownership is transferred.
    /// The recipient's node watches for these messages and updates their
    /// own ArtifactIndex automatically.
    ArtifactGranted {
        /// Reference to the artifact.
        artifact: ArtifactRef,
        /// What kind of access was granted.
        mode: crate::access::AccessMode,
        /// How the artifact was received.
        provenance: crate::access::ArtifactProvenance,
        /// Inherited grants (for transfers — permanent co-owners carry over).
        inherited_grants: Vec<crate::access::AccessGrant>,
    },

    /// Request to recover artifacts from a peer after device loss.
    RecoveryRequest {
        /// Who is requesting recovery.
        requester: MemberId,
    },

    /// Response to a recovery request with available artifacts.
    RecoveryManifest {
        /// The recovery manifest listing available artifacts.
        manifest: crate::artifact_recovery::RecoveryManifest,
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

    /// Check if this is an extension message.
    pub fn is_extension(&self) -> bool {
        matches!(self, Content::Extension { .. })
    }

    /// Get the extension type_id if this is an Extension message.
    pub fn extension_type_id(&self) -> Option<&str> {
        match self {
            Content::Extension { type_id, .. } => Some(type_id),
            _ => None,
        }
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

    /// Check if this is an artifact granted message.
    pub fn is_artifact_granted(&self) -> bool {
        matches!(self, Content::ArtifactGranted { .. })
    }

    /// Check if this is a recovery request message.
    pub fn is_recovery_request(&self) -> bool {
        matches!(self, Content::RecoveryRequest { .. })
    }

    /// Check if this is a recovery manifest message.
    pub fn is_recovery_manifest(&self) -> bool {
        matches!(self, Content::RecoveryManifest { .. })
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

    #[test]
    fn test_content_artifact_granted() {
        use crate::access::{AccessGrant, AccessMode, ArtifactProvenance, ProvenanceType};

        let content = Content::ArtifactGranted {
            artifact: ArtifactRef {
                name: "document.pdf".to_string(),
                size: 2048,
                hash: [0x42u8; 32],
                mime_type: Some("application/pdf".to_string()),
            },
            mode: AccessMode::Permanent,
            provenance: ArtifactProvenance {
                original_owner: [1u8; 32],
                received_from: [1u8; 32],
                received_at: 100,
                received_via: ProvenanceType::CoOwnership,
            },
            inherited_grants: vec![],
        };

        assert!(content.is_artifact_granted());
        assert!(!content.is_text());
        assert!(!content.is_artifact());
        assert!(!content.is_recovery_request());
    }

    #[test]
    fn test_content_recovery_request() {
        let content = Content::RecoveryRequest {
            requester: [1u8; 32],
        };

        assert!(content.is_recovery_request());
        assert!(!content.is_artifact_granted());
        assert!(!content.is_text());
    }

    #[test]
    fn test_content_recovery_manifest() {
        use crate::artifact_recovery::RecoveryManifest;

        let content = Content::RecoveryManifest {
            manifest: RecoveryManifest::new(),
        };

        assert!(content.is_recovery_manifest());
        assert!(!content.is_text());
        assert!(!content.is_artifact_granted());
    }

    #[test]
    fn test_content_artifact_granted_serialization() {
        use crate::access::{AccessMode, ArtifactProvenance, ProvenanceType};

        let content = Content::ArtifactGranted {
            artifact: ArtifactRef {
                name: "test.pdf".to_string(),
                size: 1024,
                hash: [0x42u8; 32],
                mime_type: Some("application/pdf".to_string()),
            },
            mode: AccessMode::Revocable,
            provenance: ArtifactProvenance {
                original_owner: [1u8; 32],
                received_from: [1u8; 32],
                received_at: 100,
                received_via: ProvenanceType::Transfer,
            },
            inherited_grants: vec![],
        };

        // Verify serialization roundtrip
        let bytes = postcard::to_allocvec(&content).unwrap();
        let deserialized: Content = postcard::from_bytes(&bytes).unwrap();
        assert!(deserialized.is_artifact_granted());
    }

    #[test]
    fn test_content_artifact_granted_with_timed_mode() {
        use crate::access::{AccessMode, ArtifactProvenance, ProvenanceType};

        let content = Content::ArtifactGranted {
            artifact: ArtifactRef {
                name: "temp_doc.pdf".to_string(),
                size: 4096,
                hash: [0x33u8; 32],
                mime_type: Some("application/pdf".to_string()),
            },
            mode: AccessMode::Timed { expires_at: 500 },
            provenance: ArtifactProvenance {
                original_owner: [1u8; 32],
                received_from: [1u8; 32],
                received_at: 100,
                received_via: ProvenanceType::CoOwnership,
            },
            inherited_grants: vec![],
        };

        assert!(content.is_artifact_granted());
        // Verify serialization roundtrip preserves expiry
        let bytes = postcard::to_allocvec(&content).unwrap();
        let deserialized: Content = postcard::from_bytes(&bytes).unwrap();
        if let Content::ArtifactGranted { mode, .. } = deserialized {
            assert!(mode.is_expired(501));
            assert!(!mode.is_expired(499));
        } else {
            panic!("Deserialized to wrong variant");
        }
    }

    #[test]
    fn test_content_artifact_granted_with_inherited_grants() {
        use crate::access::{AccessGrant, AccessMode, ArtifactProvenance, ProvenanceType};

        let inherited = vec![
            AccessGrant {
                grantee: [2u8; 32],
                mode: AccessMode::Permanent,
                granted_at: 50,
                granted_by: [1u8; 32],
            },
            AccessGrant {
                grantee: [3u8; 32],
                mode: AccessMode::Revocable,
                granted_at: 60,
                granted_by: [1u8; 32],
            },
        ];

        let content = Content::ArtifactGranted {
            artifact: ArtifactRef {
                name: "shared_file.zip".to_string(),
                size: 1048576,
                hash: [0xAAu8; 32],
                mime_type: Some("application/zip".to_string()),
            },
            mode: AccessMode::Transfer,
            provenance: ArtifactProvenance {
                original_owner: [1u8; 32],
                received_from: [1u8; 32],
                received_at: 200,
                received_via: ProvenanceType::Transfer,
            },
            inherited_grants: inherited,
        };

        assert!(content.is_artifact_granted());

        // Verify serialization roundtrip preserves inherited grants
        let bytes = postcard::to_allocvec(&content).unwrap();
        let deserialized: Content = postcard::from_bytes(&bytes).unwrap();
        if let Content::ArtifactGranted { inherited_grants, mode, .. } = deserialized {
            assert_eq!(inherited_grants.len(), 2);
            assert_eq!(mode, AccessMode::Transfer);
            assert!(inherited_grants[0].mode.allows_download());
            assert!(!inherited_grants[1].mode.allows_download());
        } else {
            panic!("Deserialized to wrong variant");
        }
    }

    #[test]
    fn test_content_recovery_manifest_with_artifacts() {
        use crate::access::AccessMode;
        use crate::artifact_recovery::{RecoverableArtifact, RecoveryManifest};

        let mut manifest = RecoveryManifest::new();
        manifest.add(RecoverableArtifact {
            id: [1u8; 32],
            name: "photo.jpg".to_string(),
            size: 204800,
            mime_type: Some("image/jpeg".to_string()),
            access_mode: AccessMode::Permanent,
            owner: [2u8; 32],
        });
        manifest.add(RecoverableArtifact {
            id: [3u8; 32],
            name: "doc.pdf".to_string(),
            size: 8192,
            mime_type: Some("application/pdf".to_string()),
            access_mode: AccessMode::Revocable,
            owner: [2u8; 32],
        });

        let content = Content::RecoveryManifest { manifest };

        assert!(content.is_recovery_manifest());

        // Verify serialization roundtrip
        let bytes = postcard::to_allocvec(&content).unwrap();
        let deserialized: Content = postcard::from_bytes(&bytes).unwrap();
        if let Content::RecoveryManifest { manifest } = deserialized {
            assert_eq!(manifest.len(), 2);
            assert_eq!(manifest.total_size(), 204800 + 8192);
            assert_eq!(manifest.fully_recoverable().len(), 1); // Only permanent
        } else {
            panic!("Deserialized to wrong variant");
        }
    }
}
