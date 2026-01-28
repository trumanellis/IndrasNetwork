//! Note - markdown documents within realms.
//!
//! Notes are rich text documents with metadata that can be stored in
//! any realm. They support markdown content, tags, and timestamps.
//!
//! Notes are CRDT-synchronized across all realm members.

use crate::member::MemberId;

use serde::{Deserialize, Serialize};

/// Unique identifier for a note (16 bytes).
pub type NoteId = [u8; 16];

/// Generate a new unique note ID.
pub fn generate_note_id() -> NoteId {
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

/// A note - a markdown document with metadata.
///
/// Notes support rich markdown content with tags for organization
/// and timestamps for tracking changes.
///
/// # Example
///
/// ```ignore
/// // Create a note
/// let note = Note::new(
///     "Meeting Notes",
///     "# Project Update\n\n- Item 1\n- Item 2",
///     my_id,
/// );
///
/// // Add tags
/// note.add_tag("work");
/// note.add_tag("meeting");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Note {
    /// Unique identifier for this note.
    pub id: NoteId,
    /// Title of the note.
    pub title: String,
    /// Markdown content of the note.
    pub content: String,
    /// The member who created this note.
    pub author: MemberId,
    /// Tags for organization and filtering.
    pub tags: Vec<String>,
    /// When the note was created (Unix timestamp in milliseconds).
    pub created_at_millis: i64,
    /// When the note was last updated (Unix timestamp in milliseconds).
    pub updated_at_millis: i64,
}

impl Note {
    /// Create a new note.
    pub fn new(
        title: impl Into<String>,
        content: impl Into<String>,
        author: MemberId,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: generate_note_id(),
            title: title.into(),
            content: content.into(),
            author,
            tags: Vec::new(),
            created_at_millis: now,
            updated_at_millis: now,
        }
    }

    /// Create a new note with tags.
    pub fn with_tags(
        title: impl Into<String>,
        content: impl Into<String>,
        author: MemberId,
        tags: Vec<String>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: generate_note_id(),
            title: title.into(),
            content: content.into(),
            author,
            tags,
            created_at_millis: now,
            updated_at_millis: now,
        }
    }

    /// Update the note's content.
    pub fn update_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.updated_at_millis = chrono::Utc::now().timestamp_millis();
    }

    /// Update the note's title.
    pub fn update_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
        self.updated_at_millis = chrono::Utc::now().timestamp_millis();
    }

    /// Add a tag to the note.
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
            self.updated_at_millis = chrono::Utc::now().timestamp_millis();
        }
    }

    /// Remove a tag from the note.
    pub fn remove_tag(&mut self, tag: &str) -> bool {
        if let Some(pos) = self.tags.iter().position(|t| t == tag) {
            self.tags.remove(pos);
            self.updated_at_millis = chrono::Utc::now().timestamp_millis();
            true
        } else {
            false
        }
    }

    /// Check if the note has a specific tag.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Check if the note has been modified since creation.
    pub fn is_modified(&self) -> bool {
        self.updated_at_millis > self.created_at_millis
    }
}

/// Document schema for storing notes in a realm.
///
/// This is used with `realm.document::<NoteDocument>("notes")` to get
/// a CRDT-synchronized note list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteDocument {
    /// All notes in this document.
    pub notes: Vec<Note>,
}

impl NoteDocument {
    /// Create a new empty note document.
    pub fn new() -> Self {
        Self { notes: Vec::new() }
    }

    /// Add a note to the document.
    pub fn add(&mut self, note: Note) {
        self.notes.push(note);
    }

    /// Create and add a new note.
    pub fn create_note(
        &mut self,
        title: impl Into<String>,
        content: impl Into<String>,
        author: MemberId,
        tags: Vec<String>,
    ) -> NoteId {
        let note = Note::with_tags(title, content, author, tags);
        let id = note.id;
        self.add(note);
        id
    }

    /// Find a note by ID.
    pub fn find(&self, id: &NoteId) -> Option<&Note> {
        self.notes.iter().find(|n| &n.id == id)
    }

