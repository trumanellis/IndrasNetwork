//! # Indras Artifacts
//!
//! Domain types for the Indra's Network artifact/attention economy.
//!
//! This crate defines the core data model shared across the entire stack:
//! artifacts, access control, attention tracking, peering, and token valuation.
//! It contains no networking logic — only pure data structures and traits.
//!
//! ## Key Types
//!
//! - [`ArtifactId`]: Content-addressed (`Blob`) or document-addressed (`Doc`) identifier
//! - [`Artifact`]: Unified artifact with optional payload, references, and metadata
//! - [`AccessMode`]: `Revocable`, `Permanent`, `Timed`, or `Transfer`
//! - [`Vault`] / [`Story`] / [`Exchange`] / [`Request`] / [`Intention`]: High-level artifact containers
//! - [`AttentionLog`] / [`compute_heat`]: Attention tracking and heat computation
//! - [`ArtifactStore`] / [`PayloadStore`] / [`AttentionStore`]: Storage traits
//!
//! ## Architecture
//!
//! Artifacts form a soft DAG via forward references. Any artifact can have
//! content (payload), references to other artifacts, both, or neither.
//! "Dimension" is emergent, not enforced.
//!
//! ```text
//! Vault (top-level, one per user)
//! ├── Story (narrative thread)
//! │   ├── Leaf: Message, Image, File, Token, Attestation
//! │   └── Gallery
//! ├── Exchange (trade/gift)
//! ├── Request (ask for artifacts)
//! └── Intention (goal with proofs + attention tokens)
//! ```
//!
//! Artifact IDs are BLAKE3 hashes of content (Blob) or random/deterministic (Doc).
//! `dm_story_id(A, B)` is symmetric — both peers derive the same ID.

pub mod access;
pub mod artifact;
pub mod attention;
pub mod error;
pub mod exchange;
pub mod intention;
pub mod peering;
pub mod request;
pub mod store;
pub mod story;
pub mod token;
pub mod vault;

pub use access::{AccessGrant, AccessMode, ArtifactProvenance, ArtifactStatus, ProvenanceType};
pub use artifact::*;
pub use attention::{AttentionLog, AttentionSwitchEvent, AttentionValue, DwellWindow, compute_heat, extract_dwell_windows};
pub use error::VaultError;
pub use exchange::Exchange;
pub use intention::Intention;
pub use peering::{MutualPeering, PeerEntry, PeerRegistry};
pub use request::Request;
pub use store::{
    ArtifactStore, AttentionStore, InMemoryArtifactStore, InMemoryAttentionStore,
    InMemoryPayloadStore, IntegrityResult, PayloadStore,
};
pub use story::Story;
pub use token::compute_token_value;
pub use vault::Vault;
