//! # Indras Relay
//!
//! A blind relay server for the Indras P2P mesh network.
//!
//! The relay acts as an always-on super-peer that stores and forwards
//! encrypted event data without being able to read it. It observes
//! gossip traffic and caches `InterfaceEventMessage` blobs for delivery
//! to peers that reconnect after being offline.
//!
//! ## Key Design Principles
//!
//! - **Blind**: Never receives interface keys, cannot decrypt any content
//! - **Passive observer**: Subscribes to gossip topics and stores encrypted blobs
//! - **Store-and-forward**: Delivers missed events to reconnecting peers
//! - **Optional**: Network works without relay; relay is a convenience layer
//!
//! ## Architecture
//!
//! - `RelayNode`: Core server combining transport, gossip, and storage
//! - `BlobStore`: redb-backed persistent storage for encrypted events
//! - `RegistrationState`: Tracks which peers are registered for which interfaces
//! - `QuotaManager`: Per-peer storage limits and enforcement

pub mod admin;
pub mod blob_store;
pub mod config;
pub mod error;
pub mod quota;
pub mod registration;
pub mod relay_node;

pub use config::RelayConfig;
pub use error::{RelayError, RelayResult};
pub use relay_node::RelayNode;
