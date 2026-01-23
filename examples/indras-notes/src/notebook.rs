//! Notebook - a shared collection of notes
//!
//! Wraps an NInterface to provide note-specific operations.

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use indras_core::InterfaceId;

use crate::note::{Note, NoteId, NoteOperation};

/// A shared notebook containing multiple notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    /// Notebook name
    pub name: String,
    /// Interface ID for sync
    pub interface_id: InterfaceId,
    /// Notes in this notebook
    pub notes: HashMap<NoteId, Note>,
    /// When the notebook was created
    pub created_at: chrono::DateTime<Utc>,
}

impl Notebook {
    /// Create a new notebook
    pub fn new(name: impl Into<String>, interface_id: InterfaceId) -> Self {
        Self {
            name: name.into(),
            interface_id,
            notes: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Apply a note operation
    pub fn apply(&mut self, op: NoteOperation) -> Option<NoteId> {
        match op {
            NoteOperation::Create(note) => {
                let id = note.id.clone();
                self.notes.insert(id.clone(), note);
                Some(id)
            }
            NoteOperation::UpdateContent { id, content } => {
                if let Some(note) = self.notes.get_mut(&id) {
                    note.update_content(content);
                    Some(id)
                } else {
                    None
                }
            }
            NoteOperation::UpdateTitle { id, title } => {
                if let Some(note) = self.notes.get_mut(&id) {
                    note.update_title(title);
                    Some(id)
                } else {
                    None
                }
            }
            NoteOperation::Delete { id } => {
                self.notes.remove(&id);
                Some(id)
            }
        }
    }

    /// Get a note by ID
    pub fn get(&self, id: &NoteId) -> Option<&Note> {
        self.notes.get(id)
    }

    /// Get a mutable note by ID
    pub fn get_mut(&mut self, id: &NoteId) -> Option<&mut Note> {
        self.notes.get_mut(id)
    }

    /// List all notes, sorted by modification time (newest first)
    pub fn list(&self) -> Vec<&Note> {
        let mut notes: Vec<_> = self.notes.values().collect();
        notes.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        notes
    }

    /// Get the number of notes
    pub fn count(&self) -> usize {
        self.notes.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// Summary of a notebook for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookSummary {
    /// Notebook name
    pub name: String,
    /// Interface ID
    pub interface_id: InterfaceId,
    /// Number of notes
    pub note_count: usize,
    /// When created
    pub created_at: chrono::DateTime<Utc>,
}

impl From<&Notebook> for NotebookSummary {
    fn from(nb: &Notebook) -> Self {
        Self {
            name: nb.name.clone(),
            interface_id: nb.interface_id,
            note_count: nb.notes.len(),
            created_at: nb.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notebook_operations() {
        let interface_id = InterfaceId::generate();
        let mut notebook = Notebook::new("Test", interface_id);

        // Create a note
        let note = Note::new("First Note", "alice");
        let id = note.id.clone();
        notebook.apply(NoteOperation::create(note));

        assert_eq!(notebook.count(), 1);
        assert!(notebook.get(&id).is_some());

        // Update content
        notebook.apply(NoteOperation::update_content(&id, "Hello world"));
        assert_eq!(notebook.get(&id).unwrap().content, "Hello world");

        // Delete
        notebook.apply(NoteOperation::delete(&id));
        assert!(notebook.is_empty());
    }

    #[test]
    fn test_notebook_list_sorted() {
        let interface_id = InterfaceId::generate();
        let mut notebook = Notebook::new("Test", interface_id);

        // Create notes with slight time gaps
        let note1 = Note::new("First", "alice");
        let id1 = note1.id.clone();
        notebook.apply(NoteOperation::create(note1));

        std::thread::sleep(std::time::Duration::from_millis(10));

        let note2 = Note::new("Second", "bob");
        let id2 = note2.id.clone();
        notebook.apply(NoteOperation::create(note2));

        // List should have newest first
        let list = notebook.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, id2);
        assert_eq!(list[1].id, id1);
    }
}
