//! Chat components for Indras Network applications.
//!
//! Provides a reusable chat panel with stream-based updates, edit/delete
//! support, message grouping, and auto-scroll.

pub mod chat_state;
pub mod chat_panel;
pub mod chat_messages;
pub mod chat_message;
pub mod chat_input;

pub use chat_panel::ChatPanel;
pub use chat_state::{ChatMessageView, ChatState, ChatStatus, ChatViewType, convert_editable_to_view};
