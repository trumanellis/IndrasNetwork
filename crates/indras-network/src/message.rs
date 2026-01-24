//! Message types for realm communication.
//!
//! Provides simplified message types that wrap the underlying
//! messaging infrastructure.

use crate::member::Member;
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
/// artifacts (files), reactions, and system messages.
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
}
