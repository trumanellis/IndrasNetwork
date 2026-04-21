//! Application state types for The Synchronicity Engine.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use indras_sync_engine::team::LogicalAgentId;
use serde::{Deserialize, Serialize};

use crate::config::RelayConfig;
use crate::heartbeat::PeerLiveness;

/// Payload carried during a drag-to-share operation.
#[derive(Debug, Clone)]
pub struct DragPayload {
    /// Display name of the file being dragged.
    pub file_name: String,
    /// Full path to the file on disk (needed for upload).
    pub file_disk_path: PathBuf,
    /// Source realm (None = private vault). Prevents same-realm drops.
    pub source_realm: Option<RealmId>,
}

/// The current step in the application flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppStep {
    /// Welcome splash with name input and Create/SignIn choice.
    Welcome,
    /// Loading: creating identity + vault + seeding HelloWorld.md.
    Creating,
    /// Pass story entry framed as restore (sign-in flow).
    RestoreStory,
    /// Review narrative before confirming (restore flow).
    StoryReview,
    /// Loading: deriving keys + connecting + syncing.
    Restoring,
    /// Main view: realm columns + file modal.
    HomeVault,
}

/// Sync status of the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    /// All files synced across devices.
    Synced,
    /// Sync in progress.
    Syncing,
    /// No network connection.
    Offline,
    /// Sync error.
    Error(String),
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self::Offline
    }
}

/// View model for a file in the vault.
#[derive(Debug, Clone, PartialEq)]
pub struct FileView {
    /// Relative path within the vault (e.g., "HelloWorld.md").
    pub path: String,
    /// Filename only (e.g., "HelloWorld.md").
    pub name: String,
    /// File size in bytes.
    pub size: u64,
    /// Formatted relative time (e.g., "just now", "2m ago").
    pub modified: String,
    /// Raw timestamp for sorting (unix millis).
    pub modified_ms: i64,
}

/// Status of an async loading operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingStage {
    /// Operation in progress with a message.
    InProgress(String),
    /// Step completed.
    Done(String),
    /// Operation failed.
    Failed(String),
}

/// Category of a realm in the column layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RealmCategory {
    /// Personal vault (home realm).
    Private,
    /// Direct message with a single peer.
    Dm,
    /// Shared group realm.
    Group,
    /// World/discoverable realm.
    World,
}

/// A 32-byte realm identifier.
pub type RealmId = [u8; 32];

/// Display info for a connected peer in the peer bar.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerDisplayInfo {
    /// Display name (from contacts or hex-truncated member ID).
    pub name: String,
    /// First letter of name for avatar dot.
    pub letter: String,
    /// CSS class for dot color (e.g. "identity-love").
    pub color_class: String,
    /// Whether the peer is currently online.
    pub online: bool,
    /// Raw 32-byte member identifier.
    pub member_id: [u8; 32],
}

/// Rotating Member Identity Color classes (see DESIGN.md §2 Member Identity Colors).
pub const MEMBER_IDENTITY_CLASSES: &[&str] = &[
    "identity-love",
    "identity-joy",
    "identity-peace",
    "identity-grace",
    "identity-hope",
    "identity-faith",
];

/// Deterministic member-identity class for a 32-byte id (rotates by first byte).
pub fn member_class_for(id: &[u8; 32]) -> &'static str {
    MEMBER_IDENTITY_CLASSES[(id[0] as usize) % MEMBER_IDENTITY_CLASSES.len()]
}

/// Hex color for a member-identity class (mirrors styles.css `.identity-*`).
pub fn member_hex_for(id: &[u8; 32]) -> &'static str {
    match member_class_for(id) {
        "identity-love" => "#ff6b9d",
        "identity-joy" => "#ffd93d",
        "identity-peace" => "#6bcfff",
        "identity-grace" => "#b19cd9",
        "identity-hope" => "#98d8aa",
        "identity-faith" => "#ffb347",
        _ => "#818cf8",
    }
}

/// View model for a realm entry in the column UI.
#[derive(Debug, Clone)]
pub struct RealmView {
    /// Unique realm identifier.
    pub id: RealmId,
    /// Display name (peer name for DMs, realm name for groups).
    pub display_name: String,
    /// Which column this realm belongs to.
    pub category: RealmCategory,
    /// Number of members in the realm.
    pub member_count: usize,
    /// Files in this realm (loaded lazily on accordion expand).
    pub files: Vec<FileView>,
}

