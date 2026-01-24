//! # Indras DTN
//!
//! Delay-tolerant networking for Indras Network.
//!
//! DTN (Delay-Tolerant Networking) provides specialized handling for networks
//! with long delays, intermittent connectivity, and unpredictable topology.
//! This crate extends the base Indras routing with DTN-specific features.
//!
//! ## Features
//!
//! - **Bundle Protocol compatibility**: DTN bundles wrap packets with additional
//!   metadata for lifetime management and custody transfer.
//!
//! - **Custody transfer**: Nodes can explicitly accept responsibility for
//!   delivering bundles, providing stronger delivery guarantees.
//!
//! - **Epidemic routing**: Flood-based routing maximizes delivery probability
//!   in challenged networks, with spray-and-wait for resource efficiency.
//!
//! - **Age-based prioritization**: Older bundles can be deprioritized or expired
//!   to favor fresher data.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use std::time::Duration;
//! use indras_dtn::{Bundle, DtnConfig, EpidemicRouter, CustodyManager};
//! use indras_core::{Packet, PeerIdentity};
//!
//! // Create a bundle from an existing packet
//! let bundle = Bundle::from_packet(packet, chrono::Duration::hours(1));
//!
//! // Configure DTN subsystem
//! let config = DtnConfig::default();
//!
//! // Create routing and custody components
//! let router = EpidemicRouter::new(config.epidemic.clone());
//! let custody = CustodyManager::new(config.custody.clone());
//! ```
//!
//! ## Architecture
//!
//! The DTN crate is organized into several modules:
//!
//! - [`bundle`]: The `Bundle` type that wraps packets with DTN metadata
//! - [`custody`]: Custody transfer management
//! - [`epidemic`]: Epidemic and spray-and-wait routing
//! - [`expiration`]: Age-based expiration and priority demotion
//! - [`strategy`]: Strategy selection based on network conditions
//! - [`prophet`]: PRoPHET probabilistic routing using encounter history
//! - [`error`]: DTN-specific error types

pub mod bundle;
pub mod custody;
pub mod epidemic;
pub mod error;
pub mod expiration;
pub mod prophet;
pub mod strategy;

// Re-export main types
pub use bundle::{Bundle, BundleId, BundleSummary, ClassOfService, CustodyTransfer};
pub use custody::{
    CustodyConfig, CustodyManager, CustodyMessage, CustodyRecord, CustodyTransferResult,
    PendingCustodyTransfer, RefuseReason, ReleaseReason,
};
pub use epidemic::{EpidemicConfig, EpidemicDecision, EpidemicRouter, SuppressReason};
pub use error::{BundleError, CustodyError, DtnError, DtnResult};
pub use expiration::{AgeManager, ExpirationConfig, ExpirationRecord};
pub use prophet::{ProphetConfig, ProphetState, ProphetSummary};
pub use strategy::{DtnStrategy, StrategyCondition, StrategyRule, StrategySelector};

use std::time::Duration;

use indras_core::Priority;

/// Configuration for the DTN subsystem
///
/// Combines configuration for all DTN components.
#[derive(Debug, Clone)]
pub struct DtnConfig {
    /// Custody transfer configuration
    pub custody: CustodyConfig,
    /// Epidemic routing configuration
    pub epidemic: EpidemicConfig,
    /// Expiration management configuration
    pub expiration: ExpirationConfig,
    /// Default routing strategy
    pub default_strategy: DtnStrategy,
}

impl Default for DtnConfig {
    fn default() -> Self {
        Self {
            custody: CustodyConfig {
                max_custody_bundles: 1000,
                acceptance_timeout: Duration::from_secs(30),
                accept_from_unknown: true,
            },
            epidemic: EpidemicConfig {
                max_copies: 8,
                spray_and_wait: true,
                spray_count: 4,
                seen_timeout: Duration::from_secs(3600),
                max_bundle_age: Duration::from_secs(86400),
            },
            expiration: ExpirationConfig {
                default_lifetime: Duration::from_secs(3600),
                max_lifetime: Duration::from_secs(86400 * 7),
                demotion_thresholds: vec![
                    (Duration::from_secs(300), Priority::Normal),
                    (Duration::from_secs(900), Priority::Low),
                ],
                cleanup_interval: Duration::from_secs(60),
            },
            default_strategy: DtnStrategy::SprayAndWait { copies: 4 },
        }
    }
}

