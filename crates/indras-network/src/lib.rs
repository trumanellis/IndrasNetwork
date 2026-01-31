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

// Modules
pub mod artifact;
pub mod artifact_sharing;
pub mod attention;
pub mod blessing;
pub mod chat_message;
pub mod config;
pub mod contacts;
pub mod document;
pub mod document_registry;
pub mod error;
pub mod escape;
pub mod home_realm;
pub mod invite;
pub mod member;
pub mod message;
pub mod network;
pub mod note;
pub mod proof_folder;
pub mod quest;
pub mod read_tracker;
pub mod realm;
pub mod realm_alias;
pub mod sentiment;
pub mod stream;
pub mod token_of_gratitude;

// Re-export main types at crate root
pub use artifact::{Artifact, ArtifactDownload, ArtifactId, DownloadProgress};
pub use artifact_sharing::{
    ArtifactHash, ArtifactKey, ArtifactKeyRegistry, ArtifactTombstone, EncryptedArtifactKey,
    RecallAcknowledgment, RevocationEntry, SharedArtifact, SharingStatus, ARTIFACT_KEY_SIZE,
};
pub use attention::{
    AttentionDocument, AttentionError, AttentionEventId, AttentionSwitchEvent, QuestAttention,
};
pub use blessing::{Blessing, BlessingDocument, BlessingError, BlessingId, ClaimId};
pub use chat_message::{
    ChatMessageId, ChatMessageVersion, EditableChatMessage, EditableMessageType, RealmChatDocument,
};
pub use config::{NetworkBuilder, NetworkConfig, Preset};
pub use contacts::{ContactEntry, ContactsDocument, ContactsRealm};
pub use document::{Document, DocumentChange, DocumentSchema};
pub use error::{IndraError, Result};
pub use home_realm::{home_realm_id, HomeArtifactMetadata, HomeRealm};
pub use invite::InviteCode;
pub use member::{Member, MemberEvent, MemberId, MemberInfo};
pub use message::{Content, Message, MessageId};
pub use network::{IndrasNetwork, RealmId};
pub use note::{Note, NoteDocument, NoteId};
pub use proof_folder::{
    ProofFolder, ProofFolderArtifact, ProofFolderDocument, ProofFolderError, ProofFolderId,
    ProofFolderStatus,
};
pub use quest::{Quest, QuestClaim, QuestDocument, QuestError, QuestId, QuestPriority};
pub use read_tracker::ReadTrackerDocument;
pub use document_registry::DocumentRegistryDocument;
pub use network::{GlobalEvent, IdentityBackup};
pub use realm::Realm;
pub use realm_alias::{RealmAlias, RealmAliasDocument, MAX_ALIAS_LENGTH};
pub use sentiment::{
    RelayedSentiment, SentimentRelayDocument, SentimentView, DEFAULT_RELAY_ATTENUATION,
};
pub use token_of_gratitude::{
    TokenError, TokenEvent, TokenOfGratitude, TokenOfGratitudeDocument, TokenOfGratitudeId,
};

/// Prelude module for convenient imports.
///
/// Import this to get all the commonly used types:
///
/// ```ignore
/// use indras_network::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        Artifact, ArtifactDownload, Blessing, BlessingDocument, ClaimId, ContactsRealm, Content,
        Document, DocumentSchema, EditableChatMessage, GlobalEvent, HomeRealm, IdentityBackup,
        IndraError, IndrasNetwork, InviteCode, Member, MemberEvent, MemberInfo, Message, Note,
        NoteDocument, Preset, ProofFolder, ProofFolderArtifact, ProofFolderDocument, Quest,
        QuestDocument, QuestPriority, Realm, RealmAlias, RealmAliasDocument, RealmChatDocument,
        RealmId, Result, TokenOfGratitude, TokenOfGratitudeDocument,
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
