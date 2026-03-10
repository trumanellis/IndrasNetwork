//! # Indras Profile
//!
//! Profile data types with per-field visibility controls.
//!
//! Every field is wrapped in [`Visible<T>`] which pairs any value with a
//! [`Visibility`] setting (Public, Connections, Private). The [`ViewLevel`]
//! of the viewer determines which fields they can see.
//!
//! ## Key Types
//!
//! - [`Profile`] — rich profile with identity, activity, social, and content sections
//! - [`Visible<T>`] — generic wrapper pairing a value with its visibility
//! - [`Visibility`] — per-field access level
//! - [`ViewLevel`] — the viewer's access level

/// The artifact name used to store a member's profile in the network.
pub const PROFILE_ARTIFACT_NAME: &str = "_profile";

/// Derive a deterministic ArtifactId for a member's profile.
///
/// Uses a fixed prefix + member key bytes to produce a unique, stable ID
/// that doesn't change when profile content changes.
pub fn profile_artifact_id(member_key: &[u8; 32]) -> [u8; 32] {
    let mut id = [0u8; 32];
    let prefix = b"indras:profile:";
    let prefix_len = prefix.len(); // 15
    id[..prefix_len].copy_from_slice(prefix);
    let remaining = 32 - prefix_len; // 17
    id[prefix_len..].copy_from_slice(&member_key[..remaining]);
    id
}

use serde::{Deserialize, Serialize};

/// Per-field visibility setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// Visible to anyone.
    Public,
    /// Visible only to connections (contacts).
    Connections,
    /// Visible only to the profile owner.
    Private,
}

impl Default for Visibility {
    fn default() -> Self {
        Self::Public
    }
}

/// The access level of the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewLevel {
    /// Anonymous / unknown viewer.
    Public,
    /// A connected contact.
    Connection,
    /// The profile owner.
    Owner,
}

/// A value paired with its visibility setting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Visible<T> {
    /// The wrapped value.
    pub value: T,
    /// Who can see this value.
    pub visibility: Visibility,
}

impl<T> Visible<T> {
    /// Create a new public-visible value.
    pub fn new(value: T) -> Self {
        Self {
            value,
            visibility: Visibility::Public,
        }
    }

    /// Create a private value (owner only).
    pub fn private(value: T) -> Self {
        Self {
            value,
            visibility: Visibility::Private,
        }
    }

    /// Create a connections-only value.
    pub fn connections(value: T) -> Self {
        Self {
            value,
            visibility: Visibility::Connections,
        }
    }

    /// Returns the value if the viewer has sufficient access.
    pub fn for_viewer(&self, viewer: &ViewLevel) -> Option<&T> {
        match (self.visibility, viewer) {
            (Visibility::Public, _) => Some(&self.value),
            (Visibility::Connections, ViewLevel::Connection | ViewLevel::Owner) => {
                Some(&self.value)
            }
            (Visibility::Private, ViewLevel::Owner) => Some(&self.value),
            _ => None,
        }
    }
}

/// Summary of an intention for display on the profile page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentionSummary {
    /// Intention title.
    pub title: String,
    /// Kind (e.g. "Quest", "Offering", "Need", "Intention").
    pub kind: String,
    /// Status (e.g. "active", "completed").
    pub status: String,
}

/// Rich profile with per-field visibility controls.
///
/// Every field is wrapped in [`Visible<T>`] so each can independently
/// be Public, Connections-only, or Private.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    // ── Identity ──────────────────────────────────────────
    /// Display name shown on the page.
    pub display_name: Visible<String>,
    /// Username (used in URL path).
    pub username: Visible<String>,
    /// Optional bio/description.
    pub bio: Visible<Option<String>>,
    /// iroh node public key (hex-encoded).
    pub public_key: Visible<String>,

    // ── Activity ──────────────────────────────────────────
    /// Total number of intentions created.
    pub intention_count: Visible<u32>,
    /// Total tokens of gratitude held.
    pub token_count: Visible<u32>,
    /// Total blessings given to others.
    pub blessings_given: Visible<u32>,
    /// Human-readable attention time contributed.
    pub attention_contributed: Visible<String>,

    // ── Social ────────────────────────────────────────────
    /// Number of contacts.
    pub contact_count: Visible<u32>,
    /// Humanness freshness score (0.0–1.0).
    pub humanness_freshness: Visible<f64>,

    // ── Content ───────────────────────────────────────────
    /// Active quests (intentions with kind=Quest that are not completed).
    pub active_quests: Visible<Vec<IntentionSummary>>,
    /// Active offerings (intentions with kind=Offering that are not completed).
    pub active_offerings: Visible<Vec<IntentionSummary>>,
}

