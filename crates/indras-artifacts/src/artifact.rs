use rand::random;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::access::{AccessGrant, ArtifactProvenance, ArtifactStatus};

/// 32-byte player identity, compatible with iroh PublicKey bytes.
pub type PlayerId = [u8; 32];

/// Identifies an artifact. Variant tells resolution strategy.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactId {
    /// Content-addressed by BLAKE3 hash of payload.
    Blob([u8; 32]),
    /// Random unique ID (containers, mixed artifacts).
    Doc([u8; 32]),
}

impl ArtifactId {
    pub fn bytes(&self) -> &[u8; 32] {
        match self {
            ArtifactId::Blob(b) => b,
            ArtifactId::Doc(d) => d,
        }
    }
    pub fn is_blob(&self) -> bool {
        matches!(self, ArtifactId::Blob(_))
    }
    pub fn is_doc(&self) -> bool {
        matches!(self, ArtifactId::Doc(_))
    }
}

impl fmt::Debug for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self {
            ArtifactId::Blob(_) => "Blob",
            ArtifactId::Doc(_) => "Doc",
        };
        let hex: String = self
            .bytes()
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect();
        write!(f, "{prefix}({hex}..)")
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Record of a stewardship transfer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StewardshipRecord {
    pub from: PlayerId,
    pub to: PlayerId,
    pub timestamp: i64,
}

/// Record of a blessing on a token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlessingRecord {
    pub from: PlayerId,
    pub quest_id: Option<ArtifactId>,
    pub timestamp: i64,
    pub message: Option<String>,
}

/// Reference to another artifact within a container.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub position: u64,
    pub label: Option<String>,
}

/// Content metadata for artifacts that carry payload data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadRef {
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
}

/// A unified artifact — any entity in the system.
///
/// Artifacts are dimensionless: they can have content (payload), references
/// to other artifacts, both, or neither. "Dimension" is emergent:
/// - Dim 0: payload only (messages, images, files)
/// - Dim 1+: references to other artifacts (stories, galleries, collections)
///
/// There is no parent field — references are forward-only. An artifact can
/// appear in multiple collections (DAG, not tree).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Unique identifier (Blob for content-addressed, Doc for containers).
    pub id: ArtifactId,
    /// Type tag — open string, not a closed enum.
    ///
    /// Known types: "vault", "story", "gallery", "document", "request",
    /// "exchange", "collection", "inbox", "quest", "need", "offering",
    /// "intention", "message", "image", "file", "attestation", "token".
    pub artifact_type: String,
    /// Owner/steward of this artifact.
    pub steward: PlayerId,
    /// Access grants for other players.
    pub grants: Vec<AccessGrant>,
    /// Lifecycle status.
    pub status: ArtifactStatus,
    /// How we received this artifact.
    pub provenance: Option<ArtifactProvenance>,
    /// Creation timestamp.
    pub created_at: i64,
    /// Content metadata — present for content-bearing artifacts.
    pub payload: Option<PayloadRef>,
    /// References to child artifacts — present for containers.
    pub references: Vec<ArtifactRef>,
    /// Extensible key-value metadata.
    pub metadata: BTreeMap<String, Vec<u8>>,
    /// Blessing history (meaningful for token artifacts, empty for others).
    pub blessing_history: Vec<BlessingRecord>,
}

/// Generate a leaf artifact ID by hashing the payload with BLAKE3.
pub fn leaf_id(payload: &[u8]) -> ArtifactId {
    let hash = blake3::hash(payload);
    ArtifactId::Blob(*hash.as_bytes())
}

/// Generate a random artifact ID (for containers and mixed artifacts).
pub fn generate_tree_id() -> ArtifactId {
    ArtifactId::Doc(random::<[u8; 32]>())
}

/// Generate a deterministic DM story ID from two player IDs.
/// Both players produce the same ID regardless of call order.
pub fn dm_story_id(a: PlayerId, b: PlayerId) -> ArtifactId {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let a_hex: String = first.iter().map(|b| format!("{b:02x}")).collect();
    let b_hex: String = second.iter().map(|b| format!("{b:02x}")).collect();
    let input = format!("dm-v1:{a_hex}{b_hex}");
    let hash = blake3::hash(input.as_bytes());
    ArtifactId::Doc(*hash.as_bytes())
}

impl Artifact {
    /// Compute audience from active grants (all grantees with non-expired access).
    pub fn audience(&self, now: i64) -> Vec<PlayerId> {
        self.grants
            .iter()
            .filter(|g| !g.mode.is_expired(now))
            .map(|g| g.grantee)
            .collect()
    }

    /// Human-readable name, if this artifact has a payload.
    pub fn name(&self) -> Option<&str> {
        self.payload.as_ref().map(|p| p.name.as_str())
    }

    /// Whether this artifact has content.
    pub fn has_payload(&self) -> bool {
        self.payload.is_some()
    }

    /// Whether this artifact has references to other artifacts.
    pub fn has_references(&self) -> bool {
        !self.references.is_empty()
    }
}
