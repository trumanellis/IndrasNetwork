//! Realm Alias - CRDT-synchronized realm nicknames.
//!
//! Each realm can have a mutually editable alias that all members can modify.
//! The alias is stored as a CRDT document, providing Last-Writer-Wins semantics
//! via Automerge's native merge behavior.

use serde::{Deserialize, Serialize};

/// Maximum length for a realm alias (in characters, Unicode allowed).
pub const MAX_ALIAS_LENGTH: usize = 77;

/// A realm alias - a CRDT-synchronized nickname for a realm.
///
/// All realm members can edit the alias, with automatic conflict resolution
/// using Last-Writer-Wins semantics.
///
/// # Example
///
/// ```ignore
/// // Get the alias document for a realm
/// let alias_doc = realm.alias().await?;
///
/// // Read current alias
/// let current = alias_doc.read().await.alias.clone();
///
/// // Update alias (auto-synced to all members)
/// alias_doc.update(|a| {
///     a.set_alias("Project Alpha");
/// }).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealmAlias {
    /// The alias string (max 77 chars, all unicode allowed).
    pub alias: String,
    /// Tick when last updated (for display purposes).
    pub updated_at: u64,
}

impl Default for RealmAlias {
    fn default() -> Self {
        Self {
            alias: String::new(),
            updated_at: 0,
        }
    }
}

impl RealmAlias {
    /// Create a new realm alias with the given value.
    pub fn new(alias: impl Into<String>) -> Self {
        let alias = alias.into();
        Self {
            alias: truncate_alias(&alias),
            updated_at: current_tick(),
        }
    }

    /// Set the alias, truncating if necessary.
    pub fn set_alias(&mut self, alias: impl Into<String>) {
        let alias = alias.into();
        self.alias = truncate_alias(&alias);
        self.updated_at = current_tick();
    }

    /// Clear the alias.
    pub fn clear(&mut self) {
        self.alias.clear();
        self.updated_at = current_tick();
    }

    /// Check if the alias is empty.
    pub fn is_empty(&self) -> bool {
        self.alias.is_empty()
    }

    /// Get the alias, or None if empty.
    pub fn as_option(&self) -> Option<&str> {
        if self.alias.is_empty() {
            None
        } else {
            Some(&self.alias)
        }
    }
}

/// Document schema for storing realm aliases.
///
/// This is used with `realm.document::<RealmAliasDocument>("alias")` to get
/// a CRDT-synchronized alias for the realm.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RealmAliasDocument {
    /// The alias data.
    pub alias: RealmAlias,
}

impl RealmAliasDocument {
    /// Create a new empty alias document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a document with an initial alias.
    pub fn with_alias(alias: impl Into<String>) -> Self {
        Self {
            alias: RealmAlias::new(alias),
        }
    }

    /// Get the current alias string.
    pub fn get(&self) -> &str {
        &self.alias.alias
    }

    /// Get the alias as an Option (None if empty).
    pub fn get_option(&self) -> Option<&str> {
        self.alias.as_option()
    }

    /// Set the alias.
    pub fn set(&mut self, alias: impl Into<String>) {
        self.alias.set_alias(alias);
    }

    /// Clear the alias.
    pub fn clear(&mut self) {
        self.alias.clear();
    }

    /// Check if the alias is empty.
    pub fn is_empty(&self) -> bool {
        self.alias.is_empty()
    }

    /// Get the last update tick.
    pub fn updated_at(&self) -> u64 {
        self.alias.updated_at
    }
}

/// Truncate an alias to the maximum allowed length.
fn truncate_alias(alias: &str) -> String {
    alias.chars().take(MAX_ALIAS_LENGTH).collect()
}

/// Get a monotonic tick value for timestamps.
fn current_tick() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_realm_alias_creation() {
        let alias = RealmAlias::new("Test Realm");
        assert_eq!(alias.alias, "Test Realm");
        assert!(!alias.is_empty());
    }

    #[test]
    fn test_realm_alias_default() {
        let alias = RealmAlias::default();
        assert!(alias.is_empty());
        assert!(alias.as_option().is_none());
    }

    #[test]
    fn test_realm_alias_set() {
        let mut alias = RealmAlias::default();
        alias.set_alias("New Name");
        assert_eq!(alias.alias, "New Name");
        assert!(!alias.is_empty());
    }

    #[test]
    fn test_realm_alias_clear() {
        let mut alias = RealmAlias::new("Test");
        alias.clear();
        assert!(alias.is_empty());
    }

    #[test]
    fn test_alias_truncation() {
        let long_alias = "a".repeat(100);
        let alias = RealmAlias::new(&long_alias);
        assert_eq!(alias.alias.chars().count(), MAX_ALIAS_LENGTH);
    }

    #[test]
    fn test_alias_unicode() {
        let unicode_alias = "Proyecto \u{1F680} Alfa \u{2728}";
        let alias = RealmAlias::new(unicode_alias);
        assert_eq!(alias.alias, unicode_alias);
    }

    #[test]
    fn test_alias_document() {
        let mut doc = RealmAliasDocument::new();
        assert!(doc.is_empty());

        doc.set("My Realm");
        assert_eq!(doc.get(), "My Realm");
        assert!(!doc.is_empty());

        doc.clear();
        assert!(doc.is_empty());
    }

    #[test]
    fn test_alias_document_with_initial() {
        let doc = RealmAliasDocument::with_alias("Initial");
        assert_eq!(doc.get(), "Initial");
    }

    #[test]
    fn test_alias_serialization() {
        let doc = RealmAliasDocument::with_alias("Test Alias");

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&doc).unwrap();
        let deserialized: RealmAliasDocument = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(doc.get(), deserialized.get());
    }
}
