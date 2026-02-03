//! State management for the Genesis flow.

/// The current step in the genesis flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenesisStep {
    /// Welcome branding screen (auto-advances).
    Welcome,
    /// Collect display name.
    DisplayName,
    /// Home realm view (genesis complete).
    HomeRealm,
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
            pass_story_active: false,
            note_draft_title: String::new(),
            note_draft_content: String::new(),
            note_form_open: false,
            nudge_dismissed: false,
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