impl Profile {
    /// Create a new profile with identity fields; stats default to zero/empty.
    pub fn new(
        display_name: impl Into<String>,
        username: impl Into<String>,
        public_key: impl Into<String>,
    ) -> Self {
        Self {
            display_name: Visible::new(display_name.into()),
            username: Visible::new(username.into()),
            bio: Visible::new(None),
            public_key: Visible::new(public_key.into()),
            intention_count: Visible::new(0),
            token_count: Visible::new(0),
            blessings_given: Visible::new(0),
            attention_contributed: Visible::new(String::new()),
            contact_count: Visible::new(0),
            humanness_freshness: Visible::new(0.0),
            active_quests: Visible::new(Vec::new()),
            active_offerings: Visible::new(Vec::new()),
        }
    }

    /// Set the bio.
    pub fn with_bio(mut self, bio: impl Into<String>) -> Self {
        self.bio = Visible::new(Some(bio.into()));
        self
    }

    /// Set intention count.
    pub fn set_intention_count(&mut self, count: u32) {
        self.intention_count.value = count;
    }

    /// Set token count.
    pub fn set_token_count(&mut self, count: u32) {
        self.token_count.value = count;
    }

    /// Set blessings given count.
    pub fn set_blessings_given(&mut self, count: u32) {
        self.blessings_given.value = count;
    }

    /// Set attention contributed (human-readable string).
    pub fn set_attention_contributed(&mut self, time: impl Into<String>) {
        self.attention_contributed.value = time.into();
    }

    /// Set contact count.
    pub fn set_contact_count(&mut self, count: u32) {
        self.contact_count.value = count;
    }

    /// Set humanness freshness score.
    pub fn set_humanness_freshness(&mut self, score: f64) {
        self.humanness_freshness.value = score;
    }

    /// Set active quests.
    pub fn set_active_quests(&mut self, quests: Vec<IntentionSummary>) {
        self.active_quests.value = quests;
    }

    /// Set active offerings.
    pub fn set_active_offerings(&mut self, offerings: Vec<IntentionSummary>) {
        self.active_offerings.value = offerings;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_visible_to_all() {
        let v = Visible::new(42);
        assert_eq!(v.for_viewer(&ViewLevel::Public), Some(&42));
        assert_eq!(v.for_viewer(&ViewLevel::Connection), Some(&42));
        assert_eq!(v.for_viewer(&ViewLevel::Owner), Some(&42));
    }

    #[test]
    fn connections_visible_to_connection_and_owner() {
        let v = Visible::connections(42);
        assert_eq!(v.for_viewer(&ViewLevel::Public), None);
        assert_eq!(v.for_viewer(&ViewLevel::Connection), Some(&42));
        assert_eq!(v.for_viewer(&ViewLevel::Owner), Some(&42));
    }

    #[test]
    fn private_visible_to_owner_only() {
        let v = Visible::private(42);
        assert_eq!(v.for_viewer(&ViewLevel::Public), None);
        assert_eq!(v.for_viewer(&ViewLevel::Connection), None);
        assert_eq!(v.for_viewer(&ViewLevel::Owner), Some(&42));
    }

    #[test]
    fn profile_new_defaults() {
        let p = Profile::new("Alice", "alice", "deadbeef");
        assert_eq!(p.display_name.value, "Alice");
        assert_eq!(p.username.value, "alice");
        assert_eq!(p.bio.value, None);
        assert_eq!(p.intention_count.value, 0);
        assert_eq!(p.token_count.value, 0);
    }

    #[test]
    fn profile_artifact_id_deterministic() {
        let key = [0xAB; 32];
        let id1 = super::profile_artifact_id(&key);
        let id2 = super::profile_artifact_id(&key);
        assert_eq!(id1, id2);
        // Prefix is "indras:profile:" (15 bytes)
        assert_eq!(&id1[..15], b"indras:profile:");
        // Remaining 17 bytes from key
        assert_eq!(&id1[15..], &[0xAB; 17]);
    }

    #[test]
    fn profile_artifact_id_different_keys() {
        let key_a = [0x01; 32];
        let key_b = [0x02; 32];
        assert_ne!(
            super::profile_artifact_id(&key_a),
            super::profile_artifact_id(&key_b),
        );
    }

    #[test]
    fn profile_with_bio() {
        let p = Profile::new("Bob", "bob", "cafe").with_bio("P2P enthusiast");
        assert_eq!(p.bio.value, Some("P2P enthusiast".to_string()));
    }

    #[test]
    fn profile_setters() {
        let mut p = Profile::new("Carol", "carol", "1234");
        p.set_intention_count(5);
        p.set_token_count(3);
        p.set_blessings_given(7);
        p.set_contact_count(2);
        p.set_humanness_freshness(0.85);
        assert_eq!(p.intention_count.value, 5);
        assert_eq!(p.token_count.value, 3);
        assert_eq!(p.blessings_given.value, 7);
        assert_eq!(p.contact_count.value, 2);
        assert!((p.humanness_freshness.value - 0.85).abs() < f64::EPSILON);
    }
}
