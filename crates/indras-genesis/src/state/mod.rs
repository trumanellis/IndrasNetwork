//! State management for the Genesis flow.

use indras_ui::ArtifactDisplayInfo;

/// The current step in the genesis flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenesisStep {
    /// Welcome branding screen (auto-advances).
    Welcome,
    /// Collect display name.
    DisplayName,
    /// Home realm view (genesis complete).
    HomeRealm,
    /// Peer realm chat view with a specific contact.
    PeerRealm([u8; 32]),
}

/// Status of an async operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncStatus {
    /// No operation in progress.
    Idle,
    /// Operation in progress.
    Loading,
    /// Operation failed.
    Error(String),
}

/// Status of a quest for display purposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestStatus {
    /// Quest is open - no claims yet.
    Open,
    /// Quest has claims but none verified yet.
    Claimed,
    /// Quest has at least one verified claim.
    Verified,
    /// Quest is complete.
    Completed,
}

/// View model for a quest claim.
#[derive(Debug, Clone)]
pub struct QuestClaimView {
    /// Claimant's member ID (short hex).
    pub claimant_id_short: String,
    /// Claimant's display name if known.
    pub claimant_name: Option<String>,
    /// Whether this claim has been verified.
    pub verified: bool,
    /// Whether this claim has a proof artifact.
    pub has_proof: bool,
    /// When the claim was submitted (formatted string).
    pub submitted_at: String,
}

/// View model for a quest in the home realm.
#[derive(Debug, Clone)]
pub struct QuestView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub is_complete: bool,
    /// Current status for display.
    pub status: QuestStatus,
    /// Creator's member ID (short hex).
    pub creator_id_short: String,
    /// Whether current user is the creator.
    pub is_creator: bool,
    /// Claims on this quest.
    pub claims: Vec<QuestClaimView>,
    /// Number of pending (unverified) claims.
    pub pending_claim_count: usize,
    /// Number of verified claims.
    pub verified_claim_count: usize,
    /// Attention data for this quest.
    pub attention: QuestAttentionView,
}

/// View model for a note in the home realm.
#[derive(Debug, Clone)]
pub struct NoteView {
    pub id: String,
    pub title: String,
    pub content: String,
    pub content_preview: String,
}

/// Mode for the note editor modal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NoteEditorMode {
    /// Viewing a note (read-only with rendered/raw toggle).
    #[default]
    View,
    /// Editing an existing note.
    Edit,
    /// Creating a new note.
    Create,
}

/// Mode for the quest editor modal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum QuestEditorMode {
    /// Viewing a quest (read-only with rendered markdown description).
    #[default]
    View,
    /// Editing an existing quest.
    Edit,
    /// Creating a new quest.
    Create,
}

/// Sentiment indicator for a contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContactSentiment {
    /// Positive recommendation - you vouch for this contact.
    Recommend,
    /// Neutral - no strong opinion.
    #[default]
    Neutral,
    /// Blocked - you don't want to interact with this contact.
    Blocked,
}

impl ContactSentiment {
    /// Convert to i8 value for storage (-1, 0, +1).
    pub fn to_value(self) -> i8 {
        match self {
            ContactSentiment::Recommend => 1,
            ContactSentiment::Neutral => 0,
            ContactSentiment::Blocked => -1,
        }
    }

    /// Create from i8 value.
    pub fn from_value(value: i8) -> Self {
        match value {
            1 => ContactSentiment::Recommend,
            -1 => ContactSentiment::Blocked,
            _ => ContactSentiment::Neutral,
        }
    }
}

/// View model for a contact in the connections panel.
#[derive(Debug, Clone)]
pub struct ContactView {
    pub member_id: [u8; 32],
    pub member_id_short: String,
    pub display_name: Option<String>,
    pub status: String,  // "pending" or "confirmed"
    /// Sentiment towards this contact.
    pub sentiment: ContactSentiment,
}

