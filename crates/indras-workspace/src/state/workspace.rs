//! Root workspace state shared across all components.

use super::navigation::NavigationState;
use super::editor::EditorState;
use std::collections::HashMap;

/// The type of view currently displayed.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewType {
    Document,
    Story,
    Quest,
    Settings,
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
        }
    }
}
