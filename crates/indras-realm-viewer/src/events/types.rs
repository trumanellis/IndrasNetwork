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

    // ========== Chat Events ==========
    #[serde(rename = "chat_message")]
    ChatMessage {
        #[serde(default)]
        tick: u32,
        member: String,
        content: String,
        #[serde(default)]
        message_type: String,
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
            StreamEvent::ChatMessage { tick, .. } => *tick,
            StreamEvent::ProofSubmitted { tick, .. } => *tick,
            StreamEvent::BlessingGiven { tick, .. } => *tick,
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
            StreamEvent::ChatMessage { .. } => "chat_message",
            StreamEvent::ProofSubmitted { .. } => "proof_submitted",
            StreamEvent::BlessingGiven { .. } => "blessing_given",
            StreamEvent::Info { .. } => "info",
            StreamEvent::Unknown => "unknown",
        }
    }

    /// Get the category of this event for filtering
    pub fn category(&self) -> EventCategory {
        match self {
            StreamEvent::RealmCreated { .. }
            | StreamEvent::MemberJoined { .. }
            | StreamEvent::MemberLeft { .. } => EventCategory::Realm,

            StreamEvent::QuestCreated { .. }
            | StreamEvent::QuestClaimSubmitted { .. }
            | StreamEvent::QuestClaimVerified { .. }
            | StreamEvent::QuestCompleted { .. } => EventCategory::Quest,

            StreamEvent::AttentionSwitched { .. }
            | StreamEvent::AttentionCleared { .. }
            | StreamEvent::AttentionCalculated { .. }
            | StreamEvent::RankingVerified { .. } => EventCategory::Attention,

            StreamEvent::ContactAdded { .. } | StreamEvent::ContactRemoved { .. } => {
                EventCategory::Contacts
            }

            StreamEvent::ChatMessage { .. } => EventCategory::Chat,

            StreamEvent::ProofSubmitted { .. } | StreamEvent::BlessingGiven { .. } => {
                EventCategory::Blessing
            }

            StreamEvent::Info { .. } | StreamEvent::Unknown => EventCategory::Info,
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
            EventCategory::Info => "event-info",
        }
    }
}
