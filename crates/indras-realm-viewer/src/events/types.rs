//! Event type definitions for streaming from Lua scenarios
//!
//! These types match the JSONL output from SDK stress test scenarios.

use serde::Deserialize;

/// All possible events streamed from Lua scenarios
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event_type")]
pub enum StreamEvent {
    // ========== Realm Events ==========
    #[serde(rename = "realm_created")]
    RealmCreated {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        #[serde(default)]
        members: String,
        #[serde(default)]
        member_count: u32,
    },

    #[serde(rename = "member_joined")]
    MemberJoined {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        member: String,
    },

    #[serde(rename = "member_left")]
    MemberLeft {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        member: String,
    },

    #[serde(rename = "realm_alias_set")]
    RealmAliasSet {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        member: String,
        alias: String,
    },

    #[serde(rename = "profile_updated")]
    ProfileUpdated {
        #[serde(default)]
        tick: u32,
        member: String,
        #[serde(default)]
        headline: Option<String>,
        #[serde(default)]
        bio: Option<String>,
    },

    // ========== Quest Events ==========
    #[serde(rename = "quest_created")]
    QuestCreated {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        creator: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        latency_us: f64,
    },

    #[serde(rename = "quest_claim_submitted")]
    QuestClaimSubmitted {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        claimant: String,
        #[serde(default)]
        claim_index: usize,
        #[serde(default)]
        proof_artifact: Option<String>,
    },

    #[serde(rename = "quest_claim_verified")]
    QuestClaimVerified {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        #[serde(default)]
        claim_index: usize,
    },

    #[serde(rename = "quest_completed")]
    QuestCompleted {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        #[serde(default)]
        verified_claims: usize,
        #[serde(default)]
        pending_claims: usize,
    },

    // ========== Attention Events ==========
    #[serde(rename = "attention_switched")]
    AttentionSwitched {
        #[serde(default)]
        tick: u32,
        member: String,
        quest_id: String,
        #[serde(default)]
        event_id: Option<String>,
        #[serde(default)]
        latency_us: f64,
    },

    #[serde(rename = "attention_cleared")]
    AttentionCleared {
        #[serde(default)]
        tick: u32,
        member: String,
        #[serde(default)]
        clear_worked: bool,
    },

    #[serde(rename = "attention_calculated")]
    AttentionCalculated {
        #[serde(default)]
        tick: u32,
        #[serde(default)]
        quest_count: usize,
        #[serde(default)]
        latency_us: f64,
    },

    #[serde(rename = "ranking_verified")]
    RankingVerified {
        #[serde(default)]
        tick: u32,
        #[serde(default)]
        ranking_valid: bool,
        #[serde(default)]
        ranking_consistent: bool,
        #[serde(default)]
        top_quest: Option<String>,
        #[serde(default)]
        top_attention_ms: u64,
    },

    // ========== Contacts Events ==========
    #[serde(rename = "contact_added")]
    ContactAdded {
        #[serde(default)]
        tick: u32,
        member: String,
        contact: String,
    },

    #[serde(rename = "contact_removed")]
    ContactRemoved {
        #[serde(default)]
        tick: u32,
        member: String,
        contact: String,
    },

    #[serde(rename = "sentiment_updated")]
    SentimentUpdated {
        #[serde(default)]
        tick: u32,
        member: String,
        contact: String,
        /// -1 = don't recommend, 0 = neutral, 1 = recommend
        sentiment: i8,
    },

    #[serde(rename = "contact_blocked")]
    ContactBlocked {
        #[serde(default)]
        tick: u32,
        member: String,
        contact: String,
        /// Realm IDs that were left as part of the blocking cascade.
        #[serde(default)]
        realms_left: Vec<String>,
    },

    #[serde(rename = "relayed_sentiment_received")]
    RelayedSentimentReceived {
        #[serde(default)]
        tick: u32,
        /// The member whose view is being updated.
        member: String,
        /// The member this sentiment is about.
        about: String,
        /// The sentiment value.
        sentiment: i8,
        /// Which contact relayed this signal.
        via: String,
    },

