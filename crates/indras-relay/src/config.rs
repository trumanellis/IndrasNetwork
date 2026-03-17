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

    /// Quota configuration (legacy flat quota — used as fallback)
    #[serde(default)]
    pub quota: QuotaConfig,

    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,

    /// Owner's PlayerId (32 bytes, hex-encoded in TOML).
    /// When set, this relay acts as a personal server for the owner.
    #[serde(default)]
    pub owner_player_id: Option<String>,

    /// Per-tier configuration
    #[serde(default)]
    pub tiers: TierConfig,
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
            owner_player_id: None,
            tiers: TierConfig::default(),
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

/// Per-tier quota and TTL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Self tier: max bytes (default: 1 GB)
    #[serde(default = "default_self_max_bytes")]
    pub self_max_bytes: u64,

    /// Self tier: TTL in days (default: 365)
    #[serde(default = "default_self_ttl_days")]
    pub self_ttl_days: u64,

    /// Self tier: max interfaces (default: 100)
    #[serde(default = "default_self_max_interfaces")]
    pub self_max_interfaces: usize,

    /// Connections tier: max bytes (default: 500 MB)
    #[serde(default = "default_connections_max_bytes")]
    pub connections_max_bytes: u64,

    /// Connections tier: TTL in days (default: 90)
    #[serde(default = "default_connections_ttl_days")]
    pub connections_ttl_days: u64,

    /// Connections tier: max interfaces (default: 200)
    #[serde(default = "default_connections_max_interfaces")]
    pub connections_max_interfaces: usize,

    /// Public tier: max bytes (default: 50 MB)
    #[serde(default = "default_public_max_bytes")]
    pub public_max_bytes: u64,

    /// Public tier: TTL in days (default: 7)
    #[serde(default = "default_public_ttl_days")]
    pub public_ttl_days: u64,

    /// Public tier: max interfaces (default: 50)
    #[serde(default = "default_public_max_interfaces")]
    pub public_max_interfaces: usize,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            self_max_bytes: default_self_max_bytes(),
            self_ttl_days: default_self_ttl_days(),
            self_max_interfaces: default_self_max_interfaces(),
            connections_max_bytes: default_connections_max_bytes(),
            connections_ttl_days: default_connections_ttl_days(),
            connections_max_interfaces: default_connections_max_interfaces(),
            public_max_bytes: default_public_max_bytes(),
            public_ttl_days: default_public_ttl_days(),
            public_max_interfaces: default_public_max_interfaces(),
        }
    }
}

fn default_self_max_bytes() -> u64 {
    1024 * 1024 * 1024 // 1 GB
}

fn default_self_ttl_days() -> u64 {
    365
}

fn default_self_max_interfaces() -> usize {
    100
}

fn default_connections_max_bytes() -> u64 {
    500 * 1024 * 1024 // 500 MB
}

fn default_connections_ttl_days() -> u64 {
    90
}

fn default_connections_max_interfaces() -> usize {
    200
}

fn default_public_max_bytes() -> u64 {
    50 * 1024 * 1024 // 50 MB
}

fn default_public_ttl_days() -> u64 {
    7
}

fn default_public_max_interfaces() -> usize {
    50
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

    #[test]
    fn test_tier_config_defaults() {
        let config = TierConfig::default();
        assert_eq!(config.self_max_bytes, 1024 * 1024 * 1024);
        assert_eq!(config.self_ttl_days, 365);
        assert_eq!(config.self_max_interfaces, 100);
        assert_eq!(config.connections_max_bytes, 500 * 1024 * 1024);
        assert_eq!(config.connections_ttl_days, 90);
        assert_eq!(config.public_max_bytes, 50 * 1024 * 1024);
        assert_eq!(config.public_ttl_days, 7);
    }

    #[test]
    fn test_config_with_tiers_from_toml() {
        let toml_str = r#"
            display_name = "tier-relay"
            owner_player_id = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234"

            [tiers]
            self_max_bytes = 2147483648
            connections_max_bytes = 1073741824
            public_max_bytes = 104857600
        "#;

        let config: RelayConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.display_name, "tier-relay");
        assert!(config.owner_player_id.is_some());
        assert_eq!(config.tiers.self_max_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(config.tiers.connections_max_bytes, 1024 * 1024 * 1024);
        assert_eq!(config.tiers.public_max_bytes, 100 * 1024 * 1024);
        // Defaults for unset fields
        assert_eq!(config.tiers.self_ttl_days, 365);
        assert_eq!(config.tiers.public_ttl_days, 7);
    }
}