/// Field to sort files by within a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    /// Sort alphabetically by file name.
    Name,
    /// Sort by last modified date.
    Date,
    /// Sort by file size.
    Size,
}

/// Sort direction for column file lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Ascending order (A→Z, oldest→newest, smallest→largest).
    Asc,
    /// Descending order (Z→A, newest→oldest, largest→smallest).
    Desc,
}

/// Tracks column UI selection, expansion, focus, and sort state.
#[derive(Debug, Clone)]
pub struct VaultSelection {
    /// Set of realm IDs whose file accordions are expanded.
    pub expanded_realms: HashSet<RealmId>,
    /// The currently selected realm (None = private vault).
    pub selected_realm: Option<RealmId>,
    /// The currently selected file path within the selected realm.
    pub selected_file: Option<String>,
    /// Which column currently has keyboard focus (0=Private, 1=DM, 2=Group, 3=World).
    pub focused_column: usize,
    /// Field used to sort files in the focused column.
    pub sort_field: SortField,
    /// Sort direction for the focused column.
    pub sort_order: SortOrder,
}

impl Default for VaultSelection {
    fn default() -> Self {
        Self {
            expanded_realms: HashSet::new(),
            selected_realm: None,
            selected_file: None,
            focused_column: 0,
            sort_field: SortField::Date,
            sort_order: SortOrder::Desc,
        }
    }
}

/// Context menu state shown on right-click over a file.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    /// Realm the file belongs to (None = private vault).
    pub realm_id: Option<RealmId>,
    /// Path of the file the menu was opened on.
    pub file_path: String,
    /// Horizontal position of the menu in logical pixels.
    pub x: f64,
    /// Vertical position of the menu in logical pixels.
    pub y: f64,
}

/// Which file is open in the modal (if any).
#[derive(Debug, Clone)]
pub struct ModalFile {
    /// Realm this file belongs to (None = private vault).
    pub realm_id: Option<RealmId>,
    /// Filename/path of the open file.
    pub file_path: String,
}

/// What the Braid Drawer is scoped to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BraidFocus {
    /// Show the full braid for a realm.
    Realm(RealmId),
    /// Scope the braid view to a single file within a realm.
    File {
        /// The realm the file lives in.
        realm: RealmId,
        /// File path within that realm.
        path: String,
    },
}

impl BraidFocus {
    /// The realm id being focused (either side of the enum).
    pub fn realm_id(&self) -> &RealmId {
        match self {
            BraidFocus::Realm(r) => r,
            BraidFocus::File { realm, .. } => realm,
        }
    }
}

/// Evidence badge as shown on a commit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceView {
    /// Automated verification passed (tests/build/lint).
    AgentPass {
        /// Summary string, e.g. "tests 12/12 · build · lint".
        summary: String,
    },
    /// Automated verification failed.
    AgentFail {
        /// Short reason shown on the badge.
        reason: String,
    },
    /// Human-approved commit.
    Human {
        /// Optional user message shown in tooltip.
        message: Option<String>,
    },
}

/// One changeset row as rendered in the drawer and graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitView {
    /// Short numeric id, e.g. "c6".
    pub short_id: String,
    /// First 8 hex chars of the ChangeId.
    pub short_hex: String,
    /// Author user id (raw 32 bytes).
    pub author_id: [u8; 32],
    /// Resolved author display name.
    pub author_name: String,
    /// Hex color for the author (from member identity palette).
    pub author_color: String,
    /// Commit intent / message.
    pub intent: String,
    /// Short hexes of parent change ids.
    pub parents: Vec<String>,
    /// Evidence badge.
    pub evidence: EvidenceView,
    /// Authoring time (ms since epoch).
    pub timestamp_ms: i64,
    /// Human-relative age (e.g. "2m", "18s").
    pub relative_time: String,
    /// True when this commit has 2+ parents.
    pub is_merge: bool,
    /// Y-lane (peer-row) assignment for graph layout.
    pub lane: usize,
    /// X-slot (temporal column) assignment for graph layout.
    pub slot: usize,
}

