//! # Indra's Vault Sync
//!
//! P2P Obsidian vault synchronization over Indra's Network.
//!
//! Each vault maps to a Realm. Files are tracked in a CRDT document
//! (`VaultFileDocument`) with LWW-per-file merge and conflict detection
//! for concurrent edits within a configurable time window.
//!
//! ## Quick Start
//!
//! ```ignore
//! use indras_vault_sync::prelude::*;
//!
//! let (vault, invite) = Vault::create(&network, "My Vault", "/path/to/vault".into()).await?;
//! vault.initial_scan().await?;
//!
//! // On another device:
//! let vault2 = Vault::join(&network, &invite.to_string(), "/path/to/vault2".into()).await?;
//! ```

pub mod realm_vault;
pub mod relay_sync;
pub mod sync_to_disk;
pub mod vault;
pub mod vault_document;
pub mod vault_file;
pub mod watcher;

pub mod prelude {
    pub use crate::realm_vault::RealmVault;
    pub use crate::vault::Vault;
    pub use crate::vault_document::VaultFileDocument;
    pub use crate::vault_file::{ConflictRecord, UserId, VaultFile, CONFLICT_WINDOW_MS};
}

pub use realm_vault::RealmVault;
pub use vault::Vault;
pub use vault_document::VaultFileDocument;
pub use vault_file::{ConflictRecord, UserId, VaultFile, CONFLICT_WINDOW_MS};