/// View model for attention data on a quest.
#[derive(Debug, Clone, Default)]
pub struct QuestAttentionView {
    /// Total attention time in milliseconds.
    pub total_attention_millis: u64,
    /// Number of members currently focused.
    pub focused_member_count: usize,
    /// Whether the current user is focused on this quest.
    pub is_focused: bool,
}

/// View model for a token of gratitude.
#[derive(Debug, Clone)]
pub struct TokenView {
    /// Token ID (short hex).
    pub id_short: String,
    /// Source quest title.
    pub source_quest_title: Option<String>,
    /// Who gave the blessing that minted this token.
    pub blesser_name: Option<String>,
    /// Whether this token is pledged to a quest.
    pub is_pledged: bool,
    /// Quest title if pledged.
    pub pledged_quest_title: Option<String>,
    /// Created timestamp (formatted).
    pub created_at: String,
}

/// Direction of a network event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventDirection {
    /// We sent/initiated this.
    Sent,
    /// We received this from the network.
    Received,
    /// Local system event.
    System,
}

/// A single entry in the event log.
#[derive(Debug, Clone)]
pub struct EventLogEntry {
    /// Timestamp string (HH:MM:SS).
    pub timestamp: String,
    /// Direction arrow.
    pub direction: EventDirection,
    /// Short description of what happened.
    pub message: String,
    /// Optional action button (e.g., "Release Tokens" for proof notifications).
    pub action_label: Option<String>,
    /// Whether this entry should be highlighted (e.g., proof requiring attention).
    pub highlighted: bool,
}

/// Main state for the genesis flow.
#[derive(Debug, Clone)]
pub struct GenesisState {
    /// Current step in the flow.
    pub step: GenesisStep,
    /// Status of any async operation.
    pub status: AsyncStatus,
    /// Display name entered by user.
    pub display_name: String,
    /// Short member ID (hex, first 8 bytes) once identity is created.
    pub member_id_short: Option<String>,
    /// Quests loaded from home realm.
    pub quests: Vec<QuestView>,
    /// Notes loaded from home realm.
    pub notes: Vec<NoteView>,
    /// Contacts loaded from contacts realm.
    pub contacts: Vec<ContactView>,
    /// Artifacts loaded from home realm artifact index.
    pub artifacts: Vec<ArtifactDisplayInfo>,
    /// Whether the pass story flow overlay is active.
    pub pass_story_active: bool,
    /// Draft note title for the create-note form.
    pub note_draft_title: String,
    /// Draft note content for the create-note form.
    pub note_draft_content: String,
    /// Whether the note creation form is visible.
    pub note_form_open: bool,
    /// Whether the nudge banner has been dismissed.
    pub nudge_dismissed: bool,
    /// Whether the contact invite overlay is open.
    pub contact_invite_open: bool,
    /// Text input for pasting another user's invite URI.
    pub contact_invite_input: String,
    /// Connect status: None=idle, Some("error:...") or Some("success:...").
    pub contact_invite_status: Option<String>,
    /// Parsed inviter display name from pasted URI.
    pub contact_parsed_name: Option<String>,
    /// Brief "Copied!" feedback after copying invite link.
    pub contact_copy_feedback: bool,
    /// Pre-computed contact invite URI (async, includes transport info).
    pub invite_code_uri: Option<String>,
    /// Whether a contact connect operation is in progress.
    pub contact_connecting: bool,
    /// Filter text for the contacts list.
    pub contact_filter: String,
    /// Toast message for new connections (auto-cleared after 5s).
    pub new_contact_toast: Option<String>,
    /// Network event log (newest first).
    pub event_log: Vec<EventLogEntry>,
    /// Display name of the contact in the active peer realm.
    pub peer_realm_contact_name: Option<String>,
    /// Quests in the active peer realm (shared quests with contact).
    pub peer_realm_quests: Vec<QuestView>,
    /// Notes in the active peer realm (shared notes with contact).
    pub peer_realm_notes: Vec<NoteView>,
    /// Artifacts in the active peer realm (shared artifacts with contact).
    pub peer_realm_artifacts: Vec<ArtifactDisplayInfo>,
    /// Quest ID being claimed in peer realm.
    pub peer_realm_claiming_quest_id: Option<String>,
    /// Draft proof text for peer realm quest claim.
    pub peer_realm_claim_proof_text: String,
    /// Tokens of gratitude owned by the user.
    pub tokens: Vec<TokenView>,
    /// Quest ID currently being claimed (for claim form).
    pub claiming_quest_id: Option<String>,
    /// Draft proof text for quest claim.
    pub claim_proof_text: String,
    /// Whether the note editor modal is open.
    pub note_editor_open: bool,
    /// Mode for the note editor modal.
    pub note_editor_mode: NoteEditorMode,
    /// Note ID being edited (None for create mode).
    pub note_editor_id: Option<String>,
    /// Title in the note editor.
    pub note_editor_title: String,
    /// Content in the note editor.
    pub note_editor_content: String,
    /// Whether to show rendered markdown (true) or raw (false) in view mode.
    pub note_editor_preview_mode: bool,
    /// Whether the quest editor modal is open.
    pub quest_editor_open: bool,
    /// Mode for the quest editor modal.
    pub quest_editor_mode: QuestEditorMode,
    /// Quest ID being edited (None for create mode).
    pub quest_editor_id: Option<String>,
    /// Title in the quest editor.
    pub quest_editor_title: String,
    /// Description (markdown) in the quest editor.
    pub quest_editor_description: String,
    /// Whether to show rendered markdown (true) or raw (false) in view mode.
    pub quest_editor_preview_mode: bool,
}