/// One peer's HEAD within a braid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerHeadView {
    /// Raw user id.
    pub user_id: [u8; 32],
    /// Display name.
    pub name: String,
    /// Hex color.
    pub color: String,
    /// Is this the local user?
    pub is_self: bool,
    /// Short numeric id of their HEAD (e.g. "c7").
    pub head_short_id: String,
    /// 8-char hex of their HEAD ChangeId.
    pub head_short_hex: String,
    /// Number of files in their head index.
    pub file_count: usize,
    /// Relative time since last HEAD update.
    pub relative_time: String,
    /// Is this peer currently diverged from local HEAD? (Pending fork.)
    pub is_diverged: bool,
}

/// Unresolved three-way merge conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictView {
    /// Logical path in conflict.
    pub path: String,
    /// Short hex of our side's content address.
    pub ours_hex: String,
    /// Short hex of their side's content address.
    pub theirs_hex: String,
    /// Peer name who authored the other side.
    pub theirs_peer: String,
}

/// Runtime liveness of an agent as observed by hook events.
///
/// Transitions:
/// - `Idle` → `Thinking` on any hook event from the agent.
/// - `Thinking` → `Idle` when the `Stop` event arrives.
/// - `Thinking` → `Crashed` if no hook event arrives for > 10 minutes
///   (stall detection in the polling loop).
///
/// Stored in [`AppState::agent_status`] keyed by [`LogicalAgentId`].
/// Must be serde-compatible so it can flow through `save_world_view`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeStatus {
    /// Agent is connected but not actively processing.
    #[default]
    Idle,
    /// Agent is actively processing (hook events still arriving).
    Thinking,
    /// Agent appears stuck — no hook event for > 10 minutes.
    Crashed,
}

/// Derived row state for one agent in the Agent Roster UI.
///
/// Computed synchronously from the four inputs by [`agent_row_state`].
/// Drives which CSS modifier class and action pills are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRowState {
    /// Agent folder is locked by another process; retry pill shown.
    Blocked,
    /// Agent is connected but idle; no action pills.
    Idle,
    /// Agent has active hook events; spinner shown.
    Thinking,
    /// Agent has uncommitted changes; \[land\] pill shown.
    HasChanges,
    /// Agent has an inner-braid fork ready to review; \[review\] pill shown.
    ForkReady,
}

/// Derive the row display state from the four observable inputs.
///
/// Priority order (highest first):
/// 1. `Blocked` — handle not present (folder lock failed); overrides all.
/// 2. `ForkReady` — inner fork exists; overrides `HasChanges`.
/// 3. `HasChanges` — uncommitted changes but no fork yet.
/// 4. `Thinking` — runtime says agent is active.
/// 5. `Idle` — default.
pub fn agent_row_state(
    handle_present: bool,
    runtime: AgentRuntimeStatus,
    uncommitted_change_count: usize,
    has_inner_fork: bool,
) -> AgentRowState {
    if !handle_present {
        return AgentRowState::Blocked;
    }
    if has_inner_fork {
        return AgentRowState::ForkReady;
    }
    if uncommitted_change_count > 0 {
        return AgentRowState::HasChanges;
    }
    if runtime == AgentRuntimeStatus::Thinking {
        return AgentRowState::Thinking;
    }
    AgentRowState::Idle
}

/// One diverged agent whose local HEAD hasn't been merged into the user's
/// inner HEAD yet. Rendered as a row in the Private column's Agent Lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentForkView {
    /// Logical agent name (e.g. "researcher", "coder").
    pub name: String,
    /// Realm whose inner braid this agent is forking in.
    pub realm_id: RealmId,
    /// Number of changesets on the agent's branch ahead of the user's HEAD.
    pub change_count: usize,
    /// 8-char hex of the agent's HEAD `ChangeId` for tooltip.
    pub head_short_hex: String,
    /// Member-identity CSS class used to tint the agent strand.
    pub color_class: &'static str,
    /// Hex color matching `color_class`, for inline SVG stops.
    pub color_hex: &'static str,
    /// Live runtime status of the agent, derived from hook events and
    /// fs-recency heuristics.
    pub runtime_status: AgentRuntimeStatus,
}

/// Pre-computed snapshot of a braid for the drawer/graph to render.
///
/// Built by a bridge task from a `Vault`'s `BraidDag`, then stored in
/// `AppState::braid_view` for synchronous rendering.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BraidView {
    /// Which realm this snapshot is for.
    pub realm_id: RealmId,
    /// Peers seen in this DAG, in lane-assignment order.
    pub peers: Vec<PeerHeadView>,
    /// All commits, newest first.
    pub commits: Vec<CommitView>,
    /// Peers whose HEAD diverges from local (pending merges).
    pub pending_forks: Vec<PeerHeadView>,
    /// Unresolved conflicts.
    pub conflicts: Vec<ConflictView>,
}

