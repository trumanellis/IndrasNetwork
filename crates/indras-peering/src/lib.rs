//! Indras Peering — reusable P2P peering lifecycle.
//!
//! Extracts the network lifecycle (contact polling, peer events, world view saves,
//! graceful shutdown) into a UI-agnostic crate. Both standalone `indras-chat` and
//! embedded `indras-workspace` create a `PeeringRuntime` and pass it around.
//!
//! # Two construction modes
//!
//! - **`boot(config)`** — Standalone: creates `IndrasNetwork`, starts it, spawns tasks.
//! - **`attach(network, config)`** — Embedded: wraps an existing started network.

mod config;
mod error;
mod event;
mod runtime;
mod tasks;

pub use config::PeeringConfig;
pub use error::PeeringError;
pub use event::{ContactStatus, PeerEvent, PeerInfo};
pub use runtime::PeeringRuntime;

// Re-export MemberId for consumer convenience
pub use indras_network::MemberId;

// Re-export contact/sentiment types so apps don't need lower-layer deps
pub use indras_network::contacts::ContactEntry;
pub use indras_sync_engine::sentiment::{
    RelayedSentiment, SentimentRelayDocument, SentimentView, DEFAULT_RELAY_ATTENUATION,
};

/// Convenience alias for peering results.
pub type Result<T> = std::result::Result<T, PeeringError>;
