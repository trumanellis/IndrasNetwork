//! # Indra's SyncEngine
//!
//! App layer for Indra's Network — quests, blessings, tokens of gratitude,
//! attention tracking, proof-of-service, and humanness attestation.
//!
//! SyncEngine is the first app built on the Indra's Network P2P platform.
//! It holds an `Arc<IndrasNetwork>` and adds domain-specific functionality
//! via extension traits on `Realm`.
//!
//! ## Quick Start
//!
//! ```ignore
//! use indras_network::prelude::*;
//! use indras_sync_engine::prelude::*;
//!
//! // Create the network (one per device)
//! let network = Arc::new(IndrasNetwork::new("~/.myapp").await?);
//!
//! // Create the sync engine app layer
//! let engine = SyncEngine::new(Arc::clone(&network));
//!
//! // Use extension traits on Realm
//! let realm = network.create_realm("Project").await?;
//! let quest_id = realm.create_quest("Review doc", "Please review", None, my_id).await?;
//! ```

// Domain modules (moved from indras-network)
pub mod quest;
pub mod note;
pub mod message;
pub mod blessing;
pub mod attention;
pub mod token_of_gratitude;
pub mod token_valuation;
pub mod humanness;
pub mod sentiment;
pub mod proof_folder;
pub mod story_auth;
pub mod rehearsal;
pub mod bioregion_catalog;

// SyncContent extension type
pub mod content;

// Extension traits on Realm
pub mod realm_quests;
pub mod realm_notes;
pub mod realm_messages;
pub mod realm_chat;
pub mod realm_blessings;
pub mod realm_attention;
pub mod realm_tokens;
pub mod realm_humanness;
pub mod realm_proof_folders;

// Extension traits on HomeRealm
pub mod home_realm_quests;
pub mod home_realm_notes;

// SyncEngine struct
pub mod sync_engine;

// Prelude
pub mod prelude;

// Re-export main types at crate root
pub use quest::{Quest, QuestClaim, QuestDocument, QuestError, QuestId, QuestPriority};
pub use note::{Note, NoteDocument, NoteId};
pub use message::{MessageContent, MessageDocument, MessageId, StoredMessage};
pub use blessing::{Blessing, BlessingDocument, BlessingError, BlessingId, ClaimId};
pub use attention::{
    AttentionDocument, AttentionError, AttentionEventId, AttentionSwitchEvent, QuestAttention,
};
pub use token_of_gratitude::{
    TokenError, TokenEvent, TokenOfGratitude, TokenOfGratitudeDocument, TokenOfGratitudeId,
};
pub use token_valuation::{SubjectiveTokenValue, STEWARD_CHAIN_DECAY, subjective_value};
pub use humanness::{
    BioregionalLevel, Delegation, DelegationError, HumannessAttestation, HumannessDocument,
    HumannessEvent, humanness_freshness, validate_delegation_chain, FRESHNESS_DECAY_RATE,
    FRESHNESS_GRACE_DAYS,
};
pub use proof_folder::{
    ProofFolder, ProofFolderArtifact, ProofFolderDocument, ProofFolderError, ProofFolderId,
    ProofFolderStatus,
};
pub use sentiment::{
    RelayedSentiment, SentimentRelayDocument, SentimentView, DEFAULT_RELAY_ATTENUATION,
};
pub use rehearsal::RehearsalState;
pub use story_auth::{AuthResult, StoryAuth};
pub use content::SyncContent;
pub use sync_engine::SyncEngine;

// Explicit DocumentSchema impls for indras-sync-engine types (default merge = replacement).
indras_network::impl_document_schema!(
    QuestDocument,
    NoteDocument,
    MessageDocument,
    BlessingDocument,
    AttentionDocument,
    TokenOfGratitudeDocument,
    HumannessDocument,
    ProofFolderDocument,
    SentimentRelayDocument,
);

// Re-export extension traits
pub use realm_quests::RealmQuests;
pub use realm_notes::RealmNotes;
pub use realm_messages::RealmMessages;
pub use realm_chat::RealmChat;
pub use realm_blessings::RealmBlessings;
pub use realm_attention::RealmAttention;
pub use realm_tokens::RealmTokens;
pub use realm_humanness::RealmHumanness;
pub use realm_proof_folders::RealmProofFolders;
pub use home_realm_quests::HomeRealmQuests;
pub use home_realm_notes::HomeRealmNotes;
