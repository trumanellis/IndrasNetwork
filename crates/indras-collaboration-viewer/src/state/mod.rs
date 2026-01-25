// State management for Collaboration Viewer

use serde::{Deserialize, Serialize};

/// The three peers in the collaboration scenario
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Peer {
    Love,
    Joy,
    Peace,
}

impl Peer {
    pub fn all() -> &'static [Peer] {
        &[Peer::Love, Peer::Joy, Peer::Peace]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Peer::Love => "love",
            Peer::Joy => "joy",
            Peer::Peace => "peace",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Peer::Love => "Love",
            Peer::Joy => "Joy",
            Peer::Peace => "Peace",
        }
    }

    pub fn initial(&self) -> char {
        match self {
            Peer::Love => 'L',
            Peer::Joy => 'J',
            Peer::Peace => 'P',
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Peer::Love => "love",
            Peer::Joy => "joy",
            Peer::Peace => "peace",
        }
    }

    /// Position in the triangle visualization (normalized 0-1 coordinates)
    pub fn position(&self) -> (f64, f64) {
        match self {
            Peer::Love => (0.5, 0.15),   // Top center
            Peer::Joy => (0.15, 0.85),   // Bottom left
            Peer::Peace => (0.85, 0.85), // Bottom right
        }
    }
}

/// Quest status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestStatus {
    Pending,
    InProgress,
    Completed,
}

impl QuestStatus {
    pub fn display_name(&self) -> &'static str {
        match self {
            QuestStatus::Pending => "Pending",
            QuestStatus::InProgress => "In Progress",
            QuestStatus::Completed => "Completed",
        }
    }
}

/// A quest in the quest log
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quest {
    pub id: u32,
    pub title: String,
    pub creator: Peer,
    pub assignee: Peer,
    pub status: QuestStatus,
}

/// A section in the project plan
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanSection {
    pub id: u32,
    pub author: Peer,
    pub content: String,
}

/// An event in the simulation timeline
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimEvent {
    pub tick: u32,
    pub event_type: EventType,
    pub message: String,
    pub peer: Option<Peer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    Setup,
    QuestCreated,
    QuestUpdated,
    DocumentSection,
    Sync,
    PhaseComplete,
}

impl EventType {
    pub fn icon(&self) -> &'static str {
        match self {
            EventType::Setup => "🔧",
            EventType::QuestCreated => "📋",
            EventType::QuestUpdated => "✅",
            EventType::DocumentSection => "📝",
            EventType::Sync => "🔄",
            EventType::PhaseComplete => "🎉",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            EventType::Setup => "setup",
            EventType::QuestCreated | EventType::QuestUpdated => "quest",
            EventType::DocumentSection => "document",
            EventType::Sync => "sync",
            EventType::PhaseComplete => "phase",
        }
    }
}

/// A packet animation between peers
#[derive(Clone, Debug, PartialEq)]
pub struct PacketAnimation {
    pub from: Peer,
    pub to: Peer,
    pub progress: f64, // 0.0 to 1.0
    pub message_type: String,
}

impl PacketAnimation {
    pub fn position(&self) -> (f64, f64) {
        let (x1, y1) = self.from.position();
        let (x2, y2) = self.to.position();
        let t = self.progress;
        (x1 + (x2 - x1) * t, y1 + (y2 - y1) * t)
    }
}

/// Simulation phase
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Setup,
    QuestCreation,
    DocumentCollaboration,
    QuestUpdates,
    Verification,
    Complete,
}

impl Phase {
    pub fn number(&self) -> u32 {
        match self {
            Phase::Setup => 1,
            Phase::QuestCreation => 2,
            Phase::DocumentCollaboration => 3,
            Phase::QuestUpdates => 4,
            Phase::Verification => 5,
            Phase::Complete => 5,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Phase::Setup => "Setup",
            Phase::QuestCreation => "Quest Creation",
            Phase::DocumentCollaboration => "Document Collaboration",
            Phase::QuestUpdates => "Quest Updates",
            Phase::Verification => "Verification",
            Phase::Complete => "Complete",
        }
    }
}

