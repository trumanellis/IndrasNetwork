//! Document model using indras-sync InterfaceDocument
//!
//! Provides a simple text document that can be synchronized across peers.

use chrono::{DateTime, Utc};
use indras_core::{InterfaceEvent, InterfaceId, SimulationIdentity};
use indras_sync::InterfaceDocument;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Document ID
pub type DocumentId = String;

/// Errors that can occur during document operations
#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Document not found")]
    NotFound,

    #[error("Invalid document state")]
    InvalidState,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Document operation types stored in events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentOp {
    /// Set the title
    SetTitle(String),
    /// Set the content
    SetContent(String),
    /// Append to content
    AppendContent(String),
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub id: DocumentId,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub author: String,
}

/// A syncable document using InterfaceDocument
pub struct Document {
    /// The underlying sync document
    doc: InterfaceDocument,
    /// Local peer identity
    local_peer: SimulationIdentity,
    /// Interface ID
    pub interface_id: InterfaceId,
    /// Document metadata
    pub meta: DocumentMeta,
    /// Event counter
    event_counter: u64,
    /// Cached content state
    cached_title: String,
    cached_content: String,
}

impl Document {
    /// Create a new document
    pub fn new(title: &str, author: &str) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let interface_id = InterfaceId::generate();

        // Use author's first char as peer identity
        let peer_char = author.chars().next().unwrap_or('A');
        let local_peer = SimulationIdentity::new(peer_char)
            .unwrap_or_else(|| SimulationIdentity::new('A').unwrap());

        let mut doc = InterfaceDocument::new();
        doc.add_member(&local_peer);

        let meta = DocumentMeta {
            id,
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            author: author.to_string(),
        };

        let mut document = Self {
            doc,
            local_peer,
            interface_id,
            meta,
            event_counter: 0,
            cached_title: title.to_string(),
            cached_content: String::new(),
        };

        // Record initial title
        document.apply_op(DocumentOp::SetTitle(title.to_string()));

