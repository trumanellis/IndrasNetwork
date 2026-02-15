//! # Indra's Network SyncEngine
//!
//! High-level SyncEngine for building peer-to-peer applications on Indra's Network.
//!
//! This crate provides a simple, unified API for creating collaborative
//! applications with automatic peer discovery, CRDT-based synchronization,
//! and end-to-end encryption.
//!
//! ## Quick Start
//!
//! ```ignore
//! use indras_network::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), IndraError> {
//!     // Create a network instance
//!     let network = IndrasNetwork::new("~/.myapp").await?;
//!
//!     // Create a realm for collaboration
//!     let realm = network.create_realm("My Project").await?;
//!
//!     // Share the invite code with others
//!     println!("Invite: {}", realm.invite_code().unwrap());
//!
//!     // Send a message
//!     realm.send("Hello, world!").await?;
//!
//!     // Listen for messages
//!     use futures::StreamExt;
//!     let mut messages = realm.messages();
//!     while let Some(msg) = messages.next().await {
//!         println!("{}: {}", msg.sender.name(), msg.content.as_text().unwrap_or(""));
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Core Concepts
//!
//! ### IndrasNetwork
//!
//! The main entry point. Manages your identity, realms, and network connections.
//!
//! ### Realm
//!
//! A collaborative space where members can send messages, share documents,
//! and exchange artifacts. Realms are identified by invite codes that can
//! be shared to allow others to join.
//!
//! ### Document
//!
//! A CRDT-backed data structure that automatically synchronizes across all
//! realm members. Documents support typed schemas and provide reactive
//! updates when data changes.
//!
//! ### Artifact
//!
//! Static, immutable content (files) shared within a realm. Artifacts are
//! content-addressed by their hash and can be downloaded by any member.
//!
//! ## Configuration
//!
//! The SyncEngine supports preset configurations for common use cases:
//!
//! ```ignore
//! // For chat applications
//! let network = IndrasNetwork::preset(Preset::Chat)
//!     .data_dir("~/.mychat")
//!     .build()
//!     .await?;
//!
//! // For collaborative documents
//! let network = IndrasNetwork::preset(Preset::Collaboration)
//!     .data_dir("~/.mydocs")
//!     .build()
//!     .await?;
//!
//! // For IoT devices
//! let network = IndrasNetwork::preset(Preset::IoT)
//!     .data_dir("/var/lib/device")
//!     .build()
//!     .await?;
//! ```
//!
//! ## Escape Hatches
//!
//! For advanced users who need direct access to the underlying infrastructure,
//! the `escape` module provides access to low-level types:
//!
//! ```ignore
//! use indras_network::escape::*;
//!
//! // Access the underlying node
//! let node = network.node();
//!
//! // Access raw storage
//! let storage = network.storage();
//! ```

// Modules — generic P2P platform SDK
pub mod artifact;
pub mod access;
pub mod encryption;
pub mod artifact_index;
pub mod artifact_recovery;
pub mod chat_message;
pub mod config;
pub mod contacts;
pub mod direct_connect;
pub mod document;
pub mod document_registry;
pub mod encounter;
pub mod error;
pub mod escape;
pub mod home_realm;
pub mod identity_code;
pub mod invite;
pub mod member;
pub mod message;
pub mod network;
pub mod read_tracker;
pub mod realm;
pub mod realm_alias;
pub mod stream;
pub mod world_view;

// Re-export main types at crate root
pub use artifact::{Artifact, ArtifactDownload, ArtifactId, DownloadProgress};
pub use encryption::{ArtifactKey, EncryptedArtifactKey, ARTIFACT_KEY_SIZE};
pub use access::{
    AccessGrant, AccessMode, ArtifactProvenance, ArtifactStatus, GrantError, HolonicError,
    ProvenanceType, RevokeError, TransferError,
};
pub use artifact_index::{ArtifactIndex, HomeArtifactEntry};
pub use artifact_recovery::{ArtifactRecoveryRequest, ArtifactRecoveryResponse, RecoverableArtifact, RecoveryManifest};
pub use chat_message::{
    ChatMessageId, ChatMessageVersion, EditableChatMessage, EditableMessageType, RealmChatDocument,
};
pub use config::{NetworkBuilder, NetworkConfig, Preset};
pub use contacts::{ContactEntry, ContactStatus, ContactsDocument, ContactsRealm};
pub use direct_connect::{dm_realm_id, KeyExchangeStatus, PendingKeyExchange};
pub use encounter::{EncounterExchangePayload, EncounterHandle};
pub use identity_code::IdentityCode;
pub use document::{Document, DocumentChange, DocumentSchema};
pub use error::{IndraError, Result};
pub use home_realm::{home_realm_id, HomeArtifactMetadata, HomeRealm};
pub use invite::InviteCode;
pub use member::{Member, MemberEvent, MemberId, MemberInfo};
pub use message::{Content, Message, MessageId};
pub use network::{IndrasNetwork, RealmId};
pub use read_tracker::ReadTrackerDocument;
pub use document_registry::DocumentRegistryDocument;
pub use network::{GlobalEvent, IdentityBackup};
pub use realm::Realm;
pub use realm_alias::{RealmAlias, RealmAliasDocument, MAX_ALIAS_LENGTH};
pub use world_view::WorldView;

/// Prelude module for convenient imports.
///
/// Import this to get all the commonly used types:
///
/// ```ignore
/// use indras_network::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        Artifact, ArtifactDownload, ArtifactIndex, HomeArtifactEntry,
        ContactsRealm, Content, Document, DocumentSchema, EditableChatMessage, GlobalEvent,
        HomeRealm, IdentityBackup, IdentityCode, IndraError, IndrasNetwork, InviteCode, Member,
        MemberEvent, MemberInfo, Message, Preset, Realm, RealmAlias, RealmAliasDocument,
        RealmChatDocument, RealmId, Result,
    };

    // Re-export futures StreamExt for convenient stream iteration
    pub use futures::StreamExt;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prelude_imports() {
        // Just verify the prelude compiles
        use crate::prelude::*;
    }
}
