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
    /// Leaf artifact — content-addressed by BLAKE3 hash of payload.
    Blob([u8; 32]),
    /// Tree artifact — random unique ID (like an iroh Doc ID).
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeafType {
    Message,
    Image,
    File,
    Attestation,
    Token,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreeType {
    Vault,
    Story,
    Gallery,
    Document,
    Request,
    Exchange,
    Collection,
    Inbox,
    Quest,
    Need,
    Offering,
    Intention,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub position: u64,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafArtifact {
    pub id: ArtifactId,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub steward: PlayerId,
    pub grants: Vec<AccessGrant>,
    pub status: ArtifactStatus,
    pub parent: Option<ArtifactId>,
    pub provenance: Option<ArtifactProvenance>,
    pub artifact_type: LeafType,
    pub created_at: i64,
    pub blessing_history: Vec<BlessingRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeArtifact {
    pub id: ArtifactId,
    pub steward: PlayerId,
    pub grants: Vec<AccessGrant>,
    pub status: ArtifactStatus,
    pub parent: Option<ArtifactId>,
    pub provenance: Option<ArtifactProvenance>,
    pub references: Vec<ArtifactRef>,
    pub metadata: BTreeMap<String, Vec<u8>>,
    pub artifact_type: TreeType,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Artifact {
    Leaf(LeafArtifact),
    Tree(TreeArtifact),
}

/// Generate a leaf artifact ID by hashing the payload with BLAKE3.
pub fn leaf_id(payload: &[u8]) -> ArtifactId {
    let hash = blake3::hash(payload);
    ArtifactId::Blob(*hash.as_bytes())
}

/// Generate a random tree artifact ID.
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
    pub fn id(&self) -> &ArtifactId {
        match self {
            Artifact::Leaf(leaf) => &leaf.id,
            Artifact::Tree(tree) => &tree.id,
        }
    }

    pub fn steward(&self) -> &PlayerId {
        match self {
            Artifact::Leaf(leaf) => &leaf.steward,
            Artifact::Tree(tree) => &tree.steward,
        }
    }

    pub fn grants(&self) -> &[AccessGrant] {
        match self {
            Artifact::Leaf(leaf) => &leaf.grants,
            Artifact::Tree(tree) => &tree.grants,
        }
    }

    pub fn status(&self) -> &ArtifactStatus {
        match self {
            Artifact::Leaf(leaf) => &leaf.status,
            Artifact::Tree(tree) => &tree.status,
        }
    }

    pub fn parent(&self) -> Option<&ArtifactId> {
        match self {
            Artifact::Leaf(leaf) => leaf.parent.as_ref(),
            Artifact::Tree(tree) => tree.parent.as_ref(),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Artifact::Leaf(leaf) => Some(&leaf.name),
            Artifact::Tree(_) => None,
        }
    }

    /// Compute audience from active grants (all grantees with non-expired access).
    pub fn audience(&self, now: i64) -> Vec<PlayerId> {
        self.grants()
            .iter()
            .filter(|g| !g.mode.is_expired(now))
            .map(|g| g.grantee)
            .collect()
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, Artifact::Leaf(_))
    }

    pub fn is_tree(&self) -> bool {
        matches!(self, Artifact::Tree(_))
    }

    pub fn as_leaf(&self) -> Option<&LeafArtifact> {
        match self {
            Artifact::Leaf(leaf) => Some(leaf),
            _ => None,
        }
    }

    pub fn as_tree(&self) -> Option<&TreeArtifact> {
        match self {
            Artifact::Tree(tree) => Some(tree),
            _ => None,
        }
    }

    pub fn as_tree_mut(&mut self) -> Option<&mut TreeArtifact> {
        match self {
            Artifact::Tree(tree) => Some(tree),
            _ => None,
        }
    }
}
