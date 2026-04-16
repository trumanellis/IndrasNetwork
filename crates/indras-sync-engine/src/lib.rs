//! # Indra's SyncEngine
//!
//! App layer for Indra's Network — intentions, blessings, tokens of gratitude,
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
//! let intention_id = realm.create_intention("Review doc", "Please review", None, my_id).await?;
//! ```

// Domain modules (moved from indras-network)
pub mod intention;
pub mod note;
pub mod blessing;
pub mod attention;
pub mod attention_tip;
pub mod fraud_evidence;
pub mod attention_sync;
pub mod witness_roster;
pub mod certificate;
pub mod token_of_gratitude;
pub mod token_valuation;
pub mod humanness;
pub mod sentiment;
pub mod proof_folder;
pub mod story_auth;
pub mod rehearsal;
pub mod bioregion_catalog;
pub mod profile_identity;
pub mod homepage_profile;

// SyncContent extension type
pub mod content;

// Extension traits on Realm
pub mod realm_intentions;
pub mod realm_notes;
pub mod realm_chat;
pub mod realm_blessings;
pub mod realm_attention;
pub mod realm_tokens;
pub mod realm_humanness;
pub mod realm_proof_folders;
pub mod realm_vault;
pub mod realm_team;

// Extension traits on HomeRealm
pub mod home_realm_intentions;
pub mod home_realm_notes;

// Vault sync submodule
pub mod vault;

// Team types embedded in synced-vault documents
pub mod team;

// Braided VCS submodule (rides on top of the vault)
pub mod braid;

// SyncEngine struct
pub mod sync_engine;

// Prelude
pub mod prelude;

// Re-export main types at crate root
pub use intention::{Intention, IntentionKind, ServiceClaim, IntentionDocument, IntentionError, IntentionId, IntentionPriority};
pub use note::{Note, NoteDocument, NoteId};
pub use blessing::{Blessing, BlessingDocument, BlessingError, BlessingId, ClaimId};
pub use attention::{
    AttentionDocument, AttentionError, AttentionEventId, AttentionSwitchEvent, IntentionAttention,
};
pub use attention_tip::{AttentionTip, AttentionTipDocument};
pub use attention_sync::{
    ChainGap, EventFinality, classify_event_finality, current_attention_targets,
    filter_slashed_events, is_slashed, reconstruct_attention_state, sync_attention_chains,
};
pub use fraud_evidence::{FraudEvidenceDocument, FraudRecord};
pub use witness_roster::WitnessRosterDocument;
pub use certificate::CertificateDocument;
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
pub use profile_identity::ProfileIdentityDocument;
pub use homepage_profile::{HomepageProfileDocument, HomepageField};
pub use content::SyncContent;
pub use sync_engine::SyncEngine;

// Explicit DocumentSchema impls for indras-sync-engine types (default merge = replacement).
// IntentionDocument, NoteDocument, and ProofFolderDocument have manual impls with
// set-union merge semantics (see their respective modules).
indras_network::impl_document_schema!(
    BlessingDocument,
    TokenOfGratitudeDocument,
    HumannessDocument,
);

// Custom CRDT merge implementations for attention-ledger documents.
// These override the default replacement merge with proper union/max semantics.

impl indras_network::document::DocumentSchema for AttentionDocument {
    fn merge(&mut self, remote: Self) {
        // Delegate to the inherent union-merge (takes &Self, so borrow remote).
        AttentionDocument::merge(self, &remote);
    }
}

impl indras_network::document::DocumentSchema for AttentionTipDocument {
    fn merge(&mut self, remote: Self) {
        // Per-author max-seq wins.
        AttentionTipDocument::merge(self, remote);
    }
}

impl indras_network::document::DocumentSchema for FraudEvidenceDocument {
    fn merge(&mut self, remote: Self) {
        // Union of fraud records by (author, seq).
        FraudEvidenceDocument::merge(self, remote);
    }
}

// Re-export extension traits
pub use realm_intentions::RealmIntentions;
pub use realm_notes::RealmNotes;
pub use realm_chat::RealmChat;
pub use realm_blessings::RealmBlessings;
pub use realm_attention::{RealmAttention, WeightedIntentionAttention};
pub use realm_tokens::RealmTokens;
pub use realm_humanness::RealmHumanness;
pub use realm_proof_folders::RealmProofFolders;
pub use home_realm_intentions::HomeRealmIntentions;
pub use home_realm_notes::HomeRealmNotes;
pub use realm_vault::RealmVault;
pub use realm_team::RealmTeam;
pub use vault::Vault as VaultSync;
pub use vault::vault_document::VaultFileDocument;
pub use vault::vault_file::{ConflictRecord, UserId, VaultFile, CONFLICT_WINDOW_MS};
pub use team::{LogicalAgentId, Team};

// Re-export braid types
pub use braid::{
    detect_heal_needed, BraidDag, ChangeId, Changeset, Evidence, LocalRepo, PatchFile,
    PatchManifest, RealmBraid, RepairTask, TryLandError, VerificationFailure, VerificationRequest,
};
pub use braid::verification::run as verify;
