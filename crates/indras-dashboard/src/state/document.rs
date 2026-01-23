//! State types for document/CRDT visualization
//!
//! Provides types for tracking peer document states, convergence,
//! and document operations during sync scenarios.

use std::collections::HashMap;

/// State for the Documents tab
#[derive(Default)]
pub struct DocumentState {
    /// All peer document states, keyed by peer name
    pub peers: HashMap<String, PeerDocumentState>,
    /// Recent document events for timeline
    pub events: Vec<DocumentEvent>,
    /// Maximum events to keep
    pub max_events: usize,
    /// Selected scenario name
    pub scenario_name: Option<String>,
    /// Whether scenario is running
    pub running: bool,
    /// Current step in scenario
    pub current_step: usize,
    /// Total steps in scenario
    pub total_steps: usize,
    /// Whether all peers have converged (same heads)
    pub is_converged: bool,
}

/// Represents a peer's document state
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PeerDocumentState {
    pub peer_name: String,
    pub notebook_name: String,
    pub notes: Vec<NoteSnapshot>,
    pub heads: Vec<String>, // Automerge heads as hex
    pub note_count: usize,
}

/// Snapshot of a note for display
#[derive(Clone, Debug, PartialEq)]
pub struct NoteSnapshot {
    pub id: String,
    pub title: String,
    pub content_preview: String, // First 50 chars
    pub author: String,
}

/// Event types for document operations
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentEvent {
    NoteCreated {
        peer: String,
        note_id: String,
        title: String,
        step: usize,
    },
    NoteUpdated {
        peer: String,
        note_id: String,
        step: usize,
    },
    NoteDeleted {
        peer: String,
        note_id: String,
        step: usize,
    },
    SyncGenerated {
        from: String,
        to: String,
        size_bytes: usize,
        step: usize,
    },
    SyncApplied {
        peer: String,
        changes_applied: bool,
        step: usize,
    },
    Converged {
        peers: Vec<String>,
        step: usize,
    },
    PhaseChanged {
        phase: String,
        step: usize,
    },
}

impl DocumentState {
    pub fn new() -> Self {
        Self {
            max_events: 100,
            ..Default::default()
        }
    }

    pub fn add_event(&mut self, event: DocumentEvent) {
        self.events.push(event);
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }

    pub fn check_convergence(&mut self) {
        if self.peers.len() < 2 {
            self.is_converged = true;
            return;
        }
        let mut heads_iter = self.peers.values().map(|p| &p.heads);
        if let Some(first_heads) = heads_iter.next() {
            self.is_converged = heads_iter.all(|h| h == first_heads);
        }
    }

    pub fn peer_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.peers.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn reset(&mut self) {
        self.peers.clear();
        self.events.clear();
        self.current_step = 0;
        self.running = false;
        self.is_converged = false;
        self.scenario_name = None;
        self.total_steps = 0;
    }
}
