//! Sync utilities for document synchronization
//!
//! Provides helpers for syncing documents between peers.

use indras_core::SimulationIdentity;

use crate::document::{Document, DocumentError};

/// Result of a sync operation
#[derive(Debug)]
pub struct SyncResult {
    /// Number of sync rounds performed
    pub rounds: u32,
    /// Whether document A was updated
    pub a_updated: bool,
    /// Whether document B was updated
    pub b_updated: bool,
}

/// Sync two documents directly (for testing/demo)
///
/// Uses Automerge's save/merge pattern: each document saves its full state,
/// then the other merges it in. This is simple and correct for demos;
/// production code uses the incremental sync protocol.
pub fn sync_documents(
    doc_a: &mut Document,
    doc_b: &mut Document,
) -> Result<SyncResult, DocumentError> {
    let mut rounds = 0;
    let mut a_updated = false;
    let mut b_updated = false;

    // Sync in rounds until no more changes
    loop {
        rounds += 1;
        let mut made_progress = false;

        // A -> B: save A's state, merge into B
        let a_bytes = doc_a.to_bytes();
        if doc_b.apply_sync(&a_bytes)? {
            made_progress = true;
            b_updated = true;
        }

        // B -> A: save B's state, merge into A
        let b_bytes = doc_b.to_bytes();
        if doc_a.apply_sync(&b_bytes)? {
            made_progress = true;
            a_updated = true;
        }

        if !made_progress || rounds > 10 {
            break;
        }
    }

    Ok(SyncResult {
        rounds,
        a_updated,
        b_updated,
    })
}

/// Create a copy of a document for another peer
pub fn fork_document(doc: &mut Document, new_peer_char: char) -> Result<Document, DocumentError> {
    let new_peer = SimulationIdentity::new(new_peer_char).ok_or_else(|| {
        DocumentError::Sync(format!(
            "Invalid peer identity: {} (must be A-Z)",
            new_peer_char
        ))
    })?;
    Ok(doc.fork(new_peer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_new_document() {
        let mut doc_a = Document::new("Test", "Alice");
        doc_a.set_content("Hello from Alice").unwrap();

        // Create Bob's copy
        let mut doc_b = fork_document(&mut doc_a, 'B').unwrap();

        // Bob should have Alice's content
        assert_eq!(doc_b.content(), "Hello from Alice");

        // Alice makes a change
        doc_a.append_content("\nLine 2 from Alice").unwrap();

        // Sync
        let result = sync_documents(&mut doc_a, &mut doc_b).unwrap();

        assert!(result.b_updated);
        assert!(doc_b.content().contains("Line 2 from Alice"));
    }

    #[test]
    fn test_concurrent_changes() {
        let mut doc_a = Document::new("Test", "Alice");
        doc_a.set_content("Start").unwrap();

        // Clone to B
        let mut doc_b = fork_document(&mut doc_a, 'B').unwrap();

        // Both make concurrent changes
        doc_a.set_content("Alice's version").unwrap();
        doc_b.set_content("Bob's version").unwrap();

        // Sync
        let result = sync_documents(&mut doc_a, &mut doc_b).unwrap();

        // Both should be updated
        assert!(result.a_updated || result.b_updated);

        // Both should have the same content (CRDT convergence)
        assert_eq!(doc_a.content(), doc_b.content());
    }
}
