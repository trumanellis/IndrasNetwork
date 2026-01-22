//! Message types and envelopes for the messaging layer

use chrono::{DateTime, Utc};
use indras_core::{InterfaceId, PeerIdentity};
use serde::{Deserialize, Serialize};

/// Unique identifier for a message
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MessageId {
    /// Interface the message belongs to
    pub interface_id: InterfaceId,
    /// Sequence number within the interface
    pub sequence: u64,
    /// Random nonce for uniqueness
    pub nonce: [u8; 8],
}

impl MessageId {
    /// Create a new message ID
    pub fn new(interface_id: InterfaceId, sequence: u64) -> Self {
        let mut nonce = [0u8; 8];
        rand::fill(&mut nonce);
        Self {
            interface_id,
            sequence,
            nonce,
        }
    }

    /// Create from components (for deserialization)
    pub fn from_parts(interface_id: InterfaceId, sequence: u64, nonce: [u8; 8]) -> Self {
        Self {
            interface_id,
            sequence,
            nonce,
        }
    }
}

/// Content types for messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    /// Plain text message
    Text(String),

    /// Binary data with MIME type
    Binary {
        /// MIME type of the data
        mime_type: String,
        /// The binary data
        data: Vec<u8>,
    },

    /// File reference (content stored separately)
    File {
        /// File name
        name: String,
        /// File size in bytes
        size: u64,
        /// Hash of the file content
        hash: [u8; 32],
    },

    /// Reaction to another message
    Reaction {
        /// Message being reacted to
        target: MessageId,
        /// Reaction emoji or identifier
        reaction: String,
    },

    /// System message (joins, leaves, etc.)
    System(String),
}

impl MessageContent {
    /// Create a text message
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Create a binary message
    pub fn binary(mime_type: impl Into<String>, data: Vec<u8>) -> Self {
        Self::Binary {
            mime_type: mime_type.into(),
            data,
        }
    }

    /// Create a file reference
    pub fn file(name: impl Into<String>, size: u64, hash: [u8; 32]) -> Self {
        Self::File {
            name: name.into(),
            size,
            hash,
        }
    }

    /// Check if this is a text message
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Get the text content if this is a text message
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// A complete message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: Serialize + for<'de2> Deserialize<'de2>")]
pub struct Message<I: PeerIdentity> {
    /// Unique message identifier
    pub id: MessageId,
    /// Sender of the message
    pub sender: I,
    /// Interface the message belongs to
    pub interface_id: InterfaceId,
    /// Message content
    pub content: MessageContent,
    /// When the message was created
    pub timestamp: DateTime<Utc>,
    /// Optional reply reference
    pub reply_to: Option<MessageId>,
}

impl<I: PeerIdentity> Message<I> {
    /// Create a new message
    pub fn new(
        interface_id: InterfaceId,
        sender: I,
        sequence: u64,
        content: MessageContent,
    ) -> Self {
        Self {
            id: MessageId::new(interface_id, sequence),
            sender,
            interface_id,
            content,
            timestamp: Utc::now(),
            reply_to: None,
        }
    }

    /// Create a reply to another message
    pub fn reply(
        interface_id: InterfaceId,
        sender: I,
        sequence: u64,
        content: MessageContent,
        reply_to: MessageId,
    ) -> Self {
        Self {
            id: MessageId::new(interface_id, sequence),
            sender,
            interface_id,
            content,
            timestamp: Utc::now(),
            reply_to: Some(reply_to),
        }
    }

    /// Create a text message
    pub fn text(interface_id: InterfaceId, sender: I, sequence: u64, text: impl Into<String>) -> Self {
        Self::new(interface_id, sender, sequence, MessageContent::text(text))
    }
}

/// Envelope wrapping a message for transport
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: Serialize + for<'de2> Deserialize<'de2>")]
pub struct MessageEnvelope<I: PeerIdentity> {
    /// The message
    pub message: Message<I>,
    /// Optional encryption metadata
    pub encryption: Option<EncryptionMetadata>,
}

/// Metadata about message encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    /// Key ID used for encryption
    pub key_id: [u8; 32],
    /// Nonce used for encryption
    pub nonce: [u8; 12],
}

impl<I: PeerIdentity> MessageEnvelope<I> {
    /// Create an unencrypted envelope
    pub fn unencrypted(message: Message<I>) -> Self {
        Self {
            message,
            encryption: None,
        }
    }

    /// Create an encrypted envelope
    pub fn encrypted(message: Message<I>, key_id: [u8; 32], nonce: [u8; 12]) -> Self {
        Self {
            message,
            encryption: Some(EncryptionMetadata { key_id, nonce }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    #[test]
    fn test_message_id_uniqueness() {
        let interface_id = InterfaceId::new([0x42; 32]);
        let id1 = MessageId::new(interface_id, 1);
        let id2 = MessageId::new(interface_id, 1);

        // Same sequence but different nonces
        assert_ne!(id1.nonce, id2.nonce);
    }

    #[test]
    fn test_message_creation() {
        let interface_id = InterfaceId::new([0x42; 32]);
        let sender = SimulationIdentity::new('A').unwrap();
        let msg = Message::text(interface_id, sender, 1, "Hello");

        assert_eq!(msg.interface_id, interface_id);
        assert!(msg.content.is_text());
        assert_eq!(msg.content.as_text(), Some("Hello"));
        assert!(msg.reply_to.is_none());
    }

    #[test]
    fn test_message_reply() {
        let interface_id = InterfaceId::new([0x42; 32]);
        let sender = SimulationIdentity::new('A').unwrap();
        let original = Message::text(interface_id, sender, 1, "Hello");
        let reply = Message::reply(
            interface_id,
            sender,
            2,
            MessageContent::text("Reply"),
            original.id,
        );

        assert_eq!(reply.reply_to, Some(original.id));
    }

    #[test]
    fn test_content_types() {
        assert!(MessageContent::text("hello").is_text());
        assert!(!MessageContent::binary("image/png", vec![1, 2, 3]).is_text());
        assert!(!MessageContent::file("test.txt", 100, [0; 32]).is_text());
    }

    #[test]
    fn test_envelope_encryption() {
        let interface_id = InterfaceId::new([0x42; 32]);
        let sender = SimulationIdentity::new('A').unwrap();
        let msg = Message::text(interface_id, sender, 1, "Hello");

        let unencrypted = MessageEnvelope::unencrypted(msg.clone());
        assert!(unencrypted.encryption.is_none());

        let encrypted = MessageEnvelope::encrypted(msg, [0x11; 32], [0x22; 12]);
        assert!(encrypted.encryption.is_some());
    }
}
