//! State tracking for notes in the home realm.

use std::collections::HashMap;

use crate::events::HomeRealmEvent;

/// A single note in the home realm.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub tag_count: u32,
    pub created_tick: u32,
    pub updated_tick: Option<u32>,
    pub deleted: bool,
}

impl Note {
    /// Creates a new note.
    pub fn new(id: String, title: String, tag_count: u32, created_tick: u32) -> Self {
        Self {
            id,
            title,
            tag_count,
            created_tick,
            updated_tick: None,
            deleted: false,
        }
    }
}

/// State for tracking all notes.
#[derive(Debug, Clone, Default)]
pub struct NotesState {
    /// Map of note_id -> Note
    pub notes: HashMap<String, Note>,
}

impl NotesState {
    /// Creates a new empty notes state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a home realm event that may affect notes.
    pub fn process_event(&mut self, event: &HomeRealmEvent) {
        match event {
            HomeRealmEvent::NoteCreated {
                note_id,
                title,
                tag_count,
                tick,
                ..
            } => {
                let note = Note::new(note_id.clone(), title.clone(), *tag_count, *tick);
                self.notes.insert(note_id.clone(), note);
            }
            HomeRealmEvent::NoteUpdated { note_id, tick, .. } => {
                if let Some(note) = self.notes.get_mut(note_id) {
                    note.updated_tick = Some(*tick);
                }
            }
            HomeRealmEvent::NoteDeleted { note_id, .. } => {
                if let Some(note) = self.notes.get_mut(note_id) {
                    note.deleted = true;
                }
            }
            _ => {}
        }
    }

    /// Returns the count of active (non-deleted) notes.
    pub fn active_count(&self) -> usize {
        self.notes.values().filter(|n| !n.deleted).count()
    }

    /// Returns notes sorted by creation tick (newest first).
    pub fn notes_by_recency(&self) -> Vec<&Note> {
        let mut notes: Vec<_> = self.notes.values().collect();
        notes.sort_by(|a, b| b.created_tick.cmp(&a.created_tick));
        notes
    }

    /// Returns only active (non-deleted) notes sorted by recency.
    pub fn active_notes_by_recency(&self) -> Vec<&Note> {
        let mut notes: Vec<_> = self.notes.values().filter(|n| !n.deleted).collect();
        notes.sort_by(|a, b| b.created_tick.cmp(&a.created_tick));
        notes
    }

    /// Resets the state.
    pub fn reset(&mut self) {
        self.notes.clear();
    }
}