impl DtnConfig {
    /// Create a config optimized for low-latency networks
    ///
    /// Uses shorter timeouts and lifetimes, standard store-and-forward.
    pub fn low_latency() -> Self {
        Self {
            custody: CustodyConfig {
                max_custody_bundles: 500,
                acceptance_timeout: Duration::from_secs(10),
                accept_from_unknown: true,
            },
            epidemic: EpidemicConfig {
                max_copies: 4,
                spray_and_wait: false, // Use store-and-forward primarily
                spray_count: 2,
                seen_timeout: Duration::from_secs(600),
                max_bundle_age: Duration::from_secs(3600),
            },
            expiration: ExpirationConfig {
                default_lifetime: Duration::from_secs(600),
                max_lifetime: Duration::from_secs(3600),
                demotion_thresholds: vec![
                    (Duration::from_secs(60), Priority::Normal),
                    (Duration::from_secs(180), Priority::Low),
                ],
                cleanup_interval: Duration::from_secs(30),
            },
            default_strategy: DtnStrategy::StoreAndForward,
        }
    }

    /// Create a config optimized for challenged/sparse networks
    ///
    /// Uses longer timeouts, aggressive epidemic routing.
    pub fn challenged_network() -> Self {
        Self {
            custody: CustodyConfig {
                max_custody_bundles: 2000,
                acceptance_timeout: Duration::from_secs(120),
                accept_from_unknown: true,
            },
            epidemic: EpidemicConfig {
                max_copies: 16,
                spray_and_wait: true,
                spray_count: 8,
                seen_timeout: Duration::from_secs(7200),
                max_bundle_age: Duration::from_secs(86400 * 3),
            },
            expiration: ExpirationConfig {
                default_lifetime: Duration::from_secs(86400),
                max_lifetime: Duration::from_secs(86400 * 14),
                demotion_thresholds: vec![
                    (Duration::from_secs(3600), Priority::Normal),
                    (Duration::from_secs(7200), Priority::Low),
                ],
                cleanup_interval: Duration::from_secs(300),
            },
            default_strategy: DtnStrategy::Epidemic,
        }
    }

    /// Create a config optimized for resource-constrained nodes
    ///
    /// Uses smaller limits and shorter retention.
    pub fn resource_constrained() -> Self {
        Self {
            custody: CustodyConfig {
                max_custody_bundles: 100,
                acceptance_timeout: Duration::from_secs(15),
                accept_from_unknown: false, // More selective
            },
            epidemic: EpidemicConfig {
                max_copies: 2,
                spray_and_wait: true,
                spray_count: 2,
                seen_timeout: Duration::from_secs(300),
                max_bundle_age: Duration::from_secs(1800),
            },
            expiration: ExpirationConfig {
                default_lifetime: Duration::from_secs(300),
                max_lifetime: Duration::from_secs(1800),
                demotion_thresholds: vec![
                    (Duration::from_secs(60), Priority::Normal),
                    (Duration::from_secs(120), Priority::Low),
                ],
                cleanup_interval: Duration::from_secs(30),
            },
            default_strategy: DtnStrategy::SprayAndWait { copies: 2 },
        }
    }

