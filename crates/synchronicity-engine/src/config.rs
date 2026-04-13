//! Persistent relay-node configuration for the Synchronicity Engine app.
//!
//! Stored as JSON at `$INDRAS_DATA_DIR/relay.json`. Loaded on app start,
//! re-saved on every overlay edit. Live reconfig of the running network
//! is out of scope; values take effect on next launch.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::state::default_data_dir;

/// Persisted relay configuration. Mirrors the subset of
/// `indras_network::NetworkConfig` that the user can edit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelayConfig {
    /// Relay server URLs.
    pub servers: Vec<String>,
    /// When true, disable relay + DNS discovery (LAN-only).
    pub local_only: bool,
    /// One of: "Default", "Chat", "Collaboration", "IoT", "OfflineFirst".
    pub preset: String,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            local_only: false,
            preset: "Default".to_string(),
        }
    }
}

impl RelayConfig {
    /// Path to the relay config JSON file inside `$INDRAS_DATA_DIR`.
    pub fn path() -> PathBuf {
        default_data_dir().join("relay.json")
    }

    /// Path within an explicit data directory (used for tests).
    pub fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join("relay.json")
    }

    /// Load from `$INDRAS_DATA_DIR/relay.json`, returning defaults if missing.
    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist to `$INDRAS_DATA_DIR/relay.json`. Creates parent dir if needed.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&path, json)
    }
}

/// Available preset names in display order.
pub const PRESET_NAMES: &[&str] = &["Default", "Chat", "Collaboration", "IoT", "OfflineFirst"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let cfg = RelayConfig {
            servers: vec!["https://relay.example".to_string()],
            local_only: true,
            preset: "Chat".to_string(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: RelayConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn default_is_sensible() {
        let cfg = RelayConfig::default();
        assert!(cfg.servers.is_empty());
        assert!(!cfg.local_only);
        assert_eq!(cfg.preset, "Default");
    }
}
