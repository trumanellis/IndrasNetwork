//! Main application state
//!
//! Coordinates all sub-states and handles event processing.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::events::{StreamEvent, EventCategory};

use super::{RealmState, QuestState, AttentionState, ContactsState};

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
            StreamEvent::Info { message, .. } => {
                message.chars().take(50).collect()
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
    /// Contacts tracking state
    pub contacts: ContactsState,
    /// Recent events for log panel (newest first)
    pub event_log: VecDeque<LoggedEvent>,
    /// Maximum events to keep in log
    pub max_log_events: usize,
    /// Total events processed
    pub total_events: usize,
    /// Currently selected point-of-view member (None = overview mode)
    pub selected_pov: Option<String>,
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
            | StreamEvent::MemberLeft { .. } => {
                self.realms.process_event(&event);
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

            _ => {}
        }

        self.total_events += 1;
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
