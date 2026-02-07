use rand::random;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// 32-byte player identity, compatible with iroh PublicKey bytes.
pub type PlayerId = [u8; 32];

/// Identifies an artifact. Variant tells resolution strategy.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    pub size: u64,
    pub steward: PlayerId,
    pub audience: Vec<PlayerId>,
    pub artifact_type: LeafType,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeArtifact {
    pub id: ArtifactId,
    pub steward: PlayerId,
    pub audience: Vec<PlayerId>,
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

    pub fn audience(&self) -> &[PlayerId] {
        match self {
            Artifact::Leaf(leaf) => &leaf.audience,
            Artifact::Tree(tree) => &tree.audience,
        }
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
