//! Editor state — document blocks and content model.

use indras_artifacts::ArtifactId;
use serde::{Serialize, Deserialize};
use indras_network::DocumentSchema;

/// A content block in the document editor.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text {
        content: String,
        artifact_id: Option<String>,
    },
    Heading {
        level: u8,
        content: String,
        artifact_id: Option<String>,
    },
    Code {
        language: Option<String>,
        content: String,
        artifact_id: Option<String>,
    },
    Callout {
        content: String,
        artifact_id: Option<String>,
    },
    Todo {
        text: String,
        done: bool,
        artifact_id: Option<String>,
    },
    Image {
        caption: Option<String>,
        artifact_id: Option<String>,
    },
    Divider,
    Placeholder,
}

/// Metadata about the current document.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentMeta {
    pub doc_type: String,
    pub audience_count: usize,
    pub steward_name: String,
    pub is_own_steward: bool,
    pub created_at: String,
    pub edited_ago: String,
}

impl Default for DocumentMeta {
    fn default() -> Self {
        Self {
            doc_type: "Document".to_string(),
            audience_count: 0,
            steward_name: String::new(),
            is_own_steward: false,
            created_at: String::new(),
            edited_ago: String::new(),
        }
    }
}

/// Editor state for the current document/view.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorState {
    pub title: String,
    pub blocks: Vec<Block>,
    pub meta: DocumentMeta,
    pub tree_id: Option<ArtifactId>,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            blocks: Vec::new(),
            meta: DocumentMeta::default(),
            tree_id: None,
        }
    }

    /// Load blocks from a label-encoded reference list.
    /// Labels encode block type: "text", "heading:2", "code:rust", "callout",
    /// "todo", "todo:done", "image", "divider"
    pub fn parse_block_from_label(label: &Option<String>, content: String, id: Option<String>) -> Block {
        match label.as_deref() {
            Some(l) if l.starts_with("heading:") => {
                let level = l.strip_prefix("heading:").and_then(|s| s.parse().ok()).unwrap_or(2);
                Block::Heading { level, content, artifact_id: id }
            }
            Some(l) if l.starts_with("code:") => {
                let lang = l.strip_prefix("code:").map(|s| s.to_string());
                Block::Code { language: lang, content, artifact_id: id }
            }
            Some("code") => Block::Code { language: None, content, artifact_id: id },
            Some("callout") => Block::Callout { content, artifact_id: id },
            Some("todo") => Block::Todo { text: content, done: false, artifact_id: id },
            Some("todo:done") => Block::Todo { text: content, done: true, artifact_id: id },
            Some("image") => Block::Image { caption: Some(content), artifact_id: id },
            Some("divider") => Block::Divider,
            _ => Block::Text { content, artifact_id: id },
        }
    }
}

impl Block {
    /// Extract the text content from any block variant.
    pub fn content(&self) -> &str {
        match self {
            Block::Text { content, .. } => content,
            Block::Heading { content, .. } => content,
            Block::Code { content, .. } => content,
            Block::Callout { content, .. } => content,
            Block::Todo { text, .. } => text,
            Block::Image { caption, .. } => caption.as_deref().unwrap_or(""),
            Block::Divider | Block::Placeholder => "",
        }
    }

    /// Whether this block type supports click-to-edit.
    pub fn is_editable(&self) -> bool {
        !matches!(self, Block::Divider | Block::Placeholder | Block::Image { .. })
    }

    /// Create a new block with updated content, preserving type and metadata.
    pub fn with_content(&self, new_content: String) -> Block {
        match self {
            Block::Text { artifact_id, .. } => Block::Text { content: new_content, artifact_id: artifact_id.clone() },
            Block::Heading { level, artifact_id, .. } => Block::Heading { level: *level, content: new_content, artifact_id: artifact_id.clone() },
            Block::Code { language, artifact_id, .. } => Block::Code { language: language.clone(), content: new_content, artifact_id: artifact_id.clone() },
            Block::Callout { artifact_id, .. } => Block::Callout { content: new_content, artifact_id: artifact_id.clone() },
            Block::Todo { done, artifact_id, .. } => Block::Todo { text: new_content, done: *done, artifact_id: artifact_id.clone() },
            other => other.clone(),
        }
    }
}

/// CRDT document schema for syncing block content across peers.
/// Uses last-writer-wins merge — concurrent edits pick the latest version.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct BlockDocumentSchema {
    pub blocks: Vec<BlockEntry>,
}

/// A single block entry for CRDT sync.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockEntry {
    /// Block type label (e.g. "text", "heading:2", "code:rust")
    pub label: String,
    /// Block content text
    pub content: String,
}

impl DocumentSchema for BlockDocumentSchema {}

impl BlockDocumentSchema {
    /// Convert from editor blocks to CRDT schema.
    pub fn from_blocks(blocks: &[Block]) -> Self {
        Self {
            blocks: blocks.iter().map(|b| {
                let label = match b {
                    Block::Text { .. } => "text".to_string(),
                    Block::Heading { level, .. } => format!("heading:{}", level),
                    Block::Code { language: Some(lang), .. } => format!("code:{}", lang),
                    Block::Code { language: None, .. } => "code".to_string(),
                    Block::Callout { .. } => "callout".to_string(),
                    Block::Todo { done: true, .. } => "todo:done".to_string(),
                    Block::Todo { done: false, .. } => "todo".to_string(),
                    Block::Image { .. } => "image".to_string(),
                    Block::Divider => "divider".to_string(),
                    Block::Placeholder => "placeholder".to_string(),
                };
                BlockEntry {
                    label,
                    content: b.content().to_string(),
                }
            }).collect(),
        }
    }

    /// Convert CRDT schema back to editor blocks.
    pub fn to_blocks(&self) -> Vec<Block> {
        self.blocks.iter().map(|entry| {
            EditorState::parse_block_from_label(
                &Some(entry.label.clone()),
                entry.content.clone(),
                None,
            )
        }).collect()
    }
}
