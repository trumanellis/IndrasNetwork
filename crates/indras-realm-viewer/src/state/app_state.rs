//! Main application state
//!
//! Coordinates all sub-states and handles event processing.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::events::{StreamEvent, EventCategory};

use super::{ArtifactState, AttentionState, ChatState, ContactsState, DocumentState, MemberProofDraftState, ProofFolderState, QuestState, RealmState, TokenState};

/// Global event buffer for replay on reset
static EVENT_BUFFER: std::sync::OnceLock<Arc<Mutex<Vec<StreamEvent>>>> = std::sync::OnceLock::new();

/// Get or initialize the global event buffer
pub fn event_buffer() -> Arc<Mutex<Vec<StreamEvent>>> {
    EVENT_BUFFER
        .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        .clone()
}

/// Clear the event buffer
pub fn clear_event_buffer() {
    if let Some(buffer) = EVENT_BUFFER.get() {
        buffer.lock().unwrap().clear();
    }
}

/// Active dashboard tab
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActiveTab {
    #[default]
    Realms,
    Quests,
    Attention,
    Contacts,
}

impl ActiveTab {
    pub fn display_name(&self) -> &'static str {
        match self {
            ActiveTab::Realms => "Realms",
            ActiveTab::Quests => "Quests",
            ActiveTab::Attention => "Attention",
            ActiveTab::Contacts => "Contacts",
        }
    }

    pub fn all() -> &'static [ActiveTab] {
        &[
            ActiveTab::Realms,
            ActiveTab::Quests,
            ActiveTab::Attention,
            ActiveTab::Contacts,
        ]
    }
}

/// Settings for playback control
#[derive(Clone, Debug)]
pub struct PlaybackSettings {
    pub paused: bool,
    pub speed: f32,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            paused: true,  // Start paused by default
            speed: 1.0,
        }
    }
}

/// Which screen a member is currently viewing (derived from their most recent action)
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MemberScreen {
    #[default]
    Home,
    QuestBoard,
    Chat,
    ProofEditor,
    Artifacts,
    Realms,
    Activity,
}

impl MemberScreen {
    /// Lucide icon SVG inner content (stroke icons, 24x24 viewBox)
    pub fn icon_svg(&self) -> &'static str {
        match self {
            MemberScreen::Home =>
                r#"<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/><path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#,
            MemberScreen::QuestBoard =>
                r#"<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="2"/>"#,
            MemberScreen::Chat =>
                r#"<path d="M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z"/>"#,
            MemberScreen::ProofEditor =>
                r#"<rect width="8" height="4" x="8" y="2" rx="1" ry="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><path d="m9 14 2 2 4-4"/>"#,
            MemberScreen::Artifacts =>
                r#"<path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551"/>"#,
            MemberScreen::Realms =>
                r#"<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>"#,
            MemberScreen::Activity =>
                r#"<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.36 8.36a.5.5 0 0 1-.96 0L8.24 2.18a.5.5 0 0 0-.96 0l-2.36 8.36A2 2 0 0 1 3 12H2"/>"#,
        }
    }

    /// Short label shown below the icon in tab bar
    pub fn label(&self) -> &'static str {
        match self {
            MemberScreen::Home => "HOME",
            MemberScreen::QuestBoard => "QUESTS",
            MemberScreen::Chat => "CHAT",
            MemberScreen::ProofEditor => "PROOF",
            MemberScreen::Artifacts => "FILES",
            MemberScreen::Realms => "REALMS",
            MemberScreen::Activity => "ACTIVITY",
        }
    }

    /// Full human-readable name (for tooltips, etc.)
    pub fn name(&self) -> &'static str {
        match self {
            MemberScreen::Home => "Home",
            MemberScreen::QuestBoard => "Quest Board",
            MemberScreen::Chat => "Chat",
            MemberScreen::ProofEditor => "Proof Editor",
            MemberScreen::Artifacts => "Artifacts",
            MemberScreen::Realms => "Realms",
            MemberScreen::Activity => "Activity",
        }
    }

    /// All screens (including Activity)
    pub fn all() -> &'static [MemberScreen] {
        &[
            MemberScreen::Home,
            MemberScreen::QuestBoard,
            MemberScreen::Chat,
            MemberScreen::ProofEditor,
            MemberScreen::Artifacts,
            MemberScreen::Realms,
            MemberScreen::Activity,
        ]
    }

    /// Screens shown as tabs in the header (Activity lives in the footer)
    pub fn tabs() -> &'static [MemberScreen] {
        &[
            MemberScreen::Home,
            MemberScreen::QuestBoard,
            MemberScreen::Chat,
            MemberScreen::ProofEditor,
            MemberScreen::Artifacts,
            MemberScreen::Realms,
        ]
    }
}

