//! Chat view types and state management.
//!
//! Provides UI-layer types for rendering chat messages, plus conversion
//! from the backend `EditableChatMessage` to the view-layer `ChatMessageView`.

use indras_network::chat_message::{EditableChatMessage, EditableMessageType};

/// View model for a single chat message.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessageView {
    /// Stable message ID (not positional index).
    pub id: String,
    /// Hex member ID of the author.
    pub author_id: String,
    /// Display name for the author.
    pub author_name: String,
    /// Whether the current user authored this message.
    pub is_me: bool,
    /// Current content text.
    pub content: String,
    /// Type of message for rendering.
    pub message_type: ChatViewType,
    /// Creation timestamp in millis.
    pub timestamp_millis: u64,
    /// Formatted timestamp for display.
    pub timestamp_display: String,
    /// Whether this message has been edited.
    pub is_edited: bool,
    /// Whether this message has been deleted.
    pub is_deleted: bool,
    /// Number of versions (current + history).
    pub version_count: usize,
}

/// Message type for view-layer rendering.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatViewType {
    Text,
    System,
    Image {
        data_url: Option<String>,
        alt_text: Option<String>,
        dimensions: Option<(u32, u32)>,
    },
    Gallery {
        title: Option<String>,
        item_count: usize,
    },
    ProofSubmitted {
        quest_id: String,
    },
    BlessingGiven {
        quest_id: String,
        claimant: String,
    },
    ArtifactRecalled,
    Deleted,
}

/// Status of the chat panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatStatus {
    Idle,
    Loading,
    Sending,
}

/// State for the chat panel component.
#[derive(Debug, Clone)]
pub struct ChatState {
    /// All visible messages.
    pub messages: Vec<ChatMessageView>,
    /// Current draft text.
    pub draft: String,
    /// ID of message being edited (None if not editing).
    pub editing_id: Option<String>,
    /// Draft text for the edit-in-progress.
    pub edit_draft: String,
    /// Whether the action menu (attach, etc.) is open.
    pub action_menu_open: bool,
    /// Current status.
    pub status: ChatStatus,
    /// Error message to display (auto-dismissed).
    pub error: Option<String>,
    /// Whether to scroll to bottom on next render.
    pub should_scroll_bottom: bool,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            draft: String::new(),
            editing_id: None,
            edit_draft: String::new(),
            action_menu_open: false,
            status: ChatStatus::Loading,
            error: None,
            should_scroll_bottom: true,
        }
    }
}

/// Convert a backend `EditableChatMessage` to a view-layer `ChatMessageView`.
pub fn convert_editable_to_view(
    msg: &EditableChatMessage,
    my_id: &str,
    peer_name: &str,
) -> ChatMessageView {
    let is_me = msg.author == my_id;

    let author_name = if is_me {
        "You".to_string()
    } else {
        peer_name.to_string()
    };

    let message_type = if msg.is_deleted {
        ChatViewType::Deleted
    } else {
        match &msg.message_type {
            EditableMessageType::Text => ChatViewType::Text,
            EditableMessageType::Image {
                mime_type,
                inline_data,
                alt_text,
                dimensions,
                ..
            } => {
                let data_url = inline_data
                    .as_ref()
                    .map(|data| format!("data:{};base64,{}", mime_type, data));
                ChatViewType::Image {
                    data_url,
                    alt_text: alt_text.clone(),
                    dimensions: *dimensions,
                }
            }
            EditableMessageType::Gallery { title, items, .. } => ChatViewType::Gallery {
                title: title.clone(),
                item_count: items.len(),
            },
            EditableMessageType::ProofSubmitted { quest_id, .. } => ChatViewType::ProofSubmitted {
                quest_id: quest_id.clone(),
            },
            EditableMessageType::ProofFolderSubmitted { quest_id, .. } => {
                ChatViewType::ProofSubmitted {
                    quest_id: quest_id.clone(),
                }
            }
            EditableMessageType::BlessingGiven { quest_id, claimant } => {
                ChatViewType::BlessingGiven {
                    quest_id: quest_id.clone(),
                    claimant: claimant.clone(),
                }
            }
            EditableMessageType::ArtifactRecalled { .. } => ChatViewType::ArtifactRecalled,
        }
    };

    let timestamp_display = chrono::DateTime::from_timestamp_millis(msg.created_at as i64)
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_default();

    ChatMessageView {
        id: msg.id.clone(),
        author_id: msg.author.clone(),
        author_name,
        is_me,
        content: msg.current_content.clone(),
        message_type,
        timestamp_millis: msg.created_at,
        timestamp_display,
        is_edited: msg.is_edited(),
        is_deleted: msg.is_deleted,
        version_count: msg.version_count(),
    }
}
