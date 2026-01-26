//! Main application state for the home realm viewer.

use std::collections::VecDeque;

use crate::events::HomeRealmEvent;

use super::{ArtifactsState, NotesState, QuestsState, SessionState, SessionStatus, SyncStatus};

/// Maximum number of activity events to keep.
const MAX_ACTIVITY_EVENTS: usize = 50;

/// Playback settings.
#[derive(Debug, Clone)]
pub struct PlaybackSettings {
    pub speed: f32,
    pub paused: bool,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            paused: true,
        }
    }
}

/// An entry in the activity feed.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityEvent {
    pub tick: u32,
    pub description: String,
    pub event_type: ActivityEventType,
}

/// Type of activity event for styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityEventType {
    Note,
    Quest,
    Artifact,
    Session,
    Sync,
    Chat,
    Blessing,
    Info,
}

impl ActivityEvent {
    /// Creates an activity event from a home realm event.
    pub fn from_event(event: &HomeRealmEvent) -> Self {
        let (event_type, description) = match event {
            HomeRealmEvent::NoteCreated { .. }
            | HomeRealmEvent::NoteUpdated { .. }
            | HomeRealmEvent::NoteDeleted { .. } => (ActivityEventType::Note, event.description()),

            HomeRealmEvent::HomeQuestCreated { .. }
            | HomeRealmEvent::HomeQuestCompleted { .. } => {
                (ActivityEventType::Quest, event.description())
            }

            HomeRealmEvent::ArtifactUploaded { .. }
            | HomeRealmEvent::ArtifactRetrieved { .. } => {
                (ActivityEventType::Artifact, event.description())
            }

            HomeRealmEvent::SessionStarted { .. } | HomeRealmEvent::SessionEnded { .. } => {
                (ActivityEventType::Session, event.description())
            }

            HomeRealmEvent::HomeRealmIdComputed { .. }
            | HomeRealmEvent::DataRecovered { .. }
            | HomeRealmEvent::MultiDeviceSync { .. } => {
                (ActivityEventType::Sync, event.description())
            }

            HomeRealmEvent::ChatMessage { .. } => (ActivityEventType::Chat, event.description()),

            HomeRealmEvent::ProofSubmitted { .. }
            | HomeRealmEvent::BlessingGiven { .. }
            | HomeRealmEvent::BlessingReceived { .. } => {
                (ActivityEventType::Blessing, event.description())
            }

            HomeRealmEvent::Info { .. } | HomeRealmEvent::Unknown => {
                (ActivityEventType::Info, event.description())
            }
        };

        Self {
            tick: event.tick(),
            description,
            event_type,
        }
    }
}

/// Main application state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current simulation tick.
    pub tick: u32,

    /// The member being viewed (first-person focus).
    pub selected_member: Option<String>,

    /// Playback settings.
    pub playback: PlaybackSettings,

    /// Notes state.
    pub notes: NotesState,

    /// Quests state.
    pub quests: QuestsState,

    /// Artifacts state.
    pub artifacts: ArtifactsState,

    /// Session state.
    pub session: SessionState,

    /// Recent activity events (newest first).
    pub activity_log: VecDeque<ActivityEvent>,

    /// Total events processed.
    pub total_events: u32,

    /// Currently selected note (for detail view).
    pub selected_note_id: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Creates a new application state.
    pub fn new() -> Self {
        Self {
            tick: 0,
            selected_member: None,
            playback: PlaybackSettings::default(),
            notes: NotesState::new(),
            quests: QuestsState::new(),
            artifacts: ArtifactsState::new(),
            session: SessionState::new(),
            activity_log: VecDeque::with_capacity(MAX_ACTIVITY_EVENTS),
            total_events: 0,
            selected_note_id: None,
        }
    }

    /// Processes a home realm event.
    pub fn process_event(&mut self, event: HomeRealmEvent) {
        // Update tick
        self.tick = self.tick.max(event.tick());

        // Update selected member from first event if not set
        if self.selected_member.is_none() {
            if let Some(member) = event.member() {
                self.selected_member = Some(member.to_string());
            }
        }

        // Add to activity log (skip verbose events)
        let should_log = !matches!(
            event,
            // Skip HomeRealmIdComputed - these are verification checks, too noisy
            HomeRealmEvent::HomeRealmIdComputed { .. } |
            // Skip Unknown events
            HomeRealmEvent::Unknown
        );

        if should_log {
            let activity = ActivityEvent::from_event(&event);
            self.activity_log.push_front(activity);
            while self.activity_log.len() > MAX_ACTIVITY_EVENTS {
                self.activity_log.pop_back();
            }
        }

        // Dispatch to sub-states
        self.notes.process_event(&event);
        self.quests.process_event(&event);
        self.artifacts.process_event(&event);
        self.session.process_event(&event);

        self.total_events += 1;
    }

    /// Returns whether the session is currently active.
    pub fn is_session_active(&self) -> bool {
        self.session.status == SessionStatus::Active
    }

    /// Returns whether sync is healthy.
    pub fn is_sync_healthy(&self) -> bool {
        self.session.sync_status == SyncStatus::Synced
    }

    /// Resets the state.
    pub fn reset(&mut self) {
        self.tick = 0;
        self.notes.reset();
        self.quests.reset();
        self.artifacts.reset();
        self.session.reset();
        self.activity_log.clear();
        self.total_events = 0;
        self.selected_note_id = None;
    }
}

/// Returns a shortened version of an ID for display.
pub fn short_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}
