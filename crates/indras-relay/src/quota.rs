//! Per-peer storage quota management
//!
//! Enforces storage limits per peer and globally to prevent
//! any single peer from consuming excessive relay resources.

use std::sync::RwLock;

use dashmap::DashMap;

use indras_transport::identity::IrohIdentity;
use indras_transport::protocol::StorageTier;

use crate::config::{QuotaConfig, TierConfig};
use crate::error::{RelayError, RelayResult};
use crate::tier;

/// Per-peer quota tracking
#[derive(Debug, Clone)]
pub struct PeerQuota {
    /// Current bytes stored for this peer
    pub bytes_used: u64,
    /// Current number of registered interfaces
    pub interface_count: usize,
}

/// Manages per-peer storage quotas
pub struct QuotaManager {
    config: RwLock<QuotaConfig>,
    /// Per-peer quota tracking
    peer_quotas: DashMap<IrohIdentity, PeerQuota>,
}

impl QuotaManager {
    /// Create a new quota manager with the given configuration
    pub fn new(config: QuotaConfig) -> Self {
        Self {
            config: RwLock::new(config),
            peer_quotas: DashMap::new(),
        }
    }

    /// Atomically swap in a new quota configuration.
    ///
    /// Subsequent quota checks see the new values. Existing usage is retained.
    pub fn update_config(&self, config: QuotaConfig) {
        *self.config.write().expect("QuotaManager config lock poisoned") = config;
    }

    /// Snapshot the current quota configuration.
    pub fn config_snapshot(&self) -> QuotaConfig {
        self.config
            .read()
            .expect("QuotaManager config lock poisoned")
            .clone()
    }

    /// Check if a peer can register additional interfaces
    pub fn can_register(
        &self,
        peer_id: &IrohIdentity,
        additional_interfaces: usize,
    ) -> RelayResult<()> {
        let current = self
            .peer_quotas
            .get(peer_id)
            .map(|q| q.interface_count)
            .unwrap_or(0);

        let max = self
            .config
            .read()
            .expect("QuotaManager config lock poisoned")
            .default_max_interfaces_per_peer;

        if current + additional_interfaces > max {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed interface limit: {} + {} > {}",
                    current, additional_interfaces, max
                ),
            });
        }

        Ok(())
    }

    /// Check if storing additional bytes would exceed quota
    pub fn can_store(
        &self,
        peer_id: &IrohIdentity,
        additional_bytes: u64,
        total_usage: u64,
    ) -> RelayResult<()> {
        // Check per-peer limit
        let current = self
            .peer_quotas
            .get(peer_id)
            .map(|q| q.bytes_used)
            .unwrap_or(0);

        let (per_peer, global) = {
            let c = self
                .config
                .read()
                .expect("QuotaManager config lock poisoned");
            (c.default_max_bytes_per_peer, c.global_max_bytes)
        };

        if current + additional_bytes > per_peer {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed per-peer byte limit: {} + {} > {}",
                    current, additional_bytes, per_peer
                ),
            });
        }

        // Check global limit
        if total_usage + additional_bytes > global {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed global byte limit: {} + {} > {}",
                    total_usage, additional_bytes, global
                ),
            });
        }

        Ok(())
    }

    /// Record that a peer registered interfaces
    pub fn record_registration(&self, peer_id: IrohIdentity, interface_count: usize) {
        self.peer_quotas
            .entry(peer_id)
            .and_modify(|q| q.interface_count += interface_count)
            .or_insert(PeerQuota {
                bytes_used: 0,
                interface_count,
            });
    }

    /// Record that a peer unregistered interfaces
    pub fn record_unregistration(&self, peer_id: &IrohIdentity, interface_count: usize) {
        if let Some(mut quota) = self.peer_quotas.get_mut(peer_id) {
            quota.interface_count = quota.interface_count.saturating_sub(interface_count);
        }
    }

    /// Update bytes used for a peer
    pub fn record_storage(&self, peer_id: IrohIdentity, bytes: u64) {
        self.peer_quotas
            .entry(peer_id)
            .and_modify(|q| q.bytes_used += bytes)
            .or_insert(PeerQuota {
                bytes_used: bytes,
                interface_count: 0,
            });
    }

    /// Get quota info for a peer
    pub fn peer_quota(&self, peer_id: &IrohIdentity) -> Option<PeerQuota> {
        self.peer_quotas.get(peer_id).map(|q| q.clone())
    }

    /// Get a clone of the quota configuration
    pub fn config(&self) -> QuotaConfig {
        self.config_snapshot()
    }

    /// Get the number of tracked peers
    pub fn peer_count(&self) -> usize {
        self.peer_quotas.len()
    }
}

/// Per-tier quota tracking for a single peer
#[derive(Debug, Clone, Default)]
pub struct TieredPeerQuota {
    /// Bytes used per tier
    pub tier_bytes: std::collections::HashMap<StorageTier, u64>,
    /// Interface count per tier
    pub tier_interfaces: std::collections::HashMap<StorageTier, usize>,
}

