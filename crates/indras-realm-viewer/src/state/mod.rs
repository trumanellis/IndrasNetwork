//! State management for Realm Viewer
//!
//! Provides reactive state tracking for realms, intentions, attention, contacts, chat, artifacts, and proof folders.

pub mod app_state;
pub mod artifact_state;
pub mod attention_state;
pub mod chat_state;
pub mod contacts_state;
pub mod document_state;
pub mod intention_state;
pub mod proof_folder_state;
pub mod realm_state;
pub mod token_state;

pub use app_state::*;
pub use artifact_state::*;
pub use attention_state::*;
pub use chat_state::*;
pub use contacts_state::*;
pub use document_state::*;
pub use intention_state::*;
pub use proof_folder_state::*;
pub use realm_state::*;
pub use token_state::*;