/// Per-member screen state tracking
#[derive(Clone, Debug)]
pub struct MemberScreenState {
    pub screen: MemberScreen,
    pub last_action: String,
    pub last_action_tick: u32,
    /// User-editable headline (short tagline)
    pub headline: String,
    /// User-editable bio (markdown)
    pub bio: String,
}

impl Default for MemberScreenState {
    fn default() -> Self {
        Self {
            screen: MemberScreen::Home,
            last_action: String::new(),
            last_action_tick: 0,
            headline: String::new(),
            bio: String::new(),
        }
    }
}

/// Recent event for the log panel
#[derive(Clone, Debug)]
pub struct LoggedEvent {
    pub tick: u32,
    pub category: EventCategory,
    pub type_name: String,
    pub summary: String,
}

impl LoggedEvent {
    pub fn from_event(event: &StreamEvent) -> Self {
        let summary = match event {
            StreamEvent::RealmCreated { realm_id, member_count, .. } => {
                format!("Realm {} created ({} members)", short_id(realm_id), member_count)
            }
            StreamEvent::MemberJoined { realm_id, member, .. } => {
                format!("{} joined {}", member_name(member), short_id(realm_id))
            }
            StreamEvent::MemberLeft { realm_id, member, .. } => {
                format!("{} left {}", member_name(member), short_id(realm_id))
            }
            StreamEvent::RealmAliasSet { member, realm_id, alias, .. } => {
                format!("{} renamed {} to \"{}\"", member_name(member), short_id(realm_id), alias)
            }
            StreamEvent::QuestCreated { quest_id, creator, title, .. } => {
                let title_str = if title.is_empty() { quest_id.as_str() } else { title.as_str() };
                format!("{} created \"{}\"", member_name(creator), title_str)
            }
            StreamEvent::QuestClaimSubmitted { quest_id, claimant, .. } => {
                format!("{} claimed {}", member_name(claimant), short_id(quest_id))
            }
            StreamEvent::QuestClaimVerified { quest_id, .. } => {
                format!("Claim verified on {}", short_id(quest_id))
            }
            StreamEvent::QuestCompleted { quest_id, .. } => {
                format!("Quest {} completed", short_id(quest_id))
            }
            StreamEvent::AttentionSwitched { member, quest_id, .. } => {
                format!("{} focusing on {}", member_name(member), short_id(quest_id))
            }
            StreamEvent::AttentionCleared { member, .. } => {
                format!("{} cleared focus", member_name(member))
            }
            StreamEvent::AttentionCalculated { quest_count, .. } => {
                format!("Calculated attention for {} quests", quest_count)
            }
            StreamEvent::RankingVerified { top_quest, .. } => {
                format!("Ranking verified, top: {}", top_quest.as_deref().unwrap_or("none"))
            }
            StreamEvent::ContactAdded { member, contact, .. } => {
                format!("{} added {} as contact", member_name(member), member_name(contact))
            }
            StreamEvent::ContactRemoved { member, contact, .. } => {
                format!("{} removed {}", member_name(member), member_name(contact))
            }
            StreamEvent::ChatMessage { member, content, .. } => {
                let preview: String = content.chars().take(30).collect();
                format!("{}: {}", member_name(member), preview)
            }
            StreamEvent::ChatMessageEdited { member, .. } => {
                format!("{} edited a message", member_name(member))
            }
            StreamEvent::ChatMessageDeleted { member, .. } => {
                format!("{} deleted a message", member_name(member))
            }
            StreamEvent::ChatImage { member, filename, alt_text, .. } => {
                let desc = alt_text.as_ref()
                    .or(filename.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or("image");
                format!("{} shared {}", member_name(member), desc)
            }
            StreamEvent::ChatGallery { member, title, items, .. } => {
                let desc = title.as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or_else(|| "gallery");
                format!("{} shared {} ({} items)", member_name(member), desc, items.len())
            }
            StreamEvent::ProofSubmitted { claimant, quest_title, .. } => {
                let title = if quest_title.is_empty() { "quest" } else { quest_title.as_str() };
                format!("{} submitted proof for {}", member_name(claimant), title)
            }
            StreamEvent::BlessingGiven { blesser, claimant, attention_millis, .. } => {
                let duration = format_duration_millis(*attention_millis);
                format!("{} blessed {} ({})", member_name(blesser), member_name(claimant), duration)
            }
            StreamEvent::ProofFolderSubmitted { claimant, artifact_count, .. } => {
                format!("{} submitted proof folder ({} files)", member_name(claimant), artifact_count)
            }
            StreamEvent::ProofFolderCreated { claimant, .. } => {
                format!("{} started proof folder", member_name(claimant))
            }
            StreamEvent::ProofFolderNarrativeUpdated { claimant, .. } => {
                format!("{} updated narrative", member_name(claimant))
            }
            StreamEvent::ProofFolderArtifactAdded { artifact_name, .. } => {
                format!("Added artifact: {}", artifact_name)
            }
            StreamEvent::CrdtConverged { folder_id, members_synced, .. } => {
                format!("CRDT synced {} ({} members)", short_id(folder_id), members_synced)
            }
            StreamEvent::CrdtConflict { folder_id, .. } => {
                format!("CRDT conflict on {}", short_id(folder_id))
            }
            StreamEvent::Info { message, .. } => {
                message.chars().take(50).collect()
            }
            StreamEvent::DocumentEdit { editor, document_id, .. } => {
                format!("{} edited document {}", member_name(editor), short_id(document_id))
            }
            StreamEvent::ArtifactSharedRevocable { sharer, name, size, .. } => {
                format!("{} shared \"{}\" ({}B, revocable)", member_name(sharer), name, size)
            }
            StreamEvent::ArtifactRecalled { revoked_by, artifact_hash, .. } => {
                format!("{} recalled artifact {}", member_name(revoked_by), short_id(artifact_hash))
            }
            StreamEvent::RecallAcknowledged { acknowledged_by, artifact_hash, .. } => {
                format!("{} acknowledged recall of {}", member_name(acknowledged_by), short_id(artifact_hash))
            }
            StreamEvent::ProfileUpdated { member, headline, .. } => {
                let desc = headline.as_deref().unwrap_or("profile");
                format!("{} updated {}", member_name(member), desc)
            }
            StreamEvent::Unknown => "Unknown event".to_string(),
        };

        Self {
            tick: event.tick(),
            category: event.category(),
            type_name: event.event_type_name().to_string(),
            summary,
        }
    }
}

/// Statistics for a member (used in POV dashboard)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemberStats {
    pub quests_created: usize,
    pub quests_assigned: usize,
    pub quests_completed: usize,
    pub realms_count: usize,
    pub contacts_count: usize,
    pub events_count: usize,
    pub tokens_count: usize,
    pub tokens_total_value: u64,
}

