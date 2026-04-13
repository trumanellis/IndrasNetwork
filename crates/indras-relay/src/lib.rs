//! # Indras Relay
//!
//! An authenticated, profile-connected relay node for the Indras P2P mesh network.
//!
//! The relay acts as an always-on super-peer that stores and forwards
//! encrypted event data without being able to read it. It uses a three-tier
//! staging model matching the Indras social model:
//!
//! - **Self tier**: Owner's own data (backup, pinning, cross-device sync)
//! - **Connections tier**: Mutual peer data (realm sync, encrypted S&F, custody)
//! - **Public tier**: Network broadcast (announcements, discovery)
//!
//! ## Key Design Principles
//!
//! - **Blind**: Never receives interface keys, cannot decrypt any content
//! - **Authenticated**: Peers must present signed credentials linking transport identity to profile
//! - **Tiered storage**: Three tiers with independent quotas, TTLs, and access controls
//! - **Hybrid mode**: Same binary serves as personal server or community server via config
//!
//! ## Architecture
//!
//! - `RelayNode`: Core server combining transport, gossip, auth, and storage
//! - `AuthService`: Credential validation and session tracking
//! - `BlobStore`: redb-backed persistent storage with per-tier tables
//! - `RegistrationState`: Tracks which peers are registered for which interfaces
//! - `QuotaManager` / `TieredQuotaManager`: Per-peer and per-tier storage limits
//! - `tier`: Tier determination logic mapping players to access levels

pub mod admin;
pub mod auth;
pub mod blob_store;
pub mod config;
pub mod error;
pub mod quota;
pub mod registration;
pub mod relay_node;
pub mod tier;

#[cfg(feature = "homepage")]
pub mod blob_homepage;

pub use auth::AuthService;
pub use config::{QuotaConfig, RelayConfig, StorageConfig, TierConfig};
pub use error::{RelayError, RelayResult};
pub use quota::{PeerQuota, QuotaManager, TieredQuotaManager};
pub use registration::{PeerRegistrationInfo, RegistrationState};
pub use admin::{
    QuotaConfigPatch, RelayConfigPatch, RelayConfigView, StorageConfigPatch, TierConfigPatch,
};
pub use relay_node::{RelayNode, RelayService};
