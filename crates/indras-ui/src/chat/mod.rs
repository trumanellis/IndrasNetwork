//! Chat components for Indras Network applications.
//!
//! Provides a reusable chat panel with Telegram-style bubble layout,
//! reply threading, emoji reactions, typing indicators, and read receipts.

pub mod chat_state;
pub mod chat_panel;
pub mod chat_messages;
pub mod chat_message;
pub mod chat_bubble;
pub mod chat_input;

pub use chat_panel::ChatPanel;
pub use chat_state::{ChatMessageView, ChatState, ChatStatus, ChatViewType, ReplyPreview, ReactionView, DeliveryStatus, TypingPeerView, convert_editable_to_view};