    // ========== Chat Events ==========
    #[serde(rename = "chat_message")]
    ChatMessage {
        #[serde(default)]
        tick: u32,
        member: String,
        content: String,
        #[serde(default)]
        message_type: String,
        #[serde(default)]
        message_id: Option<String>,
        #[serde(default)]
        realm_id: Option<String>,
    },

    #[serde(rename = "chat_message_edited")]
    ChatMessageEdited {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        message_id: String,
        member: String,
        old_content: String,
        new_content: String,
    },

    #[serde(rename = "chat_message_deleted")]
    ChatMessageDeleted {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        message_id: String,
        member: String,
    },

    /// Image shared inline in chat
    #[serde(rename = "chat_image")]
    ChatImage {
        #[serde(default)]
        tick: u32,
        member: String,
        /// MIME type (image/png, image/jpeg, etc.)
        mime_type: String,
        /// Base64-encoded image data (for embedded images)
        #[serde(default)]
        data: Option<String>,
        /// Artifact hash (for large images stored as artifacts)
        #[serde(default)]
        artifact_hash: Option<String>,
        /// Original filename
        #[serde(default)]
        filename: Option<String>,
        /// Image dimensions (width, height)
        #[serde(default)]
        dimensions: Option<(u32, u32)>,
        /// Alt text / caption
        #[serde(default)]
        alt_text: Option<String>,
        /// Local asset path for viewer testing
        #[serde(default)]
        asset_path: Option<String>,
        /// Optional message ID
        #[serde(default)]
        message_id: Option<String>,
    },

    /// Gallery of images/videos/files shared in chat
    #[serde(rename = "chat_gallery")]
    ChatGallery {
        #[serde(default)]
        tick: u32,
        member: String,
        /// Unique folder identifier
        folder_id: String,
        /// Gallery title
        #[serde(default)]
        title: Option<String>,
        /// Items in the gallery
        #[serde(default)]
        items: Vec<GalleryEventItem>,
        /// Optional message ID
        #[serde(default)]
        message_id: Option<String>,
    },

    // ========== Proof/Blessing Events ==========
    #[serde(rename = "proof_submitted")]
    ProofSubmitted {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        claimant: String,
        #[serde(default)]
        quest_title: String,
        artifact_id: String,
        #[serde(default)]
        artifact_name: String,
    },

    #[serde(rename = "blessing_given")]
    BlessingGiven {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        claimant: String,
        blesser: String,
        #[serde(default)]
        event_count: usize,
        #[serde(default)]
        attention_millis: u64,
    },

    #[serde(rename = "proof_folder_submitted")]
    ProofFolderSubmitted {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        claimant: String,
        folder_id: String,
        #[serde(default)]
        artifact_count: usize,
        #[serde(default)]
        narrative_preview: String,
        /// Quest title for display
        #[serde(default)]
        quest_title: String,
        /// Full markdown narrative
        #[serde(default)]
        narrative: String,
        /// Artifacts in the proof folder
        #[serde(default)]
        artifacts: Vec<ProofArtifactItem>,
    },

    // ========== Token of Gratitude Events ==========
    /// Token of gratitude minted from a blessing
    #[serde(rename = "token_minted")]
    TokenMinted {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        token_id: String,
        steward: String,
        #[serde(default)]
        value_millis: u64,
        blesser: String,
        source_quest_id: String,
    },

    /// Gratitude pledged to a quest as a bounty
    #[serde(rename = "gratitude_pledged")]
    GratitudePledged {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        token_id: String,
        pledger: String,
        target_quest_id: String,
        #[serde(default)]
        amount_millis: u64,
    },