/// Main application state
#[derive(Clone, Debug, Default)]
pub struct AppState {
    /// Current tick number
    pub tick: u32,
    /// Active dashboard tab
    pub active_tab: ActiveTab,
    /// Playback settings
    pub playback: PlaybackSettings,
    /// Realm tracking state
    pub realms: RealmState,
    /// Quest tracking state
    pub quests: QuestState,
    /// Attention tracking state
    pub attention: AttentionState,
    /// Chat and blessing tracking state
    pub chat: ChatState,
    /// Contacts tracking state
    pub contacts: ContactsState,
    /// Artifact tracking state
    pub artifacts: ArtifactState,
    /// Proof folder editor state
    pub proof_folder: ProofFolderState,
    /// Token of Gratitude tracking state
    pub tokens: TokenState,
    /// Document content tracking (CRDT edits)
    pub documents: DocumentState,
    /// Per-member proof folder draft tracking (for V2 multi-column)
    pub member_proof_drafts: MemberProofDraftState,
    /// Recent events for log panel (newest first)
    pub event_log: VecDeque<LoggedEvent>,
    /// Maximum events to keep in log
    pub max_log_events: usize,
    /// Total events processed
    pub total_events: usize,
    /// Currently selected point-of-view member (None = overview mode)
    pub selected_pov: Option<String>,
    /// Per-member screen state (for omni v2 multi-screen dashboard)
    pub member_screens: HashMap<String, MemberScreenState>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            max_log_events: 100,
            ..Default::default()
        }
    }

    /// Process a stream event and update state
    pub fn process_event(&mut self, event: StreamEvent) {
        // Update tick
        let event_tick = event.tick();
        if event_tick > self.tick {
            self.tick = event_tick;
        }

        // Add to log
        let logged = LoggedEvent::from_event(&event);
        self.event_log.push_front(logged);
        while self.event_log.len() > self.max_log_events {
            self.event_log.pop_back();
        }

        // Dispatch to sub-states
        match &event {
            StreamEvent::RealmCreated { .. }
            | StreamEvent::MemberJoined { .. }
            | StreamEvent::MemberLeft { .. }
            | StreamEvent::RealmAliasSet { .. } => {
                self.realms.process_event(&event);
            }

            StreamEvent::ProfileUpdated { member, headline, bio, .. } => {
                let screen_state = self.member_screens.entry(member.clone()).or_default();
                if let Some(h) = headline {
                    screen_state.headline = h.clone();
                }
                if let Some(b) = bio {
                    screen_state.bio = b.clone();
                }
            }

            StreamEvent::QuestCreated { realm_id, .. } => {
                self.quests.process_event(&event);
                // Sync quest count to realm state
                self.realms.increment_quest_count(realm_id);
            }

            StreamEvent::QuestClaimSubmitted { .. }
            | StreamEvent::QuestClaimVerified { .. }
            | StreamEvent::QuestCompleted { .. } => {
                self.quests.process_event(&event);
            }

            StreamEvent::AttentionSwitched { .. }
            | StreamEvent::AttentionCleared { .. }
            | StreamEvent::AttentionCalculated { .. }
            | StreamEvent::RankingVerified { .. } => {
                self.attention.process_event(&event);
            }

            StreamEvent::ContactAdded { .. } | StreamEvent::ContactRemoved { .. } => {
                self.contacts.process_event(&event);
            }

            StreamEvent::ChatMessage { .. }
            | StreamEvent::ChatMessageEdited { .. }
            | StreamEvent::ChatMessageDeleted { .. }
            | StreamEvent::ChatImage { .. }
            | StreamEvent::ChatGallery { .. } => {
                self.chat.process_event(&event);
            }

            StreamEvent::ProofSubmitted { .. }
            | StreamEvent::BlessingGiven { .. } => {
                self.chat.process_event(&event);
                self.tokens.process_event(&event);
            }

            StreamEvent::ProofFolderSubmitted { .. } => {
                self.chat.process_event(&event);
                self.tokens.process_event(&event);
                self.member_proof_drafts.process_event(&event);
                self.artifacts.process_event(&event);
            }

            StreamEvent::ProofFolderCreated { .. }
            | StreamEvent::ProofFolderNarrativeUpdated { .. }
            | StreamEvent::ProofFolderArtifactAdded { .. } => {
                self.member_proof_drafts.process_event(&event);
            }

            StreamEvent::ArtifactSharedRevocable { .. }
            | StreamEvent::ArtifactRecalled { .. }
            | StreamEvent::RecallAcknowledged { .. } => {
                self.artifacts.process_event(&event);
            }

            StreamEvent::DocumentEdit { .. } => {
                self.documents.process_event(&event);
            }

            _ => {}
        }

        // Update member screen state based on event
        self.update_member_screen(&event);

        self.total_events += 1;
    }

    /// Update the screen state for the acting member based on the event type
    fn update_member_screen(&mut self, event: &StreamEvent) {
        // Realm chat messages switch all realm members to Chat
        if let StreamEvent::ChatMessage { member, content, realm_id: Some(rid), .. } = event {
            let tick = event.tick();
            let preview: String = content.chars().take(30).collect();
            let sender_name = member_name(member);
            let action_for_sender = preview.clone();
            let action_for_others = format!("{}: {}", sender_name, preview);

            // Get all members in this realm
            let realm_members: Vec<String> = self.realms.realms_for_member(member)
                .iter()
                .filter(|r| r.realm_id == *rid)
                .flat_map(|r| r.members.clone())
                .collect();

            for m in &realm_members {
                let entry = self.member_screens.entry(m.clone()).or_default();
                entry.screen = MemberScreen::Chat;
                entry.last_action = if m == member {
                    action_for_sender.clone()
                } else {
                    action_for_others.clone()
                };
                entry.last_action_tick = tick;
            }

            // Also set the sender in case they weren't in the realm member list
            let entry = self.member_screens.entry(member.clone()).or_default();
            entry.screen = MemberScreen::Chat;
            entry.last_action = action_for_sender;
            entry.last_action_tick = tick;
            return;
        }

        let (member_id, screen, action) = match event {
            StreamEvent::MemberJoined { member, realm_id, .. } => {
                (member.clone(), MemberScreen::Realms, format!("joined realm {}", short_id(realm_id)))
            }
            StreamEvent::MemberLeft { member, realm_id, .. } => {
                (member.clone(), MemberScreen::Realms, format!("left realm {}", short_id(realm_id)))
            }
            StreamEvent::RealmAliasSet { member, alias, .. } => {
                (member.clone(), MemberScreen::Realms, format!("renamed realm to \"{}\"", alias))
            }
            StreamEvent::QuestCreated { creator, title, .. } => {
                let desc = if title.is_empty() { "a quest".to_string() } else { format!("\"{}\"", title) };
                (creator.clone(), MemberScreen::QuestBoard, format!("created {}", desc))
            }
            StreamEvent::QuestClaimSubmitted { claimant, quest_id, .. } => {
                (claimant.clone(), MemberScreen::QuestBoard, format!("claimed {}", short_id(quest_id)))
            }
            StreamEvent::AttentionSwitched { member, quest_id, .. } => {
                (member.clone(), MemberScreen::QuestBoard, format!("focusing on {}", short_id(quest_id)))
            }
            StreamEvent::AttentionCleared { member, .. } => {
                (member.clone(), MemberScreen::Home, "cleared focus".to_string())
            }
            StreamEvent::ContactAdded { member, contact, .. } => {
                (member.clone(), MemberScreen::Home, format!("added contact {}", member_name(contact)))
            }
            StreamEvent::ContactRemoved { member, contact, .. } => {
                (member.clone(), MemberScreen::Home, format!("removed contact {}", member_name(contact)))
            }
            // ChatMessage without realm_id — sender-only screen switch
            StreamEvent::ChatMessage { member, content, .. } => {
                let preview: String = content.chars().take(30).collect();
                (member.clone(), MemberScreen::Chat, preview)
            }
            StreamEvent::ChatMessageEdited { member, .. } => {
                (member.clone(), MemberScreen::Chat, "edited a message".to_string())
            }
            StreamEvent::ChatMessageDeleted { member, .. } => {
                (member.clone(), MemberScreen::Chat, "deleted a message".to_string())
            }
            StreamEvent::ChatImage { member, alt_text, filename, .. } => {
                let desc = alt_text.as_ref().or(filename.as_ref())
                    .map(|s| s.as_str()).unwrap_or("image");
                (member.clone(), MemberScreen::Chat, format!("shared {}", desc))
            }
            StreamEvent::ChatGallery { member, title, items, .. } => {
                let desc = title.as_deref().unwrap_or("gallery");
                (member.clone(), MemberScreen::Chat, format!("shared {} ({} items)", desc, items.len()))
            }
            StreamEvent::ProofFolderCreated { claimant, .. } => {
                (claimant.clone(), MemberScreen::ProofEditor, "started proof folder".to_string())
            }
            StreamEvent::ProofFolderNarrativeUpdated { claimant, .. } => {
                (claimant.clone(), MemberScreen::ProofEditor, "updated narrative".to_string())
            }
            StreamEvent::ProofFolderSubmitted { claimant, artifact_count, .. } => {
                (claimant.clone(), MemberScreen::Chat, format!("submitted proof folder ({} files)", artifact_count))
            }
            StreamEvent::ProofSubmitted { claimant, quest_title, .. } => {
                let title = if quest_title.is_empty() { "quest" } else { quest_title.as_str() };
                (claimant.clone(), MemberScreen::QuestBoard, format!("submitted proof for {}", title))
            }
            StreamEvent::BlessingGiven { blesser, claimant, .. } => {
                (blesser.clone(), MemberScreen::Chat, format!("blessed {}", member_name(claimant)))
            }
            StreamEvent::ArtifactSharedRevocable { sharer, name, .. } => {
                (sharer.clone(), MemberScreen::Artifacts, format!("shared \"{}\"", name))
            }
            StreamEvent::ArtifactRecalled { revoked_by, artifact_hash, .. } => {
                (revoked_by.clone(), MemberScreen::Artifacts, format!("recalled {}", short_id(artifact_hash)))
            }
            StreamEvent::RecallAcknowledged { acknowledged_by, artifact_hash, .. } => {
                (acknowledged_by.clone(), MemberScreen::Artifacts, format!("acknowledged recall of {}", short_id(artifact_hash)))
            }
            StreamEvent::DocumentEdit { editor, document_id, .. } => {
                (editor.clone(), MemberScreen::Artifacts, format!("edited document {}", short_id(document_id)))
            }
            StreamEvent::ProfileUpdated { member, headline, .. } => {
                let desc = headline.as_deref().unwrap_or("their profile");
                (member.clone(), MemberScreen::Home, format!("updated {}", desc))
            }
            // System events with no acting member - no screen change
            _ => return,
        };

        let tick = event.tick();
        let entry = self.member_screens.entry(member_id).or_default();
        entry.screen = screen;
        entry.last_action = action;
        entry.last_action_tick = tick;
    }

    /// Reset all state
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Get all members for POV selector dropdown (sorted by name)
    pub fn all_members(&self) -> Vec<String> {
        use std::collections::HashSet;
        let mut all: HashSet<String> = HashSet::new();

        // From realms
        for m in &self.realms.all_members {
            all.insert(m.clone());
        }

        // From contacts
        for m in self.contacts.all_members() {
            all.insert(m);
        }

        // From attention events
        for m in self.attention.current_focus.keys() {
            all.insert(m.clone());
        }

        let mut members: Vec<String> = all.into_iter().collect();
        members.sort_by(|a, b| member_name(a).cmp(&member_name(b)));
        members
    }

    /// Check if viewing from a specific POV
    pub fn is_pov_mode(&self) -> bool {
        self.selected_pov.is_some()
    }

    /// Set POV (None clears to overview)
    pub fn set_pov(&mut self, member: Option<String>) {
        self.selected_pov = member;
    }

    /// Get the current screen state for a member
    pub fn screen_for_member(&self, member: &str) -> &MemberScreenState {
        static DEFAULT: std::sync::OnceLock<MemberScreenState> = std::sync::OnceLock::new();
        self.member_screens.get(member).unwrap_or_else(|| {
            DEFAULT.get_or_init(MemberScreenState::default)
        })
    }

    /// How many ticks since the member's last action
    pub fn ticks_since_action(&self, member: &str) -> u32 {
        self.member_screens.get(member)
            .map(|s| self.tick.saturating_sub(s.last_action_tick))
            .unwrap_or(u32::MAX)
    }

    /// Get statistics for a specific member
    pub fn stats_for_member(&self, member: &str) -> MemberStats {
        let quests_created = self.quests.quests.values()
            .filter(|q| q.creator == member)
            .count();

        let quests_assigned = self.quests.quests.values()
            .filter(|q| q.claims.iter().any(|c| c.claimant == member))
            .count();

        let quests_completed = self.quests.quests.values()
            .filter(|q| q.status == super::QuestStatus::Completed &&
                   (q.creator == member || q.claims.iter().any(|c| c.claimant == member)))
            .count();

        let realms_count = self.realms.realms_for_member(member).len();
        let contacts_count = self.contacts.contacts_for_member(member).len();

        let events_count = self.event_log.iter()
            .filter(|e| e.summary.contains(&member_name(member)))
            .count();

        let tokens_count = self.tokens.token_count_for_member(member);
        let tokens_total_value = self.tokens.total_value_for_member(member);

        MemberStats {
            quests_created,
            quests_assigned,
            quests_completed,
            realms_count,
            contacts_count,
            events_count,
            tokens_count,
            tokens_total_value,
        }
    }

    /// Get events involving a specific member
    pub fn events_for_member(&self, member: &str) -> Vec<&LoggedEvent> {
        let name = member_name(member);
        self.event_log.iter()
            .filter(|e| e.summary.contains(&name) || e.summary.contains(member))
            .collect()
    }
}

