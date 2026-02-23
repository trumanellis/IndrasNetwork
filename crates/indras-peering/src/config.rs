//! Peering configuration.

use std::path::PathBuf;
use std::time::Duration;

/// Configuration for [`PeeringRuntime`](crate::PeeringRuntime).
#[derive(Debug, Clone)]
pub struct PeeringConfig {
    /// Directory for identity keys and world-view persistence.
    pub data_dir: PathBuf,
    /// How often to poll contacts for peer changes (default 2s).
    pub poll_interval: Duration,
    /// How often to save the world view snapshot (default 30s).
    pub save_interval: Duration,
}

impl PeeringConfig {
    /// Create a config with the given data directory and sensible defaults.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            poll_interval: Duration::from_secs(2),
            save_interval: Duration::from_secs(30),
        }
    }
}

impl Default for PeeringConfig {
    fn default() -> Self {
        Self::new(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = PeeringConfig::new("/tmp/test");
        assert_eq!(cfg.data_dir, PathBuf::from("/tmp/test"));
        assert_eq!(cfg.poll_interval, Duration::from_secs(2));
        assert_eq!(cfg.save_interval, Duration::from_secs(30));
    }

    #[test]
    fn config_default_trait() {
        let cfg = PeeringConfig::default();
        assert_eq!(cfg.data_dir, PathBuf::from("."));
    }

    #[test]
    fn config_custom_intervals() {
        let mut cfg = PeeringConfig::new("/data");
        cfg.poll_interval = Duration::from_millis(500);
        cfg.save_interval = Duration::from_secs(60);
        assert_eq!(cfg.poll_interval, Duration::from_millis(500));
        assert_eq!(cfg.save_interval, Duration::from_secs(60));
    }
}
