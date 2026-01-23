//! Structured storage using redb
//!
//! This module provides queryable, mutable storage for:
//! - Peer registry (peer metadata, last seen times)
//! - Interface membership indices
//! - Sync state tracking
//! - Event indices
//!
//! Unlike the append-only log, this storage supports updates and deletions.

mod tables;
mod peer_registry;
pub mod interface_store;
mod sync_state;

pub use tables::{RedbStorage, RedbStorageConfig};
pub use peer_registry::{PeerRecord, PeerRegistry};
pub use interface_store::{InterfaceRecord, InterfaceStore, MembershipRecord};
pub use sync_state::{SyncStateRecord, SyncStateStore};
