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

/// View model for a quest in the home realm.
#[derive(Debug, Clone)]
pub struct QuestView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub is_complete: bool,
}

/// View model for a note in the home realm.
#[derive(Debug, Clone)]
pub struct NoteView {
    pub id: String,
    pub title: String,
    pub content_preview: String,
}

/// View model for a contact in the connections panel.
#[derive(Debug, Clone)]
pub struct ContactView {
    pub member_id: [u8; 32],
    pub member_id_short: String,
    pub display_name: Option<String>,
    pub status: String,  // "pending" or "confirmed"
}

/// View model for a message in a peer realm chat.
#[derive(Debug, Clone)]
pub struct PeerMessageView {
    pub sender_name: String,
    pub sender_id_short: String,
    pub is_me: bool,
    pub timestamp: String,
    pub message_type: PeerMessageType,
}

/// Type of message content for peer realm chat rendering.
#[derive(Debug, Clone)]
pub enum PeerMessageType {
    Text { content: String },
    Image { data_url: Option<String>, filename: Option<String>, alt_text: Option<String> },
    System { content: String },
    Artifact { name: String, size: u64, mime_type: Option<String> },
    ProofSubmitted { quest_id_short: String, claimant_name: String },
    BlessingGiven { claimant_name: String, duration: String },
    ProofFolderSubmitted { narrative_preview: String, artifact_count: usize },
    Gallery { title: Option<String>, item_count: usize },
    Reaction { emoji: String },
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
    /// Messages in the active peer realm chat.
    pub peer_realm_messages: Vec<PeerMessageView>,
    /// Draft text in the peer realm chat input.
    pub peer_realm_draft: String,
    /// Number of messages in the peer realm chat.
    pub peer_realm_message_count: usize,
    /// Last message sequence number for polling.
    pub peer_realm_last_seq: u64,
    /// Display name of the contact in the active peer realm.
    pub peer_realm_contact_name: Option<String>,
    /// Whether the action menu is open in the peer realm chat.
    pub peer_realm_action_menu_open: bool,
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
            peer_realm_messages: Vec::new(),
            peer_realm_draft: String::new(),
            peer_realm_message_count: 0,
            peer_realm_last_seq: 0,
            peer_realm_contact_name: None,
            peer_realm_action_menu_open: false,
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
