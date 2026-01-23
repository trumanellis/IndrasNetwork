//! Note document model
//!
//! Represents a single note backed by Automerge for CRDT sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A unique identifier for a note
pub type NoteId = String;

/// A note in the notebook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Unique identifier
    pub id: NoteId,
    /// Note title
    pub title: String,
    /// Note content (markdown)
    pub content: String,
    /// When the note was created
    pub created_at: DateTime<Utc>,
    /// When the note was last modified
    pub modified_at: DateTime<Utc>,
    /// Who created the note (short peer ID)
    pub author: String,
}

impl Note {
    /// Create a new note
    pub fn new(title: impl Into<String>, author: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            content: String::new(),
            created_at: now,
            modified_at: now,
            author: author.into(),
        }
    }

    /// Update the note content
    pub fn update_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.modified_at = Utc::now();
    }

    /// Update the note title
    pub fn update_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
        self.modified_at = Utc::now();
    }

    /// Get a short preview of the content
    pub fn preview(&self, max_len: usize) -> String {
        if self.content.len() <= max_len {
            self.content.clone()
        } else {
            format!("{}...", &self.content[..max_len.saturating_sub(3)])
        }
    }
}

/// Operations that can be performed on notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoteOperation {
    /// Create a new note
    Create(Note),
    /// Update a note's content
    UpdateContent { id: NoteId, content: String },
    /// Update a note's title
    UpdateTitle { id: NoteId, title: String },
    /// Delete a note
    Delete { id: NoteId },
}

impl NoteOperation {
    /// Create a "create note" operation
    pub fn create(note: Note) -> Self {
        Self::Create(note)
    }

    /// Create an "update content" operation
    pub fn update_content(id: impl Into<NoteId>, content: impl Into<String>) -> Self {
        Self::UpdateContent {
            id: id.into(),
            content: content.into(),
        }
    }

    /// Create an "update title" operation
    pub fn update_title(id: impl Into<NoteId>, title: impl Into<String>) -> Self {
        Self::UpdateTitle {
            id: id.into(),
            title: title.into(),
        }
    }

    /// Create a "delete" operation
    pub fn delete(id: impl Into<NoteId>) -> Self {
        Self::Delete { id: id.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_creation() {
        let note = Note::new("Test Note", "alice");

        assert_eq!(note.title, "Test Note");
        assert_eq!(note.author, "alice");
        assert!(note.content.is_empty());
        assert!(!note.id.is_empty());
    }

    #[test]
    fn test_note_update() {
        let mut note = Note::new("Test", "alice");
        let original_modified = note.modified_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        note.update_content("Hello world");

        assert_eq!(note.content, "Hello world");
        assert!(note.modified_at > original_modified);
    }

    #[test]
    fn test_note_preview() {
        let mut note = Note::new("Test", "alice");
        note.update_content("This is a very long content that should be truncated");

        let preview = note.preview(20);
        assert!(preview.len() <= 20);
        assert!(preview.ends_with("..."));
    }
}
