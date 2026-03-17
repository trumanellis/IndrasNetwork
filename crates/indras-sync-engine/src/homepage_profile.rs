//! CRDT document for computed homepage profile fields that sync across devices.
//!
//! Acts as a materialized view of all 12 homepage fields. The polling loop computes
//! all fields and writes them here. Uses last-writer-wins merge on `updated_at`.
//! Accessed via `home.document::<HomepageProfileDocument>("_homepage_profile")`.

use serde::{Deserialize, Serialize};

/// A single homepage profile field with grant visibility info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomepageField {
    /// Field name (one of the `indras_homepage::fields` constants).
    pub name: String,
    /// Human-readable display value.
    pub value: String,
    /// Serialized grants (`Vec<AccessGrant>` as JSON string).
    /// Stored as JSON to avoid pulling `indras-artifacts` into `indras-sync-engine`.
    pub grants_json: String,
}

/// CRDT document for all computed homepage profile fields.
///
/// Uses last-writer-wins merge strategy based on `updated_at` timestamp.
/// Fields stored as a flat Vec so adding new fields doesn't require schema migration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HomepageProfileDocument {
    /// All profile fields as name→value pairs.
    pub fields: Vec<HomepageField>,
    /// Timestamp of last update (epoch seconds) for LWW merge.
    pub updated_at: i64,
}

impl indras_network::document::DocumentSchema for HomepageProfileDocument {
    fn merge(&mut self, remote: Self) {
        // Last-writer-wins: keep whichever version has the higher `updated_at`.
        if remote.updated_at > self.updated_at {
            *self = remote;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_network::document::DocumentSchema;

    fn make_field(name: &str, value: &str) -> HomepageField {
        HomepageField {
            name: name.to_string(),
            value: value.to_string(),
            grants_json: "[]".to_string(),
        }
    }

    #[test]
    fn default_is_empty() {
        let doc = HomepageProfileDocument::default();
        assert!(doc.fields.is_empty());
        assert_eq!(doc.updated_at, 0);
    }

    #[test]
    fn lww_merge_higher_timestamp_wins() {
        let mut local = HomepageProfileDocument {
            fields: vec![make_field("display_name", "Alice")],
            updated_at: 100,
        };
        let remote = HomepageProfileDocument {
            fields: vec![make_field("display_name", "Bob")],
            updated_at: 200,
        };
        local.merge(remote);
        assert_eq!(local.fields[0].value, "Bob");
        assert_eq!(local.updated_at, 200);
    }

    #[test]
    fn lww_merge_lower_timestamp_ignored() {
        let mut local = HomepageProfileDocument {
            fields: vec![make_field("display_name", "Alice")],
            updated_at: 200,
        };
        let remote = HomepageProfileDocument {
            fields: vec![make_field("display_name", "Bob")],
            updated_at: 100,
        };
        local.merge(remote);
        assert_eq!(local.fields[0].value, "Alice");
        assert_eq!(local.updated_at, 200);
    }

    #[test]
    fn lww_merge_equal_timestamp_no_change() {
        let mut local = HomepageProfileDocument {
            fields: vec![make_field("display_name", "Alice")],
            updated_at: 100,
        };
        let remote = HomepageProfileDocument {
            fields: vec![make_field("display_name", "Bob")],
            updated_at: 100,
        };
        local.merge(remote);
        assert_eq!(local.fields[0].value, "Alice");
    }

    #[test]
    fn merge_into_empty_doc() {
        let mut local = HomepageProfileDocument::default();
        let remote = HomepageProfileDocument {
            fields: vec![
                make_field("display_name", "Alice"),
                make_field("token_count", "5"),
            ],
            updated_at: 100,
        };
        local.merge(remote);
        assert_eq!(local.fields.len(), 2);
        assert_eq!(local.fields[0].value, "Alice");
        assert_eq!(local.fields[1].value, "5");
    }

    #[test]
    fn serialization_round_trip() {
        let doc = HomepageProfileDocument {
            fields: vec![make_field("display_name", "Alice")],
            updated_at: 42,
        };
        let bytes = serde_json::to_vec(&doc).unwrap();
        let restored: HomepageProfileDocument = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.fields.len(), 1);
        assert_eq!(restored.fields[0].name, "display_name");
        assert_eq!(restored.updated_at, 42);
    }
}
