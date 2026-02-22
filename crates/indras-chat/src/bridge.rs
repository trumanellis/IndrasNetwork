//! Network bridge — connects IndrasNetwork to Dioxus signals.

use std::path::PathBuf;
use std::sync::Arc;
use indras_network::IndrasNetwork;

/// Handle to the running IndrasNetwork instance.
#[derive(Clone)]
pub struct NetworkHandle {
    pub network: Arc<IndrasNetwork>,
}

impl std::fmt::Debug for NetworkHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkHandle").finish_non_exhaustive()
    }
}

impl PartialEq for NetworkHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.network, &other.network)
    }
}

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

    net.start().await.map_err(|e| format!("{}", e))?;

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

    net.start().await.map_err(|e| format!("{}", e))?;

    Ok(NetworkHandle {
        network: Arc::new(net),
    })
}
