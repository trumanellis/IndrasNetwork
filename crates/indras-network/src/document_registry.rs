//! Document registry for tracking named documents in a realm.
//!
//! This module provides a CRDT document that tracks the names
//! of all documents that have been created in a realm, enabling
//! `Realm::documents()` to list them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// CRDT document for tracking document names within a realm.
///
/// When a document is opened via `Realm::document()`, its name is
/// automatically registered here. This enables discovery of all
/// documents that exist in a realm.
///
/// # Example
///
/// ```ignore
/// let doc = realm.document::<DocumentRegistryDocument>("_registry").await?;
/// let names = doc.read().await.document_names();
/// for name in names {
///     println!("Document: {}", name);
/// }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentRegistryDocument {
    /// Set of document names in this realm.
    ///
    /// Uses BTreeSet for deterministic ordering across peers.
    pub names: BTreeSet<String>,
}

impl DocumentRegistryDocument {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a document name.
    ///
    /// Returns true if the name was newly inserted.
    pub fn register(&mut self, name: impl Into<String>) -> bool {
        self.names.insert(name.into())
    }

    /// Remove a document name from the registry.
    ///
    /// Returns true if the name was present.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.names.remove(name)
    }

    /// Check if a document name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// Get all registered document names.
    pub fn document_names(&self) -> Vec<&str> {
        self.names.iter().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered documents.
    pub fn count(&self) -> usize {
        self.names.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_document() {
        let mut registry = DocumentRegistryDocument::new();

        assert!(registry.register("quests"));
        assert!(registry.register("notes"));
        assert!(!registry.register("quests")); // duplicate

        assert_eq!(registry.count(), 2);
        assert!(registry.contains("quests"));
        assert!(registry.contains("notes"));
    }

    #[test]
    fn test_unregister_document() {
        let mut registry = DocumentRegistryDocument::new();

        registry.register("quests");
        registry.register("notes");

        assert!(registry.unregister("quests"));
        assert!(!registry.unregister("quests")); // already removed
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_document_names_sorted() {
        let mut registry = DocumentRegistryDocument::new();

        registry.register("zebra");
        registry.register("alpha");
        registry.register("middle");

        let names = registry.document_names();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }
}