    /// Gratitude released to a proof submitter (steward transfer)
    #[serde(rename = "gratitude_released")]
    GratitudeReleased {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        token_id: String,
        from_steward: String,
        to_steward: String,
        target_quest_id: String,
        #[serde(default)]
        amount_millis: u64,
    },

    /// Gratitude pledge withdrawn by the steward
    #[serde(rename = "gratitude_withdrawn")]
    GratitudeWithdrawn {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        token_id: String,
        steward: String,
        target_quest_id: String,
        #[serde(default)]
        amount_millis: u64,
    },

    // ========== Artifact Sharing Events ==========
    /// Artifact shared with revocation support
    #[serde(rename = "artifact_shared_revocable")]
    ArtifactSharedRevocable {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        /// BLAKE3 hash of encrypted content (hex)
        artifact_hash: String,
        name: String,
        #[serde(default)]
        size: u64,
        #[serde(default)]
        mime_type: Option<String>,
        sharer: String,
        /// Local file path for real assets (optional)
        #[serde(default)]
        asset_path: Option<String>,
    },

    /// Artifact recalled - delete immediately
    #[serde(rename = "artifact_recalled")]
    ArtifactRecalled {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        /// BLAKE3 hash of artifact being revoked (hex)
        artifact_hash: String,
        /// Who is revoking (must be original sharer)
        revoked_by: String,
    },

    /// Acknowledgment that recall was processed
    #[serde(rename = "recall_acknowledged")]
    RecallAcknowledged {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        /// Hash of the recalled artifact (hex)
        artifact_hash: String,
        /// Member who acknowledged
        acknowledged_by: String,
        /// Whether the local blob was deleted
        #[serde(default)]
        blob_deleted: bool,
        /// Whether the key was removed
        #[serde(default)]
        key_removed: bool,
    },

    // ========== Proof Folder Lifecycle Events ==========
    /// Proof folder created (draft state)
    #[serde(rename = "proof_folder_created")]
    ProofFolderCreated {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        quest_id: String,
        folder_id: String,
        claimant: String,
        #[serde(default)]
        status: String,
    },

    /// Proof folder narrative updated
    #[serde(rename = "proof_folder_narrative_updated")]
    ProofFolderNarrativeUpdated {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        folder_id: String,
        claimant: String,
        #[serde(default)]
        narrative_length: usize,
        /// Full markdown narrative text (for V2 rendered preview)
        #[serde(default)]
        narrative: String,
    },

    /// Artifact added to proof folder
    #[serde(rename = "proof_folder_artifact_added")]
    ProofFolderArtifactAdded {
        #[serde(default)]
        tick: u32,
        realm_id: String,
        folder_id: String,
        artifact_id: String,
        #[serde(default)]
        artifact_name: String,
        #[serde(default)]
        artifact_size: u64,
        #[serde(default)]
        mime_type: String,
        /// Local asset path for viewer testing (images/videos)
        #[serde(default)]
        asset_path: Option<String>,
        /// Caption / alt text
        #[serde(default)]
        caption: Option<String>,
    },

    // ========== CRDT Events ==========
    /// CRDT converged across members
    #[serde(rename = "crdt_converged")]
    CrdtConverged {
        #[serde(default)]
        tick: u32,
        folder_id: String,
        #[serde(default)]
        members_synced: usize,
    },

    /// CRDT conflict detected
    #[serde(rename = "crdt_conflict")]
    CrdtConflict {
        #[serde(default)]
        tick: u32,
        folder_id: String,
        #[serde(default)]
        expected_members: usize,
        #[serde(default)]
        actual_members: usize,
    },

    // ========== Document Events ==========
    /// Document edited via CRDT
    #[serde(rename = "document_edit")]
    DocumentEdit {
        #[serde(default)]
        tick: u32,
        /// Artifact hash identifying the document
        document_id: String,
        /// Member who made the edit
        editor: String,
        /// Full document content after edit
        content: String,
        /// Realm where the document lives
        #[serde(default)]
        realm_id: Option<String>,
    },

