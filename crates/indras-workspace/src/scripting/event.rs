//! Events emitted by the app that Lua scripts can observe via the EventBus.

/// Events emitted by the app that Lua scripts can wait on.
#[derive(Debug, Clone)]
pub enum AppEvent {
    // Lifecycle
    AppReady,
    IdentityCreated(String),

    // Peers
    PeerConnected(String),
    PeerDisconnected(String),

    // Navigation
    ViewChanged(String),
    SidebarItemActive(String),

    // Messaging
    MessageReceived { from: String, text: String },
    MessageSent { text: String },

    // Overlay
    OverlayOpened(String),
    OverlayClosed(String),

    // Artifacts
    ArtifactStored(String),
    ArtifactGranted { artifact_name: String, peer_name: String },

    // Errors
    ActionFailed { action: String, error: String },
}

impl AppEvent {
    /// Get the event name for matching in Lua's wait_for().
    pub fn name(&self) -> &'static str {
        match self {
            AppEvent::AppReady => "app_ready",
            AppEvent::IdentityCreated(_) => "identity_created",
            AppEvent::PeerConnected(_) => "peer_connected",
            AppEvent::PeerDisconnected(_) => "peer_disconnected",
            AppEvent::ViewChanged(_) => "view_changed",
            AppEvent::SidebarItemActive(_) => "sidebar_item_active",
            AppEvent::MessageReceived { .. } => "message_received",
            AppEvent::MessageSent { .. } => "message_sent",
            AppEvent::OverlayOpened(_) => "overlay_opened",
            AppEvent::OverlayClosed(_) => "overlay_closed",
            AppEvent::ArtifactStored(_) => "artifact_stored",
            AppEvent::ArtifactGranted { .. } => "artifact_granted",
            AppEvent::ActionFailed { .. } => "action_failed",
        }
    }

    /// Check if this event matches a name and optional filter string.
    pub fn matches(&self, event_name: &str, filter: Option<&str>) -> bool {
        if self.name() != event_name {
            return false;
        }
        let Some(filter) = filter else { return true };
        match self {
            AppEvent::IdentityCreated(name) => name == filter,
            AppEvent::PeerConnected(name) => name == filter,
            AppEvent::PeerDisconnected(name) => name == filter,
            AppEvent::ViewChanged(view) => view == filter,
            AppEvent::SidebarItemActive(label) => label == filter,
            AppEvent::MessageReceived { from, .. } => from == filter,
            AppEvent::MessageSent { text } => text == filter,
            AppEvent::OverlayOpened(name) => name == filter,
            AppEvent::OverlayClosed(name) => name == filter,
            AppEvent::ArtifactStored(name) => name == filter,
            AppEvent::ArtifactGranted { artifact_name, .. } => artifact_name == filter,
            _ => true,
        }
    }
}
