//! Application state types for The Synchronicity Engine.

use std::path::PathBuf;

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
    /// Main view: file list + preview + vault info.
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
#[derive(Debug, Clone)]
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

/// Root application state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current screen in the flow.
    pub step: AppStep,
    /// Display name entered during creation.
    pub display_name: String,
    /// Current sync status.
    pub sync_status: SyncStatus,
    /// Files in the vault.
    pub files: Vec<FileView>,
    /// Currently selected file path.
    pub selected_file: Option<String>,
    /// Rendered HTML content of selected file.
    pub selected_content: Option<String>,
    /// Path to the vault folder on disk.
    pub vault_path: PathBuf,
    /// Number of connected devices.
    pub device_count: u32,
    /// Progress stages during creation/restore.
    pub loading_stages: Vec<LoadingStage>,
    /// Error message if something went wrong.
    pub error: Option<String>,
    /// Raw slot values from the pass story entry, stored for vault creation.
    pub pass_story_slots: Vec<String>,
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
            files: Vec::new(),
            selected_file: None,
            selected_content: None,
            vault_path,
            device_count: 1,
            loading_stages: Vec::new(),
            error: None,
            pass_story_slots: Vec::new(),
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