    /// Validate configuration invariants
    ///
    /// Returns a list of warnings/errors if the configuration has potential issues.
    /// An empty list means the configuration is valid.
    pub fn validate(&self) -> Vec<ConfigWarning> {
        let mut warnings = Vec::new();

        // Check expiration config
        if self.expiration.default_lifetime > self.expiration.max_lifetime {
            warnings.push(ConfigWarning::DefaultLifetimeExceedsMax);
        }

        // Check spray config consistency
        if self.epidemic.spray_and_wait && self.epidemic.spray_count > self.epidemic.max_copies {
            warnings.push(ConfigWarning::SprayCountExceedsMaxCopies);
        }

        // Check strategy matches config
        if let DtnStrategy::SprayAndWait { copies } = self.default_strategy
            && copies > self.epidemic.max_copies
        {
            warnings.push(ConfigWarning::StrategyExceedsMaxCopies);
        }

        // Warn about very short cleanup intervals
        if self.expiration.cleanup_interval < Duration::from_secs(10) {
            warnings.push(ConfigWarning::CleanupIntervalTooShort);
        }

        // Warn about very large custody limits
        if self.custody.max_custody_bundles > 10000 {
            warnings.push(ConfigWarning::LargeCustodyLimit);
        }

        warnings
    }

    /// Check if the configuration is valid (no errors)
    pub fn is_valid(&self) -> bool {
        self.validate().is_empty()
    }
}

/// Configuration warnings and errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWarning {
    /// Default lifetime exceeds maximum lifetime
    DefaultLifetimeExceedsMax,
    /// Spray count exceeds max copies
    SprayCountExceedsMaxCopies,
    /// Default strategy copies exceeds max copies
    StrategyExceedsMaxCopies,
    /// Cleanup interval is very short (< 10s)
    CleanupIntervalTooShort,
    /// Custody limit is very large (> 10000)
    LargeCustodyLimit,
}

impl std::fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigWarning::DefaultLifetimeExceedsMax => {
                write!(f, "default_lifetime exceeds max_lifetime")
            }
            ConfigWarning::SprayCountExceedsMaxCopies => {
                write!(f, "spray_count exceeds max_copies")
            }
            ConfigWarning::StrategyExceedsMaxCopies => {
                write!(f, "default strategy copies exceeds max_copies")
            }
            ConfigWarning::CleanupIntervalTooShort => {
                write!(f, "cleanup_interval is very short (< 10s)")
            }
            ConfigWarning::LargeCustodyLimit => {
                write!(f, "max_custody_bundles is very large (> 10000)")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DtnConfig::default();
        assert_eq!(config.custody.max_custody_bundles, 1000);
        assert!(config.epidemic.spray_and_wait);
        assert_eq!(
            config.default_strategy,
            DtnStrategy::SprayAndWait { copies: 4 }
        );
    }

    #[test]
    fn test_low_latency_config() {
        let config = DtnConfig::low_latency();
        assert_eq!(config.default_strategy, DtnStrategy::StoreAndForward);
        assert!(!config.epidemic.spray_and_wait);
    }

    #[test]
    fn test_challenged_network_config() {
        let config = DtnConfig::challenged_network();
        assert_eq!(config.default_strategy, DtnStrategy::Epidemic);
        assert_eq!(config.epidemic.max_copies, 16);
    }

    #[test]
    fn test_resource_constrained_config() {
        let config = DtnConfig::resource_constrained();
        assert_eq!(config.custody.max_custody_bundles, 100);
        assert!(!config.custody.accept_from_unknown);
    }

    #[test]
    fn test_default_config_is_valid() {
        let config = DtnConfig::default();
        assert!(config.is_valid());
    }

    #[test]
    fn test_preset_configs_are_valid() {
        assert!(DtnConfig::low_latency().is_valid());
        assert!(DtnConfig::challenged_network().is_valid());
        assert!(DtnConfig::resource_constrained().is_valid());
    }

    #[test]
    fn test_invalid_config_detected() {
        let mut config = DtnConfig::default();
        // Make default_lifetime exceed max_lifetime
        config.expiration.default_lifetime = Duration::from_secs(86400 * 30);
        config.expiration.max_lifetime = Duration::from_secs(3600);

        let warnings = config.validate();
        assert!(warnings.contains(&ConfigWarning::DefaultLifetimeExceedsMax));
    }
}
