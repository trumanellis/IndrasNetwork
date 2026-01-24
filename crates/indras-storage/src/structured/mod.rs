//! Structured storage using redb
//!
//! This module provides queryable, mutable storage for:
//! - Peer registry (peer metadata, last seen times)
//! - Interface membership indices
//! - Sync state tracking
//! - Event indices
//!
//! Unlike the append-only log, this storage supports updates and deletions.

pub mod interface_store;
mod peer_registry;
mod sync_state;
mod tables;

pub use interface_store::{InterfaceRecord, InterfaceStore, MembershipRecord};
pub use peer_registry::{PeerRecord, PeerRegistry};
pub use sync_state::{SyncStateRecord, SyncStateStore};
pub use tables::{RedbStorage, RedbStorageConfig};
