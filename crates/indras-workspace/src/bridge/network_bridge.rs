//! Network bridge — connects IndrasNetwork to Dioxus signals.

use std::sync::Arc;
use indras_network::{IndrasNetwork, HomeRealm};

/// Handle to the running IndrasNetwork instance.
#[derive(Clone)]
pub struct NetworkHandle {
    pub network: Arc<IndrasNetwork>,
}

impl NetworkHandle {
    /// Initialize and return the home realm (creates if needed).
    pub async fn home_realm(&self) -> Result<HomeRealm, String> {
        self.network.home_realm().await.map_err(|e| format!("{}", e))
    }
}

/// Platform-specific data directory for identity persistence.
pub fn default_data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("INDRAS_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    // macOS: ~/Library/Application Support/indras-network
    // Linux: ~/.local/share/indras-network
    // Windows: %APPDATA%/indras-network
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library/Application Support/indras-network");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return std::path::PathBuf::from(xdg).join("indras-network");
        }
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join(".local/share/indras-network");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return std::path::PathBuf::from(appdata).join("indras-network");
        }
    }
    std::path::PathBuf::from(".").join("indras-network")
}

/// Check if this is the user's first run (no identity keys on disk).
pub fn is_first_run() -> bool {
    IndrasNetwork::is_first_run(default_data_dir())
}

/// Create a new identity with display name and optional PassStory protection.
pub async fn create_identity(
    display_name: &str,
    pass_story_slots: Option<[String; 23]>,
) -> Result<NetworkHandle, String> {
    let data_dir = default_data_dir();
    let _ = std::fs::create_dir_all(&data_dir);

    let mut builder = IndrasNetwork::builder()
        .data_dir(&data_dir)
        .display_name(display_name);

    if let Some(slots) = pass_story_slots {
        let story = indras_crypto::PassStory::from_normalized(slots)
            .map_err(|e| format!("{}", e))?;
        builder = builder.pass_story(story);
    }

    let net = builder
        .build()
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(NetworkHandle {
        network: Arc::new(net),
    })
}

/// Load an existing identity (returning user).
pub async fn load_identity() -> Result<NetworkHandle, String> {
    let data_dir = default_data_dir();

    let net = IndrasNetwork::new(&data_dir)
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(NetworkHandle {
        network: Arc::new(net),
    })
}
