//! # indras-appflowy-bridge
//!
//! Bridge between AppFlowy's CollabPlugin and IndrasNetwork's P2P transport.
//!
//! This crate replaces AppFlowy's centralized cloud sync with IndrasNetwork's
//! peer-to-peer transport. Each AppFlowy document is mapped to an IndrasNetwork
//! interface via deterministic BLAKE3 hashing of (workspace_seed, object_id).
//!
//! ## Architecture
//!
//! ```text
//! AppFlowy Collab (owns yrs::Doc)
//!     |
//!     | CollabPlugin trait
//!     v
//! IndrasNetworkPlugin
//!     |-- receive_local_update() --> mpsc channel --> [bg task] --> node.send_message()
//!     |-- [bg task] <-- node.events() --> decode envelope --> apply yrs::Update to Doc
//! ```

pub mod envelope;
pub mod error;
pub mod id_mapping;
pub mod inbound;
pub mod outbound;
pub mod plugin;

// Re-exports
pub use envelope::AppFlowyEnvelope;
pub use error::BridgeError;
pub use id_mapping::{WorkspaceMapping, object_id_to_interface_id, object_id_to_key_seed};
pub use plugin::{BridgeConfig, CollabPlugin, IndrasNetworkPlugin};