    // ========== Info/Log Events ==========
    #[serde(rename = "info")]
    Info {
        #[serde(default)]
        tick: u32,
        #[serde(default)]
        message: String,
        #[serde(default)]
        phase: Option<u32>,
    },

    /// Catch-all for unknown event types
    #[serde(other)]
    Unknown,
}

impl StreamEvent {
    /// Get the tick number for this event
    pub fn tick(&self) -> u32 {
        match self {
            StreamEvent::RealmCreated { tick, .. } => *tick,
            StreamEvent::MemberJoined { tick, .. } => *tick,
            StreamEvent::MemberLeft { tick, .. } => *tick,
            StreamEvent::RealmAliasSet { tick, .. } => *tick,
            StreamEvent::ProfileUpdated { tick, .. } => *tick,
            StreamEvent::QuestCreated { tick, .. } => *tick,
            StreamEvent::QuestClaimSubmitted { tick, .. } => *tick,
            StreamEvent::QuestClaimVerified { tick, .. } => *tick,
            StreamEvent::QuestCompleted { tick, .. } => *tick,
            StreamEvent::AttentionSwitched { tick, .. } => *tick,
            StreamEvent::AttentionCleared { tick, .. } => *tick,
            StreamEvent::AttentionCalculated { tick, .. } => *tick,
            StreamEvent::RankingVerified { tick, .. } => *tick,
            StreamEvent::ContactAdded { tick, .. } => *tick,
            StreamEvent::ContactRemoved { tick, .. } => *tick,
            StreamEvent::SentimentUpdated { tick, .. } => *tick,
            StreamEvent::ContactBlocked { tick, .. } => *tick,
            StreamEvent::RelayedSentimentReceived { tick, .. } => *tick,
            StreamEvent::ChatMessage { tick, .. } => *tick,
            StreamEvent::ChatMessageEdited { tick, .. } => *tick,
            StreamEvent::ChatMessageDeleted { tick, .. } => *tick,
            StreamEvent::ChatImage { tick, .. } => *tick,
            StreamEvent::ChatGallery { tick, .. } => *tick,
            StreamEvent::ProofSubmitted { tick, .. } => *tick,
            StreamEvent::BlessingGiven { tick, .. } => *tick,
            StreamEvent::ProofFolderSubmitted { tick, .. } => *tick,
            StreamEvent::ProofFolderCreated { tick, .. } => *tick,
            StreamEvent::ProofFolderNarrativeUpdated { tick, .. } => *tick,
            StreamEvent::ProofFolderArtifactAdded { tick, .. } => *tick,
            StreamEvent::CrdtConverged { tick, .. } => *tick,
            StreamEvent::CrdtConflict { tick, .. } => *tick,
            StreamEvent::TokenMinted { tick, .. } => *tick,
            StreamEvent::GratitudePledged { tick, .. } => *tick,
            StreamEvent::GratitudeReleased { tick, .. } => *tick,
            StreamEvent::GratitudeWithdrawn { tick, .. } => *tick,
            StreamEvent::ArtifactSharedRevocable { tick, .. } => *tick,
            StreamEvent::ArtifactRecalled { tick, .. } => *tick,
            StreamEvent::RecallAcknowledged { tick, .. } => *tick,
            StreamEvent::DocumentEdit { tick, .. } => *tick,
            StreamEvent::Info { tick, .. } => *tick,
            StreamEvent::Unknown => 0,
        }
    }

