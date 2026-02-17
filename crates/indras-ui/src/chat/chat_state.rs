//! Chat view types and state management.
//!
//! Provides UI-layer types for rendering chat messages, plus conversion
//! from the backend `EditableChatMessage` to the view-layer `ChatMessageView`.

use indras_network::chat_message::{EditableChatMessage, EditableMessageType, RealmChatDocument};
use crate::identity::{member_name, member_color_class};

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
    /// Single-letter author display (e.g., "A", "B").
    pub author_letter: String,
    /// CSS color class for the author (e.g., "member-love").
    pub author_color_class: String,
    /// Preview of the message being replied to, if any.
    pub reply_preview: Option<ReplyPreview>,
    /// Reactions on this message.
    pub reactions: Vec<ReactionView>,
    /// Delivery status (for sent messages).
    pub delivery_status: DeliveryStatus,
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

/// Preview of a replied-to message.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplyPreview {
    /// ID of the original message.
    pub original_id: String,
    /// Display name of the original author.
    pub author_name: String,
    /// CSS color class for the original author.
    pub author_color_class: String,
    /// Truncated content of the original message.
    pub content_snippet: String,
}

/// View model for a reaction on a message.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionView {
    /// The emoji.
    pub emoji: String,
    /// Number of reactors.
    pub count: usize,
    /// Whether the current user reacted with this emoji.
    pub includes_me: bool,
    /// Display names of all reactors.
    pub author_names: Vec<String>,
}

/// Delivery status for sent messages.
#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryStatus {
    /// No status (received messages).
    None,
    /// Message sent but not yet read.
    Sent,
    /// Message read by peer.
    Read,
}

/// View model for a typing peer.
#[derive(Debug, Clone, PartialEq)]
pub struct TypingPeerView {
    /// Display name of the typing peer.
    pub name: String,
    /// CSS color class.
    pub color_class: String,
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
    /// Active reply compose state.
    pub replying_to: Option<ReplyPreview>,
    /// Peers currently typing.
    pub typing_peers: Vec<TypingPeerView>,
    /// Whether the emoji picker is open.
    pub emoji_picker_open: bool,
    /// Message ID for which the reaction picker is open.
    pub reaction_picker_msg_id: Option<String>,
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
            replying_to: None,
            typing_peers: Vec::new(),
            emoji_picker_open: false,
            reaction_picker_msg_id: None,
        }
    }
}

/// Convert a backend `EditableChatMessage` to a view-layer `ChatMessageView`.
pub fn convert_editable_to_view(
    msg: &EditableChatMessage,
    my_id: &str,
    peer_name: &str,
    doc: Option<&RealmChatDocument>,
    peer_last_read: Option<u64>,
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

    let author_letter = member_name(&msg.author);
    let author_color_class = member_color_class(&msg.author).to_string();

    // Build reply preview if this is a reply
    let reply_preview = msg.reply_to.as_ref().and_then(|reply_id| {
        doc.and_then(|d| {
            d.reply_preview(reply_id).map(|(author, snippet)| ReplyPreview {
                original_id: reply_id.clone(),
                author_name: member_name(&author),
                author_color_class: member_color_class(&author).to_string(),
                content_snippet: snippet,
            })
        })
    });

    // Build reaction views
    let reactions: Vec<ReactionView> = msg
        .reactions
        .iter()
        .map(|(emoji, authors)| ReactionView {
            emoji: emoji.clone(),
            count: authors.len(),
            includes_me: authors.iter().any(|a| a == my_id),
            author_names: authors.iter().map(|a| member_name(a)).collect(),
        })
        .collect();

    // Delivery status
    let delivery_status = if is_me {
        match peer_last_read {
            Some(seq) if msg.created_at <= seq => DeliveryStatus::Read,
            Some(_) => DeliveryStatus::Sent,
            None => DeliveryStatus::Sent,
        }
    } else {
        DeliveryStatus::None
    };

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
        author_letter,
        author_color_class,
        reply_preview,
        reactions,
        delivery_status,
    }
}