/// Peer state for the visualization
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PeerState {
    pub online: bool,
    pub quests_created: u32,
    pub quests_assigned: u32,
    pub sections_written: u32,
    pub messages_sent: u32,
}

/// Main simulation state
#[derive(Clone, Debug, PartialEq)]
pub struct CollaborationState {
    pub tick: u32,
    pub max_tick: u32,
    pub phase: Phase,
    pub paused: bool,
    pub speed: f64, // Ticks per second

    // Peer states
    pub peer_states: std::collections::HashMap<Peer, PeerState>,

    // Data
    pub quests: Vec<Quest>,
    pub plan_title: String,
    pub plan_sections: Vec<PlanSection>,
    pub events: Vec<SimEvent>,

    // Animations
    pub active_packets: Vec<PacketAnimation>,
    pub active_edges: Vec<(Peer, Peer)>,
    pub highlighted_quest: Option<u32>,
}

impl Default for CollaborationState {
    fn default() -> Self {
        let mut peer_states = std::collections::HashMap::new();
        for peer in Peer::all() {
            peer_states.insert(
                *peer,
                PeerState {
                    online: false,
                    ..Default::default()
                },
            );
        }

        Self {
            tick: 0,
            max_tick: 50,
            phase: Phase::Setup,
            paused: true,
            speed: 2.0,
            peer_states,
            quests: Vec::new(),
            plan_title: "Harmony Initiative".to_string(),
            plan_sections: Vec::new(),
            events: Vec::new(),
            active_packets: Vec::new(),
            active_edges: Vec::new(),
            highlighted_quest: None,
        }
    }
}

impl CollaborationState {
    /// Get quests by status
    pub fn quests_by_status(&self, status: QuestStatus) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|q| q.status == status)
            .collect()
    }

    /// Add an event
    pub fn add_event(&mut self, event_type: EventType, message: String, peer: Option<Peer>) {
        self.events.push(SimEvent {
            tick: self.tick,
            event_type,
            message,
            peer,
        });
        // Keep only last 50 events
        if self.events.len() > 50 {
            self.events.remove(0);
        }
    }

    /// Create a packet animation
    pub fn send_packet(&mut self, from: Peer, to: Peer, message_type: &str) {
        self.active_packets.push(PacketAnimation {
            from,
            to,
            progress: 0.0,
            message_type: message_type.to_string(),
        });
        self.active_edges.push((from, to));
    }

    /// Update packet animations (call each frame)
    pub fn update_animations(&mut self, delta: f64) {
        // Update packet progress
        for packet in &mut self.active_packets {
            packet.progress += delta * 2.0; // 0.5 seconds per packet
        }

        // Remove completed packets
        self.active_packets.retain(|p| p.progress < 1.0);

        // Clear edges if no packets
        if self.active_packets.is_empty() {
            self.active_edges.clear();
        }
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Predefined scenario data
pub struct ScenarioData;

impl ScenarioData {
    /// All quests to be created
    pub fn quests() -> Vec<(Peer, &'static str, Peer)> {
        vec![
            (Peer::Love, "Spread kindness in the community", Peer::Joy),
            (Peer::Love, "Write a gratitude journal", Peer::Love),
            (Peer::Joy, "Organize a celebration event", Peer::Peace),
            (Peer::Joy, "Create a playlist of uplifting songs", Peer::Love),
            (Peer::Peace, "Meditate for inner calm", Peer::Peace),
            (Peer::Peace, "Resolve a conflict with compassion", Peer::Joy),
        ]
    }

    /// Project plan sections
    pub fn plan_sections() -> Vec<(Peer, &'static str)> {
        vec![
            (
                Peer::Love,
                "Our mission is to create a world where compassion guides every action. Through acts of kindness, we build bridges between hearts.",
            ),
            (
                Peer::Joy,
                "Celebration is our tool for transformation. When we find joy in small moments, we amplify positivity for all.",
            ),
            (
                Peer::Peace,
                "Inner calm creates outer harmony. Through mindfulness and understanding, conflicts dissolve into cooperation.",
            ),
        ]
    }
}