/// Root application state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current screen in the flow.
    pub step: AppStep,
    /// Display name entered during creation.
    pub display_name: String,
    /// Current sync status.
    pub sync_status: SyncStatus,
    /// Private vault files (home realm, local filesystem).
    pub private_files: Vec<FileView>,
    /// All known realms, categorized for column display.
    pub realms: Vec<RealmView>,
    /// Column UI selection and expansion state.
    pub selection: VaultSelection,
    /// File currently open in the modal (None = modal closed).
    pub modal_file: Option<ModalFile>,
    /// Path to the private vault folder on disk.
    pub vault_path: PathBuf,
    /// Number of connected devices.
    pub device_count: u32,
    /// Progress stages during creation/restore.
    pub loading_stages: Vec<LoadingStage>,
    /// Error message if something went wrong.
    pub error: Option<String>,
    /// Raw slot values from the pass story entry, stored for vault creation.
    pub pass_story_slots: Vec<String>,
    /// Context menu state (right-click on file).
    pub context_menu: Option<ContextMenu>,
    /// File currently being renamed (path within its realm).
    pub renaming_file: Option<String>,
    /// Whether the contact invite overlay is open.
    pub show_contact_invite: bool,
    /// Whether the create group overlay is open.
    pub show_create_group: bool,
    /// Whether the create public vault overlay is open.
    pub show_create_public: bool,
    /// Active drag-to-share payload (None when not dragging).
    pub drag_payload: Option<DragPayload>,
    /// Realm ID currently being hovered as a drop target (for CSS highlighting).
    pub drop_target_realm: Option<RealmId>,
    /// Whether the relay-settings overlay is open.
    pub show_relay_settings: bool,
    /// Whether the profile overlay is open.
    pub show_profile: bool,
    /// Whether the sync panel overlay is open.
    pub show_sync: bool,
    /// Whether the steward-recovery setup overlay is open.
    pub show_recovery_setup: bool,
    /// Whether the "use my backup" recovery overlay is open.
    pub show_recovery_use: bool,
    /// Currently-open peer profile popup, keyed by (peer member id, DM realm id).
    /// `None` = popup closed.
    pub profile_popup_target: Option<([u8; 32], RealmId)>,
    /// Heartbeat liveness map populated by the heartbeat receiver task.
    /// `None` until the network has started.
    pub peer_liveness: Option<Arc<PeerLiveness>>,
    /// Cached relay configuration loaded from `$INDRAS_DATA_DIR/relay.json`.
    pub relay_config: RelayConfig,
    /// Is the right-docked Braid Drawer open?
    pub braid_drawer_open: bool,
    /// What the drawer is focused on (realm or file), if any.
    pub braid_drawer_focus: Option<BraidFocus>,
    /// Cached braid snapshot for the focused realm.
    pub braid_view: Option<BraidView>,
    /// Diverged agents, across every attached vault's inner braid. Drives
    /// the Agent Lane strip in the Private column — hidden when empty.
    pub agent_forks: Vec<AgentForkView>,
    /// Realm whose sync is currently in flight (set by the sync handler,
    /// cleared when the final step fires). Used to paint an "aurora" on
    /// the owning column while work is in motion.
    pub syncing_realm: Option<RealmId>,
    /// Live runtime status for each hosted agent, updated by the IPC hook
    /// handler when `indras-agent-hook` fires lifecycle events.
    ///
    /// Keyed by full [`LogicalAgentId`] (e.g. `"agent-foo"`).
    /// Serde-compatible (no `Instant`) so it can flow into `save_world_view`.
    pub agent_status: HashMap<LogicalAgentId, AgentRuntimeStatus>,
    /// Millisecond timestamp (since epoch) of the last hook event received
    /// for each agent. Used by the stall-detection loop: if
    /// `runtime == Thinking` and `now - last_activity > 10 * 60 * 1000`,
    /// transition the agent to `Crashed`.
    pub agent_last_activity_millis: HashMap<LogicalAgentId, u64>,
}

