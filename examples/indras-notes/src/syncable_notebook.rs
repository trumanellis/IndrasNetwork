//! Yrs-backed syncable notebook
//!
//! Uses InterfaceDocument from indras-sync to provide real CRDT sync
//! between multiple notebook instances.

use std::collections::HashMap;

use indras_core::{InterfaceEvent, InterfaceId, SimulationIdentity};
use indras_sync::InterfaceDocument;

use crate::note::{Note, NoteId, NoteOperation};

/// A notebook backed by Yrs for real CRDT sync
pub struct SyncableNotebook {
    /// Notebook name
    pub name: String,
    /// Interface ID for this notebook
    pub interface_id: InterfaceId,
    /// The underlying Yrs document
    doc: InterfaceDocument,
    /// Local peer identity for signing events
    local_peer: SimulationIdentity,
    /// Local event counter for unique IDs
    event_counter: u64,
    /// Cached note state (derived from events)
    notes_cache: HashMap<NoteId, Note>,
}

impl SyncableNotebook {
    /// Create a new syncable notebook
    pub fn new(
        name: impl Into<String>,
        interface_id: InterfaceId,
        local_peer: SimulationIdentity,
    ) -> Self {
        let mut doc = InterfaceDocument::new();
        doc.add_member(&local_peer);

        Self {
            name: name.into(),
            interface_id,
            doc,
            local_peer,
            event_counter: 0,
            notes_cache: HashMap::new(),
        }
    }

    /// Load a notebook from saved bytes
    pub fn load(
        name: impl Into<String>,
        interface_id: InterfaceId,
        local_peer: SimulationIdentity,
        bytes: &[u8],
    ) -> Result<Self, String> {
        let doc = InterfaceDocument::load(bytes)
            .map_err(|e| format!("Failed to load document: {}", e))?;

        // Initialize event counter based on existing events to avoid ID collisions
        let event_count = doc.event_count() as u64;

        let mut notebook = Self {
            name: name.into(),
            interface_id,
            doc,
            local_peer,
            event_counter: event_count,
            notes_cache: HashMap::new(),
        };

        // Rebuild cache from events
        notebook.rebuild_cache();
        Ok(notebook)
    }

    /// Save the notebook to bytes
    pub fn save(&self) -> Vec<u8> {
        self.doc.save()
    }

    /// Apply a note operation (creates a Yrs event)
    pub fn apply(&mut self, op: NoteOperation) -> Option<NoteId> {
        // Serialize the operation
        let op_bytes = match postcard::to_allocvec(&op) {
            Ok(bytes) => bytes,
            Err(_) => return None,
        };

        // Create an event with the operation as content
        self.event_counter += 1;
        let event = InterfaceEvent::message(self.local_peer, self.event_counter, op_bytes);

        // Append to the document
        if self.doc.append_event(&event).is_err() {
            return None;
        }

        // Apply to local cache
        self.apply_to_cache(op)
    }

    /// Apply operation to the local cache
    fn apply_to_cache(&mut self, op: NoteOperation) -> Option<NoteId> {
        match op {
            NoteOperation::Create(note) => {
                let id = note.id.clone();
                self.notes_cache.insert(id.clone(), note);
                Some(id)
            }
            NoteOperation::UpdateContent { id, content } => {
                if let Some(note) = self.notes_cache.get_mut(&id) {
                    note.update_content(content);
                    Some(id)
                } else {
                    None
                }
            }
            NoteOperation::UpdateTitle { id, title } => {
                if let Some(note) = self.notes_cache.get_mut(&id) {
                    note.update_title(title);
                    Some(id)
                } else {
                    None
                }
            }
            NoteOperation::Delete { id } => {
                self.notes_cache.remove(&id);
                Some(id)
            }
        }
    }

    /// Rebuild the notes cache from all events
    fn rebuild_cache(&mut self) {
        self.notes_cache.clear();

        let events: Vec<InterfaceEvent<SimulationIdentity>> = self.doc.events();

        for event in events {
            if let InterfaceEvent::Message { content, .. } = event
                && let Ok(op) = postcard::from_bytes::<NoteOperation>(&content)
            {
                self.apply_to_cache(op);
            }
        }
    }

    /// Get a note by ID
    pub fn get(&self, id: &NoteId) -> Option<&Note> {
        self.notes_cache.get(id)
    }

    /// List all notes, sorted by modification time (newest first)
    pub fn list(&self) -> Vec<&Note> {
        let mut notes: Vec<_> = self.notes_cache.values().collect();
        notes.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        notes
    }

    /// Get the number of notes
    pub fn count(&self) -> usize {
        self.notes_cache.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.notes_cache.is_empty()
    }

    // ===== Sync Methods =====

