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
pub use event::{PeerEvent, PeerInfo};
pub use runtime::PeeringRuntime;

/// Convenience alias for peering results.
pub type Result<T> = std::result::Result<T, PeeringError>;