    /// Get a short description of the event type
    pub fn event_type_name(&self) -> &'static str {
        match self {
            StreamEvent::RealmCreated { .. } => "realm_created",
            StreamEvent::MemberJoined { .. } => "member_joined",
            StreamEvent::MemberLeft { .. } => "member_left",
            StreamEvent::RealmAliasSet { .. } => "realm_alias_set",
            StreamEvent::ProfileUpdated { .. } => "profile_updated",
            StreamEvent::QuestCreated { .. } => "quest_created",
            StreamEvent::QuestClaimSubmitted { .. } => "claim_submitted",
            StreamEvent::QuestClaimVerified { .. } => "claim_verified",
            StreamEvent::QuestCompleted { .. } => "quest_completed",
            StreamEvent::AttentionSwitched { .. } => "attention_switched",
            StreamEvent::AttentionCleared { .. } => "attention_cleared",
            StreamEvent::AttentionCalculated { .. } => "attention_calculated",
            StreamEvent::RankingVerified { .. } => "ranking_verified",
            StreamEvent::ContactAdded { .. } => "contact_added",
            StreamEvent::ContactRemoved { .. } => "contact_removed",
            StreamEvent::SentimentUpdated { .. } => "sentiment_updated",
            StreamEvent::ContactBlocked { .. } => "contact_blocked",
            StreamEvent::RelayedSentimentReceived { .. } => "relayed_sentiment",
            StreamEvent::ChatMessage { .. } => "chat_message",
            StreamEvent::ChatMessageEdited { .. } => "chat_message_edited",
            StreamEvent::ChatMessageDeleted { .. } => "chat_message_deleted",
            StreamEvent::ChatImage { .. } => "chat_image",
            StreamEvent::ChatGallery { .. } => "chat_gallery",
            StreamEvent::ProofSubmitted { .. } => "proof_submitted",
            StreamEvent::BlessingGiven { .. } => "blessing_given",
            StreamEvent::ProofFolderSubmitted { .. } => "proof_folder_submitted",
            StreamEvent::ProofFolderCreated { .. } => "proof_folder_created",
            StreamEvent::ProofFolderNarrativeUpdated { .. } => "narrative_updated",
            StreamEvent::ProofFolderArtifactAdded { .. } => "artifact_added",
            StreamEvent::CrdtConverged { .. } => "crdt_converged",
            StreamEvent::CrdtConflict { .. } => "crdt_conflict",
            StreamEvent::TokenMinted { .. } => "token_minted",
            StreamEvent::GratitudePledged { .. } => "gratitude_pledged",
            StreamEvent::GratitudeReleased { .. } => "gratitude_released",
            StreamEvent::GratitudeWithdrawn { .. } => "gratitude_withdrawn",
            StreamEvent::ArtifactSharedRevocable { .. } => "artifact_shared_revocable",
            StreamEvent::ArtifactRecalled { .. } => "artifact_recalled",
            StreamEvent::RecallAcknowledged { .. } => "recall_acknowledged",
            StreamEvent::DocumentEdit { .. } => "document_edit",
            StreamEvent::Info { .. } => "info",
            StreamEvent::Unknown => "unknown",
        }
    }

    /// Get the category of this event for filtering
    pub fn category(&self) -> EventCategory {
        match self {
            StreamEvent::RealmCreated { .. }
            | StreamEvent::MemberJoined { .. }
            | StreamEvent::MemberLeft { .. }
            | StreamEvent::RealmAliasSet { .. }
            | StreamEvent::ProfileUpdated { .. } => EventCategory::Realm,

            StreamEvent::QuestCreated { .. }
            | StreamEvent::QuestClaimSubmitted { .. }
            | StreamEvent::QuestClaimVerified { .. }
            | StreamEvent::QuestCompleted { .. } => EventCategory::Quest,

            StreamEvent::AttentionSwitched { .. }
            | StreamEvent::AttentionCleared { .. }
            | StreamEvent::AttentionCalculated { .. }
            | StreamEvent::RankingVerified { .. } => EventCategory::Attention,

            StreamEvent::ContactAdded { .. }
            | StreamEvent::ContactRemoved { .. }
            | StreamEvent::SentimentUpdated { .. }
            | StreamEvent::ContactBlocked { .. }
            | StreamEvent::RelayedSentimentReceived { .. } => EventCategory::Contacts,

            StreamEvent::ChatMessage { .. }
            | StreamEvent::ChatMessageEdited { .. }
            | StreamEvent::ChatMessageDeleted { .. }
            | StreamEvent::ChatImage { .. }
            | StreamEvent::ChatGallery { .. } => EventCategory::Chat,

            StreamEvent::ProofSubmitted { .. }
            | StreamEvent::BlessingGiven { .. }
            | StreamEvent::ProofFolderSubmitted { .. }
            | StreamEvent::ProofFolderCreated { .. }
            | StreamEvent::ProofFolderNarrativeUpdated { .. }
            | StreamEvent::ProofFolderArtifactAdded { .. }
            | StreamEvent::TokenMinted { .. }
            | StreamEvent::GratitudePledged { .. }
            | StreamEvent::GratitudeReleased { .. }
            | StreamEvent::GratitudeWithdrawn { .. } => EventCategory::Blessing,

            StreamEvent::ArtifactSharedRevocable { .. }
            | StreamEvent::ArtifactRecalled { .. }
            | StreamEvent::RecallAcknowledged { .. }
            | StreamEvent::DocumentEdit { .. } => EventCategory::Artifact,

            StreamEvent::CrdtConverged { .. }
            | StreamEvent::CrdtConflict { .. }
            | StreamEvent::Info { .. }
            | StreamEvent::Unknown => EventCategory::Info,
        }
    }
}