impl AppState {
    /// Create initial state, detecting whether identity already exists.
    pub fn new() -> Self {
        let vault_path = default_vault_path();
        let data_dir = default_data_dir();
        let first_run = indras_network::IndrasNetwork::is_first_run(&data_dir);

        Self {
            step: if first_run { AppStep::Welcome } else { AppStep::HomeVault },
            display_name: String::new(),
            sync_status: SyncStatus::default(),
            private_files: Vec::new(),
            realms: Vec::new(),
            selection: VaultSelection::default(),
            modal_file: None,
            vault_path,
            device_count: 1,
            loading_stages: Vec::new(),
            error: None,
            pass_story_slots: Vec::new(),
            context_menu: None,
            renaming_file: None,
            show_contact_invite: false,
            show_create_group: false,
            show_create_public: false,
            drag_payload: None,
            drop_target_realm: None,
            show_relay_settings: false,
            show_profile: false,
            show_sync: false,
            show_recovery_setup: false,
            show_recovery_use: false,
            profile_popup_target: None,
            peer_liveness: None,
            relay_config: RelayConfig::load(),
            braid_drawer_open: false,
            braid_drawer_focus: None,
            braid_view: None,
            agent_forks: Vec::new(),
            syncing_realm: None,
            agent_status: HashMap::new(),
            agent_last_activity_millis: HashMap::new(),
        }
    }
}

/// Get the default data directory for identity/keys.
///
/// Respects `INDRAS_DATA_DIR` env var for multi-instance mode.
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("INDRAS_DATA_DIR") {
        return PathBuf::from(dir);
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Application Support/indras-network");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_DIR") {
            return PathBuf::from(xdg).join("indras-network");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/share/indras-network");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("indras-network");
        }
    }
    PathBuf::from(".").join("indras-network")
}

/// Get the default vault folder path placeholder.
///
/// The real home vault path is `{data_dir}/vaults/<sanitize(self_name)>/`
/// and gets set by `vault_bridge` once the display name is known
/// (account create/restore) or by `app.rs` for returning users once
/// the network loads. Until then, `SYNC_ENGINE_VAULT` (tests/overrides)
/// or `{data_dir}/vaults/default` is used as a placeholder.
pub fn default_vault_path() -> PathBuf {
    if let Ok(dir) = std::env::var("SYNC_ENGINE_VAULT") {
        return PathBuf::from(dir);
    }
    default_data_dir().join("vaults").join("default")
}

/// Format a unix millis timestamp as a relative time string.
pub fn format_relative_time(ms: i64) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let diff_secs = (now - ms) / 1000;

    if diff_secs < 5 {
        "just now".to_string()
    } else if diff_secs < 60 {
        format!("{}s ago", diff_secs)
    } else if diff_secs < 3600 {
        format!("{}m ago", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h ago", diff_secs / 3600)
    } else {
        format!("{}d ago", diff_secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_overrides_everything() {
        assert_eq!(
            agent_row_state(false, AgentRuntimeStatus::Thinking, 99, true),
            AgentRowState::Blocked
        );
    }

    #[test]
    fn fork_ready_overrides_has_changes() {
        assert_eq!(
            agent_row_state(true, AgentRuntimeStatus::Idle, 5, true),
            AgentRowState::ForkReady
        );
    }

    #[test]
    fn has_changes_when_uncommitted_work() {
        assert_eq!(
            agent_row_state(true, AgentRuntimeStatus::Idle, 3, false),
            AgentRowState::HasChanges
        );
    }

    #[test]
    fn thinking_when_runtime_active() {
        assert_eq!(
            agent_row_state(true, AgentRuntimeStatus::Thinking, 0, false),
            AgentRowState::Thinking
        );
    }

    #[test]
    fn idle_is_default() {
        assert_eq!(
            agent_row_state(true, AgentRuntimeStatus::Idle, 0, false),
            AgentRowState::Idle
        );
    }

    #[test]
    fn crashed_agent_with_handle_shows_idle_not_blocked() {
        // Crashed is a runtime status, not a BindError — handle is present.
        // Row shows Idle (Crashed doesn't affect agent_row_state directly;
        // the UI renders the crashed indicator via runtime_status separately).
        assert_eq!(
            agent_row_state(true, AgentRuntimeStatus::Crashed, 0, false),
            AgentRowState::Idle
        );
    }
}
