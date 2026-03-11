//! Homepage server for IndrasNetwork.
//!
//! Serves a member's public profile page with grant-controlled visibility.
//! Every displayable item is an artifact with its own grant list — profile
//! fields and content artifacts use the same [`grants::can_view`] check.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use indras_artifacts::access::AccessGrant;

pub mod auth;
pub mod grants;
mod server;
mod templates;

/// Known profile field name constants.
pub mod fields {
    /// Display name shown on the page.
    pub const DISPLAY_NAME: &str = "display_name";
    /// Username (used in URL path).
    pub const USERNAME: &str = "username";
    /// Optional bio/description.
    pub const BIO: &str = "bio";
    /// Hex-encoded public key.
    pub const PUBLIC_KEY: &str = "public_key";
    /// Total number of intentions created.
    pub const INTENTION_COUNT: &str = "intention_count";
    /// Total tokens of gratitude held.
    pub const TOKEN_COUNT: &str = "token_count";
    /// Total blessings given to others.
    pub const BLESSINGS_GIVEN: &str = "blessings_given";
    /// Human-readable attention time contributed.
    pub const ATTENTION_CONTRIBUTED: &str = "attention_contributed";
    /// Number of contacts.
    pub const CONTACT_COUNT: &str = "contact_count";
    /// Humanness freshness score (0.0–1.0).
    pub const HUMANNESS_FRESHNESS: &str = "humanness_freshness";
    /// Active quests (JSON-serialized `Vec<IntentionSummary>`).
    pub const ACTIVE_QUESTS: &str = "active_quests";
    /// Active offerings (JSON-serialized `Vec<IntentionSummary>`).
    pub const ACTIVE_OFFERINGS: &str = "active_offerings";
}

/// Derive a deterministic ArtifactId for a profile field.
///
/// Uses BLAKE3 hash of `"indras:profile:{field_name}:{member_key_hex}"`.
pub fn profile_field_artifact_id(member_key: &[u8; 32], field_name: &str) -> [u8; 32] {
    let input = format!(
        "indras:profile:{}:{}",
        field_name,
        hex::encode(member_key)
    );
    *blake3::hash(input.as_bytes()).as_bytes()
}

/// Summary of an intention for display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IntentionSummary {
    /// Title of the intention.
    pub title: String,
    /// Kind (quest or offering).
    pub kind: String,
    /// Current status.
    pub status: String,
}

/// A profile field with grant-controlled visibility.
#[derive(Debug, Clone)]
pub struct ProfileFieldArtifact {
    /// Field name (one of the [`fields`] constants).
    pub field_name: String,
    /// Human-readable display value.
    pub display_value: String,
    /// Access grants controlling who can see this field.
    pub grants: Vec<AccessGrant>,
}

/// A content artifact from the home realm.
#[derive(Debug, Clone)]
pub struct ContentArtifact {
    /// Artifact name.
    pub name: String,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// Creation timestamp (epoch seconds).
    pub created_at: i64,
    /// Access grants controlling who can see this artifact.
    pub grants: Vec<AccessGrant>,
}

/// Errors from the homepage server.
#[derive(Debug, thiserror::Error)]
pub enum HomepageError {
    /// Failed to bind to address.
    #[error("failed to bind: {0}")]
    Bind(String),
    /// Server error.
    #[error("server error: {0}")]
    Serve(String),
}

/// Homepage server that renders grant-filtered profile fields and artifacts.
pub struct HomepageServer {
    fields: Arc<RwLock<Vec<ProfileFieldArtifact>>>,
    artifacts: Arc<RwLock<Vec<ContentArtifact>>>,
    steward: [u8; 32],
}

impl HomepageServer {
    /// Create a new homepage server.
    pub fn new(steward: [u8; 32]) -> Self {
        Self {
            fields: Arc::new(RwLock::new(Vec::new())),
            artifacts: Arc::new(RwLock::new(Vec::new())),
            steward,
        }
    }

    /// Start serving on the given address.
    pub async fn serve(self, addr: SocketAddr) -> Result<(), HomepageError> {
        server::serve(addr, self.fields, self.artifacts, self.steward).await
    }

    /// Get a handle to push profile field updates.
    pub fn fields_handle(&self) -> Arc<RwLock<Vec<ProfileFieldArtifact>>> {
        self.fields.clone()
    }

    /// Get a handle to push content artifact updates.
    pub fn artifacts_handle(&self) -> Arc<RwLock<Vec<ContentArtifact>>> {
        self.artifacts.clone()
    }

    /// Get the steward's public key.
    pub fn steward(&self) -> &[u8; 32] {
        &self.steward
    }
}