/// Convert member ID to human-readable name
pub fn member_name(member_id: &str) -> String {
    // Use first 4 hex chars to select from virtue names
    let names = [
        "Love", "Joy", "Peace", "Grace", "Hope", "Faith", "Light", "Truth",
        "Wisdom", "Mercy", "Valor", "Honor", "Glory", "Spirit", "Unity", "Bliss",
    ];

    if member_id.len() >= 4 {
        // Parse first 4 chars as hex
        if let Ok(n) = u16::from_str_radix(&member_id[..4], 16) {
            let idx = (n as usize) % names.len();
            return names[idx].to_string();
        }
    }

    // Short IDs (e.g. "A", "B", "C" from mesh builder): use ordinal position
    if !member_id.is_empty() && member_id.len() < 4 {
        let first = member_id.as_bytes()[0];
        let idx = match first {
            b'A'..=b'Z' => (first - b'A') as usize,
            b'a'..=b'z' => (first - b'a') as usize,
            b'0'..=b'9' => (first - b'0') as usize,
            _ => first as usize,
        };
        return names[idx % names.len()].to_string();
    }

    // Fallback: use first few chars
    if member_id.len() > 8 {
        format!("{}...", &member_id[..8])
    } else {
        member_id.to_string()
    }
}

/// Shorten an ID for display
pub fn short_id(id: &str) -> String {
    if id.len() > 8 {
        format!("{}...", &id[..8])
    } else {
        id.to_string()
    }
}

/// Format milliseconds as human-readable duration (e.g., "2h 30m")
pub fn format_duration_millis(millis: u64) -> String {
    let seconds = millis / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        let remaining_mins = minutes % 60;
        if remaining_mins > 0 {
            format!("{}h {}m", hours, remaining_mins)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_name() {
        // First 4 hex chars determine the name
        assert!(!member_name("abcd1234").is_empty());
        assert!(!member_name("0000ffff").is_empty());
    }

    #[test]
    fn test_short_id() {
        assert_eq!(short_id("abcdefghij"), "abcdefgh...");
        assert_eq!(short_id("short"), "short");
    }
}