/// Manages per-tier storage quotas
pub struct TieredQuotaManager {
    tier_config: RwLock<TierConfig>,
    /// Per-peer, per-tier quota tracking
    peer_quotas: DashMap<IrohIdentity, TieredPeerQuota>,
}

impl TieredQuotaManager {
    /// Create a new tiered quota manager
    pub fn new(tier_config: TierConfig) -> Self {
        Self {
            tier_config: RwLock::new(tier_config),
            peer_quotas: DashMap::new(),
        }
    }

    /// Atomically swap in a new tier configuration.
    pub fn update_config(&self, tier_config: TierConfig) {
        *self
            .tier_config
            .write()
            .expect("TieredQuotaManager config lock poisoned") = tier_config;
    }

    /// Snapshot the current tier configuration.
    pub fn tier_config_snapshot(&self) -> TierConfig {
        self.tier_config
            .read()
            .expect("TieredQuotaManager config lock poisoned")
            .clone()
    }

    /// Check if storing additional bytes in a tier would exceed quota
    pub fn can_store_tiered(
        &self,
        peer_id: &IrohIdentity,
        tier: StorageTier,
        additional_bytes: u64,
    ) -> RelayResult<()> {
        let max_bytes = {
            let c = self
                .tier_config
                .read()
                .expect("TieredQuotaManager config lock poisoned");
            tier::tier_max_bytes(tier, &c)
        };
        let current = self
            .peer_quotas
            .get(peer_id)
            .and_then(|q| q.tier_bytes.get(&tier).copied())
            .unwrap_or(0);

        if current + additional_bytes > max_bytes {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed {:?} tier byte limit: {} + {} > {}",
                    tier, current, additional_bytes, max_bytes
                ),
            });
        }

        Ok(())
    }

    /// Check if a peer can register interfaces in a tier
    pub fn can_register_tiered(
        &self,
        peer_id: &IrohIdentity,
        tier: StorageTier,
        additional_interfaces: usize,
    ) -> RelayResult<()> {
        let max_interfaces = {
            let c = self
                .tier_config
                .read()
                .expect("TieredQuotaManager config lock poisoned");
            tier::tier_max_interfaces(tier, &c)
        };
        let current = self
            .peer_quotas
            .get(peer_id)
            .and_then(|q| q.tier_interfaces.get(&tier).copied())
            .unwrap_or(0);

        if current + additional_interfaces > max_interfaces {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed {:?} tier interface limit: {} + {} > {}",
                    tier, current, additional_interfaces, max_interfaces
                ),
            });
        }

        Ok(())
    }

    /// Record bytes stored in a tier
    pub fn record_storage_tiered(&self, peer_id: IrohIdentity, tier: StorageTier, bytes: u64) {
        self.peer_quotas
            .entry(peer_id)
            .or_default()
            .tier_bytes
            .entry(tier)
            .and_modify(|b| *b += bytes)
            .or_insert(bytes);
    }

    /// Record interfaces registered in a tier
    pub fn record_registration_tiered(
        &self,
        peer_id: IrohIdentity,
        tier: StorageTier,
        interface_count: usize,
    ) {
        self.peer_quotas
            .entry(peer_id)
            .or_default()
            .tier_interfaces
            .entry(tier)
            .and_modify(|c| *c += interface_count)
            .or_insert(interface_count);
    }

    /// Record interfaces unregistered from a tier
    pub fn record_unregistration_tiered(
        &self,
        peer_id: &IrohIdentity,
        tier: StorageTier,
        interface_count: usize,
    ) {
        if let Some(mut quota) = self.peer_quotas.get_mut(peer_id) {
            if let Some(count) = quota.tier_interfaces.get_mut(&tier) {
                *count = count.saturating_sub(interface_count);
            }
        }
    }

    /// Get a clone of the tier config
    pub fn tier_config(&self) -> TierConfig {
        self.tier_config_snapshot()
    }

    /// Get tier-specific usage for a peer
    pub fn peer_tier_bytes(&self, peer_id: &IrohIdentity, tier: StorageTier) -> u64 {
        self.peer_quotas
            .get(peer_id)
            .and_then(|q| q.tier_bytes.get(&tier).copied())
            .unwrap_or(0)
    }

    /// Populate quota state from persisted usage data
    ///
    /// Called during relay startup to restore in-memory quota tracking from the
    /// BlobStore's durable usage tables. Each entry maps a peer to a tier and
    /// the total bytes that peer has stored in that tier. Bytes are accumulated
    /// so multiple entries for the same `(peer_id, tier)` pair are summed.
    ///
    /// The caller is responsible for joining BlobStore's per-interface usage
    /// (from `BlobStore::all_interface_usage`) with the peer→interface mapping
    /// (from `RegistrationState`) before calling this method.
    pub fn reconstruct_from_usage(&self, usage_data: Vec<(IrohIdentity, StorageTier, u64)>) {
        for (peer_id, tier, bytes) in usage_data {
            self.peer_quotas
                .entry(peer_id)
                .or_default()
                .tier_bytes
                .entry(tier)
                .and_modify(|b| *b += bytes)
                .or_insert(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::SecretKey;

    fn test_peer() -> IrohIdentity {
        let secret = SecretKey::generate(&mut rand::rng());
        IrohIdentity::new(secret.public())
    }

    fn test_config() -> QuotaConfig {
        QuotaConfig {
            default_max_bytes_per_peer: 1024, // 1 KB for testing
            default_max_interfaces_per_peer: 3,
            global_max_bytes: 4096,
        }
    }

    #[test]
    fn test_can_register() {
        let mgr = QuotaManager::new(test_config());
        let peer = test_peer();

        assert!(mgr.can_register(&peer, 2).is_ok());
        assert!(mgr.can_register(&peer, 3).is_ok());
        assert!(mgr.can_register(&peer, 4).is_err());
    }

    #[test]
    fn test_can_register_after_recording() {
        let mgr = QuotaManager::new(test_config());
        let peer = test_peer();

        mgr.record_registration(peer, 2);
        assert!(mgr.can_register(&peer, 1).is_ok());
        assert!(mgr.can_register(&peer, 2).is_err());
    }

    #[test]
    fn test_can_store() {
        let mgr = QuotaManager::new(test_config());
        let peer = test_peer();

        assert!(mgr.can_store(&peer, 500, 0).is_ok());
        assert!(mgr.can_store(&peer, 1025, 0).is_err()); // Exceeds per-peer
        assert!(mgr.can_store(&peer, 500, 4000).is_err()); // Exceeds global
    }

    #[test]
    fn test_record_storage() {
        let mgr = QuotaManager::new(test_config());
        let peer = test_peer();

        mgr.record_storage(peer, 500);
        assert!(mgr.can_store(&peer, 500, 500).is_ok());
        assert!(mgr.can_store(&peer, 525, 500).is_err()); // 500 + 525 > 1024
    }

    #[test]
    fn test_unregistration() {
        let mgr = QuotaManager::new(test_config());
        let peer = test_peer();

        mgr.record_registration(peer, 3);
        assert!(mgr.can_register(&peer, 1).is_err());

        mgr.record_unregistration(&peer, 2);
        assert!(mgr.can_register(&peer, 1).is_ok());
        assert!(mgr.can_register(&peer, 3).is_err());
    }

    #[test]
    fn test_tiered_can_store() {
        let tier_config = TierConfig {
            self_max_bytes: 1024,
            connections_max_bytes: 512,
            public_max_bytes: 256,
            ..Default::default()
        };
        let mgr = TieredQuotaManager::new(tier_config);
        let peer = test_peer();

        // Within limits
        assert!(mgr.can_store_tiered(&peer, StorageTier::Self_, 500).is_ok());
        assert!(mgr.can_store_tiered(&peer, StorageTier::Connections, 500).is_ok());
        assert!(mgr.can_store_tiered(&peer, StorageTier::Public, 200).is_ok());

        // Exceeds tier limits
        assert!(mgr.can_store_tiered(&peer, StorageTier::Self_, 1025).is_err());
        assert!(mgr.can_store_tiered(&peer, StorageTier::Connections, 513).is_err());
        assert!(mgr.can_store_tiered(&peer, StorageTier::Public, 257).is_err());
    }

    #[test]
    fn test_tiered_record_storage() {
        let tier_config = TierConfig {
            self_max_bytes: 1024,
            ..Default::default()
        };
        let mgr = TieredQuotaManager::new(tier_config);
        let peer = test_peer();

        mgr.record_storage_tiered(peer, StorageTier::Self_, 500);
        assert!(mgr.can_store_tiered(&peer, StorageTier::Self_, 500).is_ok());
        assert!(mgr.can_store_tiered(&peer, StorageTier::Self_, 525).is_err());
        assert_eq!(mgr.peer_tier_bytes(&peer, StorageTier::Self_), 500);
    }

    #[test]
    fn test_tiered_can_register() {
        let tier_config = TierConfig {
            self_max_interfaces: 3,
            connections_max_interfaces: 5,
            public_max_interfaces: 2,
            ..Default::default()
        };
        let mgr = TieredQuotaManager::new(tier_config);
        let peer = test_peer();

        assert!(mgr.can_register_tiered(&peer, StorageTier::Self_, 3).is_ok());
        assert!(mgr.can_register_tiered(&peer, StorageTier::Self_, 4).is_err());
        assert!(mgr.can_register_tiered(&peer, StorageTier::Public, 2).is_ok());
        assert!(mgr.can_register_tiered(&peer, StorageTier::Public, 3).is_err());
    }
}
