//! Relay configuration
//!
//! TOML-based configuration for the relay server.

use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level relay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    /// Directory for persistent data (database, registration state)
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Display name for this relay
    #[serde(default = "default_display_name")]
    pub display_name: String,

    /// Admin HTTP API bind address
    #[serde(default = "default_admin_bind")]
    pub admin_bind: SocketAddr,

    /// Bearer token for admin API authentication
    #[serde(default = "default_admin_token")]
    pub admin_token: String,

    /// Quota configuration
    #[serde(default)]
    pub quota: QuotaConfig,

    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            display_name: default_display_name(),
            admin_bind: default_admin_bind(),
            admin_token: default_admin_token(),
            quota: QuotaConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

impl RelayConfig {
    /// Load configuration from a TOML file
    pub fn from_file(path: &std::path::Path) -> Result<Self, crate::error::RelayError> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            crate::error::RelayError::Config(format!("Failed to read config file: {e}"))
        })?;
        toml::from_str(&contents).map_err(|e| {
            crate::error::RelayError::Config(format!("Failed to parse config: {e}"))
        })
    }
}

/// Quota configuration for per-peer limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Maximum bytes of stored events per peer (default: 100 MB)
    #[serde(default = "default_max_bytes_per_peer")]
    pub default_max_bytes_per_peer: u64,

    /// Maximum interfaces a single peer can register (default: 50)
    #[serde(default = "default_max_interfaces_per_peer")]
    pub default_max_interfaces_per_peer: usize,

    /// Global maximum bytes across all peers (default: 10 GB)
    #[serde(default = "default_global_max_bytes")]
    pub global_max_bytes: u64,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            default_max_bytes_per_peer: default_max_bytes_per_peer(),
            default_max_interfaces_per_peer: default_max_interfaces_per_peer(),
            global_max_bytes: default_global_max_bytes(),
        }
    }
}

/// Storage configuration for event retention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Default TTL for stored events in days (default: 90)
    #[serde(default = "default_event_ttl_days")]
    pub default_event_ttl_days: u64,

    /// Maximum TTL for stored events in days (default: 365)
    #[serde(default = "default_max_event_ttl_days")]
    pub max_event_ttl_days: u64,

    /// How often to run cleanup in seconds (default: 3600)
    #[serde(default = "default_cleanup_interval_secs")]
    pub cleanup_interval_secs: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            default_event_ttl_days: default_event_ttl_days(),
            max_event_ttl_days: default_max_event_ttl_days(),
            cleanup_interval_secs: default_cleanup_interval_secs(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./relay-data")
}

fn default_display_name() -> String {
    "indras-relay".to_string()
}

fn default_admin_bind() -> SocketAddr {
    "127.0.0.1:9090".parse().unwrap()
}

fn default_admin_token() -> String {
    "change-me".to_string()
}

fn default_max_bytes_per_peer() -> u64 {
    100 * 1024 * 1024 // 100 MB
}

fn default_max_interfaces_per_peer() -> usize {
    50
}

fn default_global_max_bytes() -> u64 {
    10 * 1024 * 1024 * 1024 // 10 GB
}

fn default_event_ttl_days() -> u64 {
    90
}

fn default_max_event_ttl_days() -> u64 {
    365
}

fn default_cleanup_interval_secs() -> u64 {
    3600
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RelayConfig::default();
        assert_eq!(config.display_name, "indras-relay");
        assert_eq!(config.quota.default_max_interfaces_per_peer, 50);
        assert_eq!(config.storage.default_event_ttl_days, 90);
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
            data_dir = "/tmp/relay"
            display_name = "my-relay"
            admin_bind = "0.0.0.0:8080"
            admin_token = "secret123"

            [quota]
            default_max_bytes_per_peer = 52428800
            default_max_interfaces_per_peer = 25

            [storage]
            default_event_ttl_days = 30
        "#;

        let config: RelayConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.display_name, "my-relay");
        assert_eq!(config.quota.default_max_bytes_per_peer, 50 * 1024 * 1024);
        assert_eq!(config.quota.default_max_interfaces_per_peer, 25);
        assert_eq!(config.storage.default_event_ttl_days, 30);
    }
}
