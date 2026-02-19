//! Root workspace state shared across all components.

use super::navigation::NavigationState;
use super::editor::EditorState;
use std::collections::HashMap;

/// The type of view currently displayed.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewType {
    Document,
    Story,
    Artifacts,
    Quest,
    Settings,
}

/// Phase of the application lifecycle.
#[derive(Clone, Debug, PartialEq)]
pub enum AppPhase {
    Loading,    // Checking is_first_run
    Setup,      // First run: collect display name
    Workspace,  // Main workspace (identity loaded)
}

/// Direction of a P2P event.
#[derive(Clone, Debug, PartialEq)]
pub enum EventDirection {
    Sent,
    Received,
    System,
}

/// A single entry in the network event log.
#[derive(Clone, Debug, PartialEq)]
pub struct EventLogEntry {
    pub timestamp: String,
    pub direction: EventDirection,
    pub message: String,
}

/// Display info for a peer in the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerDisplayInfo {
    pub name: String,
    pub letter: String,
    pub color_class: String,
    pub online: bool,
    pub player_id: [u8; 32],
}

/// Per-peer heat data for an artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerHeat {
    pub peer_name: String,
    pub heat: f32,
    pub color: String,
}

/// Peer state — connected peers and their heat data.
#[derive(Clone, Debug)]
pub struct PeerState {
    pub entries: Vec<PeerDisplayInfo>,
    pub heat_values: HashMap<String, Vec<PeerHeat>>,
}

impl PeerState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            heat_values: HashMap::new(),
        }
    }
}

/// UI-only state (sidebar visibility, detail panel, etc.)
#[derive(Clone, Debug)]
pub struct UiState {
    pub sidebar_open: bool,
    pub detail_open: bool,
    pub slash_menu_open: bool,
    pub active_detail_tab: usize,
    pub active_view: ViewType,
}

/// Root workspace state.
#[derive(Clone, Debug)]
pub struct WorkspaceState {
    pub nav: NavigationState,
    pub editor: EditorState,
    pub peers: PeerState,
    pub ui: UiState,
    pub phase: AppPhase,
    pub event_log: Vec<EventLogEntry>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            nav: NavigationState::new(),
            editor: EditorState::new(),
            peers: PeerState::new(),
            ui: UiState {
                sidebar_open: true,
                detail_open: false,
                slash_menu_open: false,
                active_detail_tab: 0,
                active_view: ViewType::Document,
            },
            phase: AppPhase::Loading,
            event_log: Vec::new(),
        }
    }
}

/// Append an event to the log (newest at index 0), truncating at 200 entries.
pub fn log_event(ws: &mut WorkspaceState, dir: EventDirection, msg: impl Into<String>) {
    let now = chrono::Local::now().format("%H:%M:%S").to_string();
    ws.event_log.insert(0, EventLogEntry {
        timestamp: now,
        direction: dir,
        message: msg.into(),
    });
    ws.event_log.truncate(200);
}