    /// Find a note by ID (mutable).
    pub fn find_mut(&mut self, id: &NoteId) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| &n.id == id)
    }

    /// Remove a note by ID.
    pub fn remove(&mut self, id: &NoteId) -> Option<Note> {
        if let Some(pos) = self.notes.iter().position(|n| &n.id == id) {
            Some(self.notes.remove(pos))
        } else {
            None
        }
    }

    /// Get all notes by a specific author.
    pub fn notes_by_author(&self, author: &MemberId) -> Vec<&Note> {
        self.notes.iter().filter(|n| &n.author == author).collect()
    }

    /// Get all notes with a specific tag.
    pub fn notes_with_tag(&self, tag: &str) -> Vec<&Note> {
        self.notes.iter().filter(|n| n.has_tag(tag)).collect()
    }

    /// Get all unique tags across all notes.
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .notes
            .iter()
            .flat_map(|n| n.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// Get notes sorted by last update time (most recent first).
    pub fn notes_by_recent(&self) -> Vec<&Note> {
        let mut notes: Vec<&Note> = self.notes.iter().collect();
        notes.sort_by(|a, b| b.updated_at_millis.cmp(&a.updated_at_millis));
        notes
    }

    /// Get the number of notes.
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// Check if the document is empty.
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
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
    fn test_note_id_generation() {
        let id1 = generate_note_id();
        let id2 = generate_note_id();
        // IDs should be unique
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_note_creation() {
        let note = Note::new("Test Note", "# Hello\n\nWorld", test_member_id());
        assert_eq!(note.title, "Test Note");
        assert_eq!(note.content, "# Hello\n\nWorld");
        assert_eq!(note.author, test_member_id());
        assert!(note.tags.is_empty());
        assert!(!note.is_modified());
    }

    #[test]
    fn test_note_with_tags() {
        let note = Note::with_tags(
            "Tagged Note",
            "Content",
            test_member_id(),
            vec!["work".to_string(), "important".to_string()],
        );
        assert_eq!(note.tags.len(), 2);
        assert!(note.has_tag("work"));
        assert!(note.has_tag("important"));
        assert!(!note.has_tag("personal"));
    }

    #[test]
    fn test_note_update_content() {
        let mut note = Note::new("Test", "Original", test_member_id());
        let original_updated = note.updated_at_millis;

        // Small delay to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        note.update_content("Updated content");
        assert_eq!(note.content, "Updated content");
        assert!(note.is_modified());
        assert!(note.updated_at_millis >= original_updated);
    }

    #[test]
    fn test_note_tags() {
        let mut note = Note::new("Test", "Content", test_member_id());

        note.add_tag("tag1");
        assert!(note.has_tag("tag1"));

        // Adding duplicate should not add again
        note.add_tag("tag1");
        assert_eq!(note.tags.len(), 1);

        note.add_tag("tag2");
        assert_eq!(note.tags.len(), 2);

        assert!(note.remove_tag("tag1"));
        assert!(!note.has_tag("tag1"));
        assert!(!note.remove_tag("nonexistent"));
    }

    #[test]
    fn test_note_document() {
        let mut doc = NoteDocument::new();
        assert!(doc.is_empty());

        let note1 = Note::with_tags(
            "Note 1",
            "Content 1",
            test_member_id(),
            vec!["work".to_string()],
        );
        let id1 = note1.id;
        doc.add(note1);

        let note2 = Note::with_tags(
            "Note 2",
            "Content 2",
            another_member_id(),
            vec!["personal".to_string()],
        );
        let id2 = note2.id;
        doc.add(note2);

        assert_eq!(doc.len(), 2);
        assert!(doc.find(&id1).is_some());
        assert!(doc.find(&id2).is_some());
    }

    #[test]
    fn test_note_document_queries() {
        let mut doc = NoteDocument::new();

        doc.create_note(
            "Work Note",
            "Work content",
            test_member_id(),
            vec!["work".to_string(), "project".to_string()],
        );
        doc.create_note(
            "Personal Note",
            "Personal content",
            test_member_id(),
            vec!["personal".to_string()],
        );
        doc.create_note(
            "Other Work",
            "Other work content",
            another_member_id(),
            vec!["work".to_string()],
        );

        // Query by author
        let my_notes = doc.notes_by_author(&test_member_id());
        assert_eq!(my_notes.len(), 2);

        // Query by tag
        let work_notes = doc.notes_with_tag("work");
        assert_eq!(work_notes.len(), 2);

        let personal_notes = doc.notes_with_tag("personal");
        assert_eq!(personal_notes.len(), 1);

        // All tags
        let all_tags = doc.all_tags();
        assert_eq!(all_tags.len(), 3); // personal, project, work (sorted)
    }

    #[test]
    fn test_note_document_remove() {
        let mut doc = NoteDocument::new();
        let id = doc.create_note("Test", "Content", test_member_id(), vec![]);

        assert_eq!(doc.len(), 1);
        let removed = doc.remove(&id);
        assert!(removed.is_some());
        assert_eq!(doc.len(), 0);

        // Removing non-existent should return None
        let removed_again = doc.remove(&id);
        assert!(removed_again.is_none());
    }

    #[test]
    fn test_note_serialization() {
        let note = Note::with_tags(
            "Test Note",
            "# Markdown\n\nContent here",
            test_member_id(),
            vec!["tag1".to_string(), "tag2".to_string()],
        );

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&note).unwrap();
        let deserialized: Note = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(note.id, deserialized.id);
        assert_eq!(note.title, deserialized.title);
        assert_eq!(note.content, deserialized.content);
        assert_eq!(note.author, deserialized.author);
        assert_eq!(note.tags, deserialized.tags);
    }

    #[test]
    fn test_note_document_serialization() {
        let mut doc = NoteDocument::new();
        doc.create_note("Note 1", "Content 1", test_member_id(), vec!["tag".to_string()]);
        doc.create_note("Note 2", "Content 2", another_member_id(), vec![]);

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&doc).unwrap();
        let deserialized: NoteDocument = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(doc.len(), deserialized.len());
        assert_eq!(doc.notes[0].title, deserialized.notes[0].title);
        assert_eq!(doc.notes[1].title, deserialized.notes[1].title);
    }

    #[test]
    fn test_note_document_default() {
        let doc = NoteDocument::default();
        assert!(doc.is_empty());
        assert_eq!(doc.len(), 0);
    }
}