impl Default for GenesisState {
    fn default() -> Self {
        Self {
            step: GenesisStep::Welcome,
            status: AsyncStatus::Idle,
            display_name: String::new(),
            member_id_short: None,
            quests: Vec::new(),
            notes: Vec::new(),
            contacts: Vec::new(),
            artifacts: Vec::new(),
            pass_story_active: false,
            note_draft_title: String::new(),
            note_draft_content: String::new(),
            note_form_open: false,
            nudge_dismissed: false,
            contact_invite_open: false,
            contact_invite_input: String::new(),
            contact_invite_status: None,
            contact_parsed_name: None,
            contact_copy_feedback: false,
            invite_code_uri: None,
            contact_connecting: false,
            contact_filter: String::new(),
            new_contact_toast: None,
            event_log: Vec::new(),
            peer_realm_contact_name: None,
            peer_realm_quests: Vec::new(),
            peer_realm_notes: Vec::new(),
            peer_realm_artifacts: Vec::new(),
            peer_realm_claiming_quest_id: None,
            peer_realm_claim_proof_text: String::new(),
            tokens: Vec::new(),
            claiming_quest_id: None,
            claim_proof_text: String::new(),
            note_editor_open: false,
            note_editor_mode: NoteEditorMode::default(),
            note_editor_id: None,
            note_editor_title: String::new(),
            note_editor_content: String::new(),
            note_editor_preview_mode: true,
            quest_editor_open: false,
            quest_editor_mode: QuestEditorMode::default(),
            quest_editor_id: None,
            quest_editor_title: String::new(),
            quest_editor_description: String::new(),
            quest_editor_preview_mode: true,
        }
    }
}

impl GenesisState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// State for the lazy pass story flow.
#[derive(Debug, Clone)]
pub struct PassStoryState {
    /// Current stage index (0-10).
    pub current_stage: usize,
    /// The 23 slot values filled by the user.
    pub slots: [String; 23],
    /// Indices of slots flagged as weak by entropy gate.
    pub weak_slots: Vec<usize>,
    /// Whether the story has been submitted.
    pub submitted: bool,
    /// Status of the submission.
    pub status: AsyncStatus,
}

impl Default for PassStoryState {
    fn default() -> Self {
        Self {
            current_stage: 0,
            slots: std::array::from_fn(|_| String::new()),
            weak_slots: Vec::new(),
            submitted: false,
            status: AsyncStatus::Idle,
        }
    }
}
