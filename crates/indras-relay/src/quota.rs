//! Per-peer storage quota management
//!
//! Enforces storage limits per peer and globally to prevent
//! any single peer from consuming excessive relay resources.

use dashmap::DashMap;

use indras_transport::identity::IrohIdentity;

use crate::config::QuotaConfig;
use crate::error::{RelayError, RelayResult};

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
    config: QuotaConfig,
    /// Per-peer quota tracking
    peer_quotas: DashMap<IrohIdentity, PeerQuota>,
}

impl QuotaManager {
    /// Create a new quota manager with the given configuration
    pub fn new(config: QuotaConfig) -> Self {
        Self {
            config,
            peer_quotas: DashMap::new(),
        }
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

        if current + additional_interfaces > self.config.default_max_interfaces_per_peer {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed interface limit: {} + {} > {}",
                    current,
                    additional_interfaces,
                    self.config.default_max_interfaces_per_peer
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

        if current + additional_bytes > self.config.default_max_bytes_per_peer {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed per-peer byte limit: {} + {} > {}",
                    current, additional_bytes, self.config.default_max_bytes_per_peer
                ),
            });
        }

        // Check global limit
        if total_usage + additional_bytes > self.config.global_max_bytes {
            return Err(RelayError::QuotaExceeded {
                reason: format!(
                    "Would exceed global byte limit: {} + {} > {}",
                    total_usage, additional_bytes, self.config.global_max_bytes
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

    /// Get the quota configuration
    pub fn config(&self) -> &QuotaConfig {
        &self.config
    }

    /// Get the number of tracked peers
    pub fn peer_count(&self) -> usize {
        self.peer_quotas.len()
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
}