/// Event categories for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    Realm,
    Quest,
    Attention,
    Contacts,
    Chat,
    Blessing,
    Artifact,
    Info,
}

impl EventCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            EventCategory::Realm => "Realm",
            EventCategory::Quest => "Quest",
            EventCategory::Attention => "Attention",
            EventCategory::Contacts => "Contacts",
            EventCategory::Chat => "Chat",
            EventCategory::Blessing => "Blessing",
            EventCategory::Artifact => "Artifact",
            EventCategory::Info => "Info",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            EventCategory::Realm => "event-realm",
            EventCategory::Quest => "event-quest",
            EventCategory::Attention => "event-attention",
            EventCategory::Contacts => "event-contacts",
            EventCategory::Chat => "event-chat",
            EventCategory::Blessing => "event-blessing",
            EventCategory::Artifact => "event-artifact",
            EventCategory::Info => "event-info",
        }
    }
}

/// Item in a gallery event
#[derive(Debug, Clone, Deserialize)]
pub struct GalleryEventItem {
    /// Filename
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// Size in bytes
    #[serde(default)]
    pub size: u64,
    /// Base64-encoded thumbnail (for images)
    #[serde(default)]
    pub thumbnail_data: Option<String>,
    /// Text preview (first ~200 chars for text/markdown files)
    #[serde(default)]
    pub text_preview: Option<String>,
    /// Artifact hash reference (hex)
    pub artifact_hash: String,
    /// Item dimensions (width, height) if applicable
    #[serde(default)]
    pub dimensions: Option<(u32, u32)>,
    /// Local asset path for viewer testing
    #[serde(default)]
    pub asset_path: Option<String>,
}

/// Artifact item in a proof folder
#[derive(Debug, Clone, Deserialize)]
pub struct ProofArtifactItem {
    /// BLAKE3 hash of artifact (hex)
    pub artifact_hash: String,
    /// Filename
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// Size in bytes
    #[serde(default)]
    pub size: u64,
    /// Base64-encoded thumbnail (for images)
    #[serde(default)]
    pub thumbnail_data: Option<String>,
    /// Base64-encoded inline data (for small images/files)
    #[serde(default)]
    pub inline_data: Option<String>,
    /// Image dimensions (width, height) if applicable
    #[serde(default)]
    pub dimensions: Option<(u32, u32)>,
    /// Local asset path for viewer testing
    #[serde(default)]
    pub asset_path: Option<String>,
    /// Caption / alt text
    #[serde(default)]
    pub caption: Option<String>,
}