        document
    }

    /// Create from existing bytes
    pub fn from_bytes(
        bytes: &[u8],
        meta: DocumentMeta,
        local_peer: SimulationIdentity,
    ) -> Result<Self, DocumentError> {
        let doc = InterfaceDocument::load(bytes)
            .map_err(|e| DocumentError::Sync(format!("Failed to load: {}", e)))?;

        let event_count = doc.event_count() as u64;

        let mut document = Self {
            doc,
            local_peer,
            interface_id: InterfaceId::generate(),
            meta,
            event_counter: event_count,
            cached_title: String::new(),
            cached_content: String::new(),
        };

        // Rebuild state from events
        document.rebuild_cache();

        Ok(document)
    }

    /// Get the document ID
    pub fn id(&self) -> &str {
        &self.meta.id
    }

    /// Get the document title
    pub fn title(&self) -> &str {
        &self.cached_title
    }

    /// Set the document title
    pub fn set_title(&mut self, title: &str) -> Result<(), DocumentError> {
        self.apply_op(DocumentOp::SetTitle(title.to_string()));
        self.meta.title = title.to_string();
        self.meta.updated_at = Utc::now();
        Ok(())
    }

    /// Get the document content
    pub fn content(&self) -> &str {
        &self.cached_content
    }

    /// Set the document content
    pub fn set_content(&mut self, content: &str) -> Result<(), DocumentError> {
        self.apply_op(DocumentOp::SetContent(content.to_string()));
        self.meta.updated_at = Utc::now();
        Ok(())
    }

    /// Append to the document content
    pub fn append_content(&mut self, text: &str) -> Result<(), DocumentError> {
        self.apply_op(DocumentOp::AppendContent(text.to_string()));
        self.meta.updated_at = Utc::now();
        Ok(())
    }

    /// Get the author
    pub fn author(&self) -> &str {
        &self.meta.author
    }

    /// Get last updated timestamp
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.meta.updated_at
    }

    /// Export the document as bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.doc.save()
    }

    /// Apply an operation
    fn apply_op(&mut self, op: DocumentOp) {
        let op_bytes = match postcard::to_allocvec(&op) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };

        self.event_counter += 1;
        let event = InterfaceEvent::message(self.local_peer, self.event_counter, op_bytes);

        if self.doc.append_event(&event).is_ok() {
            self.apply_op_to_cache(op);
        }
    }

    /// Apply operation to cache
    fn apply_op_to_cache(&mut self, op: DocumentOp) {
        match op {
            DocumentOp::SetTitle(title) => {
                self.cached_title = title;
            }
            DocumentOp::SetContent(content) => {
                self.cached_content = content;
            }
            DocumentOp::AppendContent(text) => {
                self.cached_content.push_str(&text);
            }
        }
    }

    /// Rebuild cache from events
    fn rebuild_cache(&mut self) {
        self.cached_title = self.meta.title.clone();
        self.cached_content.clear();

        let events: Vec<InterfaceEvent<SimulationIdentity>> = self.doc.events();

        for event in events {
            if let InterfaceEvent::Message { content, .. } = event
                && let Ok(op) = postcard::from_bytes::<DocumentOp>(&content)
            {
                self.apply_op_to_cache(op);
            }
        }
    }

    /// Get the current state vector for sync
    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.state_vector()
    }

    /// Generate sync message for peer
    pub fn generate_sync_message(&self, their_sv: &[u8]) -> Vec<u8> {
        self.doc.generate_sync_message(their_sv).unwrap_or_default()
    }

    /// Apply sync message from peer
    pub fn apply_sync_message(&mut self, changes: &[u8]) -> Result<bool, DocumentError> {
        if changes.is_empty() {
            return Ok(false);
        }

        let old_count = self.doc.event_count();

        self.doc
            .apply_sync_message(changes)
            .map_err(|e| DocumentError::Sync(format!("Sync error: {}", e)))?;

        let new_count = self.doc.event_count();

        if new_count > old_count {
            self.rebuild_cache();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Fork this document for another peer
    pub fn fork(&self, new_peer: SimulationIdentity) -> Self {
        let mut forked_doc = self.doc.fork().expect("Fork failed");
        forked_doc.add_member(&new_peer);

        let mut forked = Self {
            doc: forked_doc,
            local_peer: new_peer,
            interface_id: self.interface_id,
            meta: self.meta.clone(),
            event_counter: 0,
            cached_title: String::new(),
            cached_content: String::new(),
        };

        forked.rebuild_cache();
        forked
    }
}

impl std::fmt::Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Document: {}", self.title())?;
        writeln!(f, "ID: {}", self.id())?;
        writeln!(f, "Author: {}", self.author())?;
        writeln!(f, "Updated: {}", self.updated_at())?;
        writeln!(f, "---")?;
        write!(f, "{}", self.content())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_document() {
        let doc = Document::new("Test Doc", "Alice");
        assert_eq!(doc.title(), "Test Doc");
        assert_eq!(doc.author(), "Alice");
        assert_eq!(doc.content(), "");
    }

    #[test]
    fn test_set_content() {
        let mut doc = Document::new("Test", "Alice");
        doc.set_content("Hello, World!").unwrap();
        assert_eq!(doc.content(), "Hello, World!");
    }

    #[test]
    fn test_append_content() {
        let mut doc = Document::new("Test", "Alice");
        doc.set_content("Hello").unwrap();
        doc.append_content(", World!").unwrap();
        assert_eq!(doc.content(), "Hello, World!");
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut doc = Document::new("Test", "Alice");
        doc.set_content("Test content").unwrap();

        let bytes = doc.to_bytes();
        let meta = doc.meta.clone();
        let peer = SimulationIdentity::new('A').unwrap();

        let loaded = Document::from_bytes(&bytes, meta, peer).unwrap();
        assert_eq!(loaded.title(), "Test");
        assert_eq!(loaded.content(), "Test content");
    }

    #[test]
    fn test_set_title() {
        let mut doc = Document::new("Original", "Alice");
        doc.set_title("New Title").unwrap();
        assert_eq!(doc.title(), "New Title");
    }
}
