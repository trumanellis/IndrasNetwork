//! Network bridge — connects PeeringRuntime to Dioxus signals.

use std::path::PathBuf;
use std::sync::Arc;
use indras_peering::{PeeringConfig, PeeringRuntime};

/// Platform-specific data directory for identity persistence.
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("INDRAS_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Application Support/indras-chat");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("indras-chat");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/share/indras-chat");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("indras-chat");
        }
    }
    PathBuf::from(".").join("indras-chat")
}

/// Check if this is the user's first run (no identity keys on disk).
pub fn is_first_run() -> bool {
    PeeringRuntime::is_first_run(default_data_dir())
}

/// Create a new identity with display name and optional PassStory protection.
pub async fn create_identity(
    display_name: &str,
    pass_story_slots: Option<[String; 23]>,
) -> Result<Arc<PeeringRuntime>, String> {
    let data_dir = default_data_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let config = PeeringConfig::new(&data_dir);

    let pass_story = match pass_story_slots {
        Some(slots) => {
            let story = indras_crypto::PassStory::from_normalized(slots)
                .map_err(|e| format!("{e}"))?;
            Some(story)
        }
        None => None,
    };

    let runtime = PeeringRuntime::create(display_name, pass_story, config)
        .await
        .map_err(|e| format!("{e}"))?;

    Ok(Arc::new(runtime))
}

/// Load an existing identity (returning user).
pub async fn load_identity() -> Result<Arc<PeeringRuntime>, String> {
    let data_dir = default_data_dir();
    let config = PeeringConfig::new(&data_dir);

    let runtime = PeeringRuntime::boot(config)
        .await
        .map_err(|e| format!("{e}"))?;

    Ok(Arc::new(runtime))
}
