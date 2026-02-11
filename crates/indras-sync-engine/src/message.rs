//! Message - chat messages within realms.
//!
//! Messages are text-based communications that can be stored in
//! any realm. They support text content and timestamps.
//!
//! Messages are CRDT-synchronized across all realm members.

use indras_network::member::MemberId;

use serde::{Deserialize, Serialize};

/// Unique identifier for a message (16 bytes).
pub type MessageId = [u8; 16];

/// Generate a new unique message ID.
pub fn generate_message_id() -> MessageId {
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

/// Content type for a stored message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageContent {
    /// Plain text message.
    Text(String),
    /// System message (joins, leaves, etc).
    System(String),
}

impl MessageContent {
    /// Get the text content if this is a text message.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// A stored message in the CRDT document.
///
/// Messages are stored with their sender, content, and timestamp
/// for CRDT synchronization across realm members.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredMessage {
    /// Unique identifier for this message.
    pub id: MessageId,
    /// The member who sent this message.
    pub sender: MemberId,
    /// The message content.
    pub content: MessageContent,
    /// When the message was sent (Unix timestamp in milliseconds).
    pub timestamp_millis: i64,
}

impl StoredMessage {
    /// Create a new text message.
    pub fn text(sender: MemberId, text: impl Into<String>) -> Self {
        Self {
            id: generate_message_id(),
            sender,
            content: MessageContent::Text(text.into()),
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Create a new system message.
    pub fn system(sender: MemberId, text: impl Into<String>) -> Self {
        Self {
            id: generate_message_id(),
            sender,
            content: MessageContent::System(text.into()),
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Get the text content if this is a text message.
    pub fn as_text(&self) -> Option<&str> {
        self.content.as_text()
    }
}

/// Document schema for storing messages in a realm.
///
/// This is used with `realm.document::<MessageDocument>("messages")` to get
/// a CRDT-synchronized message list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageDocument {
    /// All messages in this document.
    pub messages: Vec<StoredMessage>,
}

impl MessageDocument {
    /// Create a new empty message document.
    pub fn new() -> Self {
        Self { messages: Vec::new() }
    }

    /// Add a message to the document.
    pub fn add(&mut self, message: StoredMessage) {
        self.messages.push(message);
    }

    /// Add a text message from a sender.
    pub fn add_text(&mut self, sender: MemberId, text: impl Into<String>) -> MessageId {
        let msg = StoredMessage::text(sender, text);
        let id = msg.id;
        self.add(msg);
        id
    }

    /// Add a system message.
    pub fn add_system(&mut self, sender: MemberId, text: impl Into<String>) -> MessageId {
        let msg = StoredMessage::system(sender, text);
        let id = msg.id;
        self.add(msg);
        id
    }

    /// Find a message by ID.
    pub fn find(&self, id: &MessageId) -> Option<&StoredMessage> {
        self.messages.iter().find(|m| &m.id == id)
    }

    /// Get all messages by a specific sender.
    pub fn messages_by_sender(&self, sender: &MemberId) -> Vec<&StoredMessage> {
        self.messages.iter().filter(|m| &m.sender == sender).collect()
    }

    /// Get messages sorted by timestamp (oldest first).
    pub fn messages_by_time(&self) -> Vec<&StoredMessage> {
        let mut messages: Vec<&StoredMessage> = self.messages.iter().collect();
        messages.sort_by(|a, b| a.timestamp_millis.cmp(&b.timestamp_millis));
        messages
    }

    /// Get the number of messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if the document is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
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
    fn test_message_id_generation() {
        let id1 = generate_message_id();
        let id2 = generate_message_id();
        // IDs should be unique
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_stored_message_text() {
        let msg = StoredMessage::text(test_member_id(), "Hello, world!");
        assert_eq!(msg.sender, test_member_id());
        assert_eq!(msg.as_text(), Some("Hello, world!"));
    }

    #[test]
    fn test_stored_message_system() {
        let msg = StoredMessage::system(test_member_id(), "User joined");
        assert_eq!(msg.sender, test_member_id());
        match &msg.content {
            MessageContent::System(s) => assert_eq!(s, "User joined"),
            _ => panic!("Expected system message"),
        }
    }

    #[test]
    fn test_message_document() {
        let mut doc = MessageDocument::new();
        assert!(doc.is_empty());

        let id1 = doc.add_text(test_member_id(), "First message");
        let id2 = doc.add_text(another_member_id(), "Second message");

        assert_eq!(doc.len(), 2);
        assert!(doc.find(&id1).is_some());
        assert!(doc.find(&id2).is_some());
    }

    #[test]
    fn test_message_document_queries() {
        let mut doc = MessageDocument::new();

        doc.add_text(test_member_id(), "Hello from user 1");
        doc.add_text(another_member_id(), "Hello from user 2");
        doc.add_text(test_member_id(), "Another from user 1");

        // Query by sender
        let user1_msgs = doc.messages_by_sender(&test_member_id());
        assert_eq!(user1_msgs.len(), 2);

        let user2_msgs = doc.messages_by_sender(&another_member_id());
        assert_eq!(user2_msgs.len(), 1);
    }

    #[test]
    fn test_message_serialization() {
        let msg = StoredMessage::text(test_member_id(), "Test message");

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&msg).unwrap();
        let deserialized: StoredMessage = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.sender, deserialized.sender);
        assert_eq!(msg.content, deserialized.content);
    }

    #[test]
    fn test_message_document_serialization() {
        let mut doc = MessageDocument::new();
        doc.add_text(test_member_id(), "Message 1");
        doc.add_text(another_member_id(), "Message 2");

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&doc).unwrap();
        let deserialized: MessageDocument = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(doc.len(), deserialized.len());
    }

    #[test]
    fn test_message_document_default() {
        let doc = MessageDocument::default();
        assert!(doc.is_empty());
        assert_eq!(doc.len(), 0);
    }
}
