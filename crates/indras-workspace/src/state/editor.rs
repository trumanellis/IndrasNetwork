//! Editor state — document blocks and content model.


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
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            blocks: Vec::new(),
            meta: DocumentMeta::default(),
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