    /// Get the current state vector (for tracking sync state)
    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.state_vector()
    }

    /// Get state vector as hex string (for Lua interop)
    pub fn state_vector_hex(&self) -> String {
        hex::encode(self.doc.state_vector())
    }

    /// Generate a sync message for a peer given their known state vector
    ///
    /// Returns the incremental changes since the peer's known state.
    pub fn generate_sync_message(&self, their_sv: &[u8]) -> Vec<u8> {
        self.doc.generate_sync_message(their_sv).unwrap_or_default()
    }

    /// Generate a sync message from hex-encoded state vector (for Lua interop)
    pub fn generate_sync_message_hex(&self, their_sv_hex: &str) -> Vec<u8> {
        match hex::decode(their_sv_hex) {
            Ok(sv_bytes) => self.generate_sync_message(&sv_bytes),
            Err(_) => Vec::new(),
        }
    }

    /// Apply a sync message from a peer
    ///
    /// Returns true if new changes were applied.
    pub fn apply_sync_message(&mut self, changes: &[u8]) -> Result<bool, String> {
        if changes.is_empty() {
            return Ok(false);
        }

        let old_event_count = self.doc.event_count();

        self.doc
            .apply_sync_message(changes)
            .map_err(|e| format!("Sync error: {}", e))?;

        let new_event_count = self.doc.event_count();

        // If new events were added, rebuild cache
        if new_event_count > old_event_count {
            self.rebuild_cache();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Fork this notebook (create an independent copy for another peer)
    pub fn fork(&mut self, new_peer: SimulationIdentity) -> Self {
        let forked_doc = self.doc.fork().expect("Fork failed");
        forked_doc.add_member(&new_peer);

        let mut forked = Self {
            name: self.name.clone(),
            interface_id: self.interface_id,
            doc: forked_doc,
            local_peer: new_peer,
            event_counter: 0,
            notes_cache: HashMap::new(),
        };

        forked.rebuild_cache();
        forked
    }

    /// Get the local peer identity
    pub fn local_peer(&self) -> &SimulationIdentity {
        &self.local_peer
    }
}

impl SyncableNotebook {
    /// Create a clone with the same peer identity
    pub fn clone_for_same_peer(&mut self) -> Self {
        let forked_doc = self.doc.fork().expect("Fork failed");

        Self {
            name: self.name.clone(),
            interface_id: self.interface_id,
            doc: forked_doc,
            local_peer: self.local_peer,
            event_counter: self.event_counter,
            notes_cache: self.notes_cache.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_peer(c: char) -> SimulationIdentity {
        SimulationIdentity::new(c).unwrap()
    }

    #[test]
    fn test_basic_operations() {
        let peer = create_test_peer('A');
        let interface_id = InterfaceId::generate();
        let mut notebook = SyncableNotebook::new("Test", interface_id, peer);

        // Create a note
        let note = Note::new("Test Note", "alice");
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
    fn test_sync_between_notebooks() {
        let peer_a = create_test_peer('A');
        let peer_b = create_test_peer('B');
        let interface_id = InterfaceId::generate();

        // Alice creates a notebook and adds a note
        let mut alice_nb = SyncableNotebook::new("Shared", interface_id, peer_a);
        let note = Note::new("Alice's Note", "alice");
        let note_id = note.id.clone();
        alice_nb.apply(NoteOperation::create(note));

        // Create Bob's notebook as a fork of Alice's
        let mut bob_nb = alice_nb.fork(peer_b);

        // Bob should have Alice's note
        assert_eq!(bob_nb.count(), 1);
        assert!(bob_nb.get(&note_id).is_some());

        // Alice adds another note
        let note2 = Note::new("Alice's Second Note", "alice");
        let note2_id = note2.id.clone();
        alice_nb.apply(NoteOperation::create(note2));

        // Bob doesn't have it yet
        assert_eq!(bob_nb.count(), 1);

        // Sync: Alice generates message for Bob
        let bob_sv = bob_nb.state_vector();
        let sync_msg = alice_nb.generate_sync_message(&bob_sv);

        // Bob applies the sync message
        let changed = bob_nb.apply_sync_message(&sync_msg).unwrap();
        assert!(changed);

        // Bob now has both notes
        assert_eq!(bob_nb.count(), 2);
        assert!(bob_nb.get(&note2_id).is_some());
    }

    #[test]
    fn test_concurrent_edits_converge() {
        let peer_a = create_test_peer('A');
        let peer_b = create_test_peer('B');
        let interface_id = InterfaceId::generate();

        // Start with same initial state
        let mut alice_nb = SyncableNotebook::new("Shared", interface_id, peer_a);
        let mut bob_nb = alice_nb.fork(peer_b);

        // Get initial state vectors
        let alice_initial_sv = alice_nb.state_vector();
        let bob_initial_sv = bob_nb.state_vector();

        // Both make concurrent edits
        let alice_note = Note::new("Alice's Concurrent Note", "alice");
        let alice_note_id = alice_note.id.clone();
        alice_nb.apply(NoteOperation::create(alice_note));

        let bob_note = Note::new("Bob's Concurrent Note", "bob");
        let bob_note_id = bob_note.id.clone();
        bob_nb.apply(NoteOperation::create(bob_note));

        // Before sync, each has only their own note
        assert_eq!(alice_nb.count(), 1);
        assert_eq!(bob_nb.count(), 1);

        // Sync Alice -> Bob
        let sync_to_bob = alice_nb.generate_sync_message(&bob_initial_sv);
        bob_nb.apply_sync_message(&sync_to_bob).unwrap();

        // Sync Bob -> Alice
        let sync_to_alice = bob_nb.generate_sync_message(&alice_initial_sv);
        alice_nb.apply_sync_message(&sync_to_alice).unwrap();

        // Both should have both notes (CRDT convergence)
        assert_eq!(alice_nb.count(), 2);
        assert_eq!(bob_nb.count(), 2);
        assert!(alice_nb.get(&bob_note_id).is_some());
        assert!(bob_nb.get(&alice_note_id).is_some());
    }

    #[test]
    fn test_save_and_load() {
        let peer = create_test_peer('A');
        let interface_id = InterfaceId::generate();
        let mut notebook = SyncableNotebook::new("Test", interface_id, peer);

        let note = Note::new("Persisted Note", "alice");
        let note_id = note.id.clone();
        notebook.apply(NoteOperation::create(note));

        // Save
        let bytes = notebook.save();

        // Load into new notebook
        let loaded = SyncableNotebook::load("Test", interface_id, peer, &bytes).unwrap();

        assert_eq!(loaded.count(), 1);
        assert!(loaded.get(&note_id).is_some());
        assert_eq!(loaded.get(&note_id).unwrap().title, "Persisted Note");
    }
}
