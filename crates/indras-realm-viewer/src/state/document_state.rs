//! Document state tracking for CRDT-edited documents
//!
//! Tracks document content per artifact hash, updated via DocumentEdit events.

use std::collections::HashMap;

use crate::events::StreamEvent;

/// Content of a collaboratively edited document
#[derive(Clone, Debug)]
pub struct DocumentContent {
    pub document_id: String,
    pub content: String,
    pub last_editor: String,
    pub last_edit_tick: u32,
    pub edit_count: u32,
}

/// State tracking document content across CRDT edits
#[derive(Clone, Debug, Default)]
pub struct DocumentState {
    /// Current content per document_id (artifact_hash)
    pub documents: HashMap<String, DocumentContent>,
}

impl DocumentState {
    /// Process a document-related event
    pub fn process_event(&mut self, event: &StreamEvent) {
        if let StreamEvent::DocumentEdit {
            tick,
            document_id,
            editor,
            content,
            ..
        } = event
        {
            let entry = self.documents.entry(document_id.clone()).or_insert_with(|| {
                DocumentContent {
                    document_id: document_id.clone(),
                    content: String::new(),
                    last_editor: String::new(),
                    last_edit_tick: 0,
                    edit_count: 0,
                }
            });
            entry.content = content.clone();
            entry.last_editor = editor.clone();
            entry.last_edit_tick = *tick;
            entry.edit_count += 1;
        }
    }

    /// Get document content by document_id (artifact_hash)
    pub fn get_content(&self, document_id: &str) -> Option<&DocumentContent> {
        self.documents.get(document_id)
    }
}
