//! Application state types for The Synchronicity Engine.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::RelayConfig;

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
    /// CSS class for dot color (e.g. "peer-dot-sage").
    pub color_class: String,
    /// Whether the peer is currently online.
    pub online: bool,
    /// Raw 32-byte member identifier.
    pub member_id: [u8; 32],
}

/// Rotating color classes for peer dots.
pub const PEER_COLORS: &[&str] = &["peer-dot-sage", "peer-dot-zeph", "peer-dot-rose"];

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
    /// Cached relay configuration loaded from `$INDRAS_DATA_DIR/relay.json`.
    pub relay_config: RelayConfig,
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
            relay_config: RelayConfig::load(),
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

/// Get the default vault folder path.
///
/// Respects `SYNC_ENGINE_VAULT` env var. Defaults to ~/SyncEngine/.
pub fn default_vault_path() -> PathBuf {
    if let Ok(dir) = std::env::var("SYNC_ENGINE_VAULT") {
        return PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join("SyncEngine");
    }
    PathBuf::from(".").join("SyncEngine")
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
