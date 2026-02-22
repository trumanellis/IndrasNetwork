//! Indras Chat — P2P Telegram-style chat library.
//!
//! Re-exports components, state, and bridge for embedding in other apps.

pub mod bridge;
pub mod components;
pub mod state;

/// Chat-specific CSS for embedding in host apps.
pub const CHAT_CSS: &str = include_str!("style.css");
