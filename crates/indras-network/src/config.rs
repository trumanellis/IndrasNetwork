//! Configuration and presets for IndrasNetwork.
//!
//! Provides sensible defaults with the ability to customize behavior
//! through the builder pattern.

use indras_node::NodeConfig;
use std::path::PathBuf;

/// Preset configurations for common use cases.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Preset {
    /// Balanced defaults for general use.
    #[default]
    Default,
    /// Optimized for real-time chat applications.
    /// - Fast sync interval (1s)
    /// - Large event channel (4096)
    Chat,
    /// Optimized for collaborative document editing.
    /// - Frequent document sync (2s)
    /// - Medium event channel (2048)
    Collaboration,
    /// Minimal footprint for IoT/embedded devices.
    /// - Slow sync interval (30s)
    /// - Small event channel (256)
    IoT,
    /// Maximum tolerance for disconnection.
    /// - DTN routing enabled
    /// - Store-and-forward optimization
    OfflineFirst,
}

impl Preset {
    /// Get the event channel capacity for this preset.
    pub fn event_channel_capacity(&self) -> usize {
        match self {
            Preset::Default => 1024,
            Preset::Chat => 4096,
            Preset::Collaboration => 2048,
            Preset::IoT => 256,
            Preset::OfflineFirst => 1024,
        }
    }

    /// Get the sync interval in seconds for this preset.
    pub fn sync_interval_secs(&self) -> u64 {
        match self {
            Preset::Default => 5,
            Preset::Chat => 1,
            Preset::Collaboration => 2,
            Preset::IoT => 30,
            Preset::OfflineFirst => 5,
        }
    }
}

/// Configuration for the IndrasNetwork SDK.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Data directory for persistence.
    pub data_dir: PathBuf,
    /// Display name for this node.
    pub display_name: Option<String>,
    /// Relay servers for NAT traversal.
    pub relay_servers: Vec<String>,
    /// Configuration preset.
    pub preset: Preset,
    /// Whether to enforce post-quantum signatures.
    pub enforce_pq_signatures: bool,
    /// Underlying node configuration.
    pub(crate) node_config: Option<NodeConfig>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            data_dir: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("indras-network"),
            display_name: None,
            relay_servers: Vec::new(),
            preset: Preset::Default,
            enforce_pq_signatures: false,
            node_config: None,
        }
    }
}

impl NetworkConfig {
    /// Create a new configuration with a data directory.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            ..Default::default()
        }
    }

    /// Convert to the underlying node configuration.
    pub(crate) fn to_node_config(&self) -> NodeConfig {
        let mut config = self
            .node_config
            .clone()
            .unwrap_or_else(|| NodeConfig::with_data_dir(&self.data_dir));

        config = config.with_event_channel_capacity(self.preset.event_channel_capacity());

        if self.enforce_pq_signatures {
            config = config.enforce_pq_signatures();
        }

        config
    }
}

/// Builder for creating an IndrasNetwork instance with custom configuration.
#[derive(Debug, Clone)]
pub struct NetworkBuilder {
    config: NetworkConfig,
}

impl NetworkBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: NetworkConfig::default(),
        }
    }

    /// Create a new builder with a preset configuration.
    pub fn with_preset(preset: Preset) -> Self {
        Self {
            config: NetworkConfig {
                preset,
                ..Default::default()
            },
        }
    }

    /// Set the data directory for persistence.
    pub fn data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.data_dir = dir.into();
        self
    }

    /// Set the display name for this node.
    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.config.display_name = Some(name.into());
        self
    }

    /// Set relay servers for NAT traversal.
    pub fn relay_servers(mut self, servers: Vec<String>) -> Self {
        self.config.relay_servers = servers;
        self
    }

    /// Enforce post-quantum signatures (disable legacy unsigned messages).
    pub fn enforce_pq_signatures(mut self) -> Self {
        self.config.enforce_pq_signatures = true;
        self
    }

    /// Use a custom node configuration.
    ///
    /// This is an escape hatch for advanced users who need full control
    /// over the underlying node configuration.
    pub fn with_node_config(mut self, config: NodeConfig) -> Self {
        self.config.node_config = Some(config);
        self
    }

    /// Build the configuration.
    pub fn build_config(self) -> NetworkConfig {
        self.config
    }
}

impl Default for NetworkBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Helper module for directory resolution
mod dirs {
    use std::path::PathBuf;

    pub fn data_dir() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join("Library/Application Support"))
        }

        #[cfg(target_os = "linux")]
        {
            std::env::var("XDG_DATA_HOME")
                .ok()
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var("HOME")
                        .ok()
                        .map(|home| PathBuf::from(home).join(".local/share"))
                })
        }

        #[cfg(target_os = "windows")]
        {
            std::env::var("APPDATA").ok().map(PathBuf::from)
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_defaults() {
        assert_eq!(Preset::Default.event_channel_capacity(), 1024);
        assert_eq!(Preset::Chat.event_channel_capacity(), 4096);
        assert_eq!(Preset::IoT.sync_interval_secs(), 30);
    }

    #[test]
    fn test_builder() {
        let config = NetworkBuilder::new()
            .data_dir("/tmp/test")
            .display_name("Alice")
            .enforce_pq_signatures()
            .build_config();

        assert_eq!(config.data_dir, PathBuf::from("/tmp/test"));
        assert_eq!(config.display_name, Some("Alice".to_string()));
        assert!(config.enforce_pq_signatures);
    }
}
