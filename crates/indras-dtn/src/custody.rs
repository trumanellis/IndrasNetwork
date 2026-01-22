//! Custody transfer management for DTN bundles
//!
//! Custody transfer is a DTN mechanism where a node explicitly accepts
//! responsibility for delivering a bundle. The custodian commits to
//! either delivering the bundle or finding another node to take custody.

use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{instrument, warn};

use indras_core::PeerIdentity;

use crate::bundle::{Bundle, BundleId, BundleSummary};
use crate::error::CustodyError;

/// Configuration for custody management
#[derive(Debug, Clone)]
pub struct CustodyConfig {
    /// Maximum number of bundles to hold custody of
    pub max_custody_bundles: usize,
    /// Timeout for custody acceptance (how long to wait for a response)
    pub acceptance_timeout: Duration,
    /// Whether to accept custody from unknown/untrusted peers
    pub accept_from_unknown: bool,
}

impl Default for CustodyConfig {
    fn default() -> Self {
        Self {
            max_custody_bundles: 1000,
            acceptance_timeout: Duration::from_secs(30),
            accept_from_unknown: true,
        }
    }
}

/// Record of a bundle we have custody of
#[derive(Debug, Clone)]
pub struct CustodyRecord<I: PeerIdentity> {
    /// The bundle ID
    pub bundle_id: BundleId,
    /// When we accepted custody
    pub accepted_at: Instant,
    /// Who we accepted custody from (None if we're the source)
    pub accepted_from: Option<I>,
    /// The bundle's destination
    pub destination: I,
    /// When this custody record expires
    pub expiration: Instant,
    /// Number of transfer attempts made
    pub transfer_attempts: u32,
}

/// A pending custody transfer offer
#[derive(Debug, Clone)]
pub struct PendingCustodyTransfer<I: PeerIdentity> {
    /// The bundle ID being offered
    pub bundle_id: BundleId,
    /// Who we offered custody to
    pub offered_to: I,
    /// When the offer was made
    pub offered_at: Instant,
    /// Timeout for the offer
    pub timeout: Duration,
}

impl<I: PeerIdentity> PendingCustodyTransfer<I> {
    /// Check if this offer has timed out
    pub fn is_timed_out(&self) -> bool {
        self.offered_at.elapsed() > self.timeout
    }
}

/// Result of handling a custody acceptance response
#[derive(Debug, Clone)]
pub enum CustodyTransferResult<I: PeerIdentity> {
    /// Transfer was accepted
    Accepted {
        bundle_id: BundleId,
        new_custodian: I,
    },
    /// Transfer was refused
    Refused {
        bundle_id: BundleId,
        reason: RefuseReason,
    },
    /// No pending transfer found
    NoPendingTransfer,
}

/// Manages custody state for bundles
///
/// The CustodyManager tracks which bundles we have custody of,
/// manages custody transfer offers, and handles acceptance/refusal responses.
pub struct CustodyManager<I: PeerIdentity> {
    /// Bundles we have custody of
    custody_records: DashMap<BundleId, CustodyRecord<I>>,
    /// Pending custody transfer offers (awaiting response)
    pending_transfers: DashMap<BundleId, PendingCustodyTransfer<I>>,
    /// Configuration
    config: CustodyConfig,
}

impl<I: PeerIdentity> CustodyManager<I> {
    /// Create a new custody manager
    pub fn new(config: CustodyConfig) -> Self {
        Self {
            custody_records: DashMap::new(),
            pending_transfers: DashMap::new(),
            config,
        }
    }

    /// Accept custody of a bundle
    ///
    /// Returns an error if we're at capacity or already have custody.
    #[instrument(skip(self, bundle, from), fields(bundle_id = %bundle.bundle_id, current_count = self.custody_records.len()))]
    pub fn accept_custody(
        &self,
        bundle: &Bundle<I>,
        from: Option<&I>,
    ) -> Result<(), CustodyError> {
        // Check capacity
        if self.custody_records.len() >= self.config.max_custody_bundles {
            return Err(CustodyError::StorageFull {
                max: self.config.max_custody_bundles,
            });
        }

        // Check if we already have custody
        if self.custody_records.contains_key(&bundle.bundle_id) {
            return Err(CustodyError::AlreadyHaveCustody);
        }

        // Calculate expiration based on bundle's remaining lifetime
        let ttl = bundle.time_to_live();
        let expiration = Instant::now() + Duration::from_millis(ttl.num_milliseconds() as u64);

        let record = CustodyRecord {
            bundle_id: bundle.bundle_id,
            accepted_at: Instant::now(),
            accepted_from: from.cloned(),
            destination: bundle.destination().clone(),
            expiration,
            transfer_attempts: 0,
        };

        self.custody_records.insert(bundle.bundle_id, record);
        Ok(())
    }

    /// Offer custody transfer to another node
    ///
    /// Records that we've offered custody and are waiting for a response.
    #[instrument(skip(self, to), fields(bundle_id = %bundle_id))]
    pub fn offer_custody(&self, bundle_id: BundleId, to: I) -> Result<(), CustodyError> {
        // Verify we have custody
        if !self.custody_records.contains_key(&bundle_id) {
            return Err(CustodyError::NotInCustody);
        }

        let pending = PendingCustodyTransfer {
            bundle_id,
            offered_to: to,
            offered_at: Instant::now(),
            timeout: self.config.acceptance_timeout,
        };

        self.pending_transfers.insert(bundle_id, pending);
        Ok(())
    }

    /// Handle a custody acceptance/refusal response
    #[instrument(skip(self), fields(bundle_id = %bundle_id, accepted = accepted))]
    pub fn handle_acceptance(
        &self,
        bundle_id: BundleId,
        accepted: bool,
    ) -> CustodyTransferResult<I> {
        // Get and remove the pending transfer
        let pending = match self.pending_transfers.remove(&bundle_id) {
            Some((_, p)) => p,
            None => return CustodyTransferResult::NoPendingTransfer,
        };

        if accepted {
            // Transfer succeeded - release our custody
            self.custody_records.remove(&bundle_id);
            CustodyTransferResult::Accepted {
                bundle_id,
                new_custodian: pending.offered_to,
            }
        } else {
            // Transfer refused - increment attempt counter
            if let Some(mut record) = self.custody_records.get_mut(&bundle_id) {
                record.transfer_attempts += 1;
            }
            CustodyTransferResult::Refused {
                bundle_id,
                reason: RefuseReason::NotInterested,
            }
        }
    }

    /// Get all bundle IDs we have custody of
    pub fn custodied_bundles(&self) -> Vec<BundleId> {
        self.custody_records
            .iter()
            .map(|r| r.bundle_id)
            .collect()
    }

    /// Check if we have custody of a bundle
    pub fn has_custody(&self, bundle_id: &BundleId) -> bool {
        self.custody_records.contains_key(bundle_id)
    }

    /// Get custody record for a bundle
    pub fn get_custody_record(&self, bundle_id: &BundleId) -> Option<CustodyRecord<I>> {
        self.custody_records.get(bundle_id).map(|r| r.clone())
    }

    /// Release custody of a bundle
    ///
    /// Called when the bundle is delivered, expired, or we give up.
    pub fn release_custody(&self, bundle_id: &BundleId) -> Option<CustodyRecord<I>> {
        self.pending_transfers.remove(bundle_id);
        self.custody_records.remove(bundle_id).map(|(_, r)| r)
    }

    /// Check for and return timed-out custody transfers
    ///
    /// These offers should be retried or the bundles handled differently.
    pub fn check_timeouts(&self) -> Vec<BundleId> {
        let timed_out: Vec<BundleId> = self
            .pending_transfers
            .iter()
            .filter(|p| p.is_timed_out())
            .map(|p| p.bundle_id)
            .collect();

        // Remove timed out transfers and increment attempt counters
        for bundle_id in &timed_out {
            self.pending_transfers.remove(bundle_id);
            if let Some(mut record) = self.custody_records.get_mut(bundle_id) {
                record.transfer_attempts += 1;
            }
        }

        timed_out
    }

    /// Get bundles whose custody has expired
    pub fn get_expired(&self) -> Vec<BundleId> {
        let now = Instant::now();
        self.custody_records
            .iter()
            .filter(|r| now > r.expiration)
            .map(|r| r.bundle_id)
            .collect()
    }

    /// Get the number of bundles we have custody of
    pub fn custody_count(&self) -> usize {
        self.custody_records.len()
    }

    /// Get remaining capacity
    pub fn remaining_capacity(&self) -> usize {
        self.config
            .max_custody_bundles
            .saturating_sub(self.custody_records.len())
    }

    /// Clean up expired custody records
    pub fn cleanup_expired(&self) -> Vec<BundleId> {
        let expired = self.get_expired();
        for bundle_id in &expired {
            self.release_custody(bundle_id);
        }
        expired
    }
}

/// Custody-related protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub enum CustodyMessage<I: PeerIdentity> {
    /// Offering custody transfer
    CustodyOffer {
        bundle_id: BundleId,
        bundle_summary: BundleSummary<I>,
    },
    /// Accepting custody
    CustodyAccept { bundle_id: BundleId },
    /// Refusing custody
    CustodyRefuse {
        bundle_id: BundleId,
        reason: RefuseReason,
    },
    /// Signaling that custody was released
    CustodyRelease {
        bundle_id: BundleId,
        reason: ReleaseReason,
    },
}

/// Reasons for refusing custody
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefuseReason {
    /// Storage is full
    StorageFull,
    /// Not interested in this bundle's destination
    NotInterested,
    /// Already have custody of this bundle
    AlreadyHaveCustody,
    /// Bundle has expired
    BundleExpired,
}

impl std::fmt::Display for RefuseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefuseReason::StorageFull => write!(f, "storage full"),
            RefuseReason::NotInterested => write!(f, "not interested"),
            RefuseReason::AlreadyHaveCustody => write!(f, "already have custody"),
            RefuseReason::BundleExpired => write!(f, "bundle expired"),
        }
    }
}

/// Reasons for releasing custody
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseReason {
    /// Bundle was delivered to destination
    Delivered,
    /// Bundle expired
    Expired,
    /// Custody was transferred to another node
    Transferred,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use indras_core::{EncryptedPayload, Packet, PacketId, SimulationIdentity};

    use crate::bundle::Bundle;

    fn make_test_bundle() -> Bundle<SimulationIdentity> {
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('Z').unwrap();
        let id = PacketId::new(0x1234, 1);

        let packet = Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(b"test".to_vec()),
            vec![],
        );

        Bundle::from_packet(packet, ChronoDuration::hours(1))
    }

    #[test]
    fn test_accept_custody() {
        let manager = CustodyManager::new(CustodyConfig::default());
        let bundle = make_test_bundle();
        let from = SimulationIdentity::new('B').unwrap();

        let result = manager.accept_custody(&bundle, Some(&from));
        assert!(result.is_ok());
        assert!(manager.has_custody(&bundle.bundle_id));
        assert_eq!(manager.custody_count(), 1);
    }

    #[test]
    fn test_capacity_limit() {
        let config = CustodyConfig {
            max_custody_bundles: 1,
            ..Default::default()
        };
        let manager = CustodyManager::new(config);

        let bundle1 = make_test_bundle();
        assert!(manager.accept_custody(&bundle1, None).is_ok());

        // Create a different bundle
        let source = SimulationIdentity::new('B').unwrap();
        let dest = SimulationIdentity::new('Z').unwrap();
        let id = PacketId::new(0x5678, 2);
        let packet = Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(vec![]),
            vec![],
        );
        let bundle2 = Bundle::from_packet(packet, ChronoDuration::hours(1));

        let result = manager.accept_custody(&bundle2, None);
        assert!(matches!(result, Err(CustodyError::StorageFull { .. })));
    }

    #[test]
    fn test_duplicate_custody() {
        let manager = CustodyManager::new(CustodyConfig::default());
        let bundle = make_test_bundle();

        assert!(manager.accept_custody(&bundle, None).is_ok());
        let result = manager.accept_custody(&bundle, None);
        assert!(matches!(result, Err(CustodyError::AlreadyHaveCustody)));
    }

    #[test]
    fn test_custody_transfer_flow() {
        let manager = CustodyManager::new(CustodyConfig::default());
        let bundle = make_test_bundle();
        let bundle_id = bundle.bundle_id;
        let next_hop = SimulationIdentity::new('C').unwrap();

        // Accept custody
        manager.accept_custody(&bundle, None).unwrap();

        // Offer to transfer
        manager.offer_custody(bundle_id, next_hop).unwrap();

        // Simulate acceptance
        let result = manager.handle_acceptance(bundle_id, true);
        assert!(matches!(result, CustodyTransferResult::Accepted { .. }));
        assert!(!manager.has_custody(&bundle_id));
    }

    #[test]
    fn test_custody_release() {
        let manager = CustodyManager::new(CustodyConfig::default());
        let bundle = make_test_bundle();
        let bundle_id = bundle.bundle_id;

        manager.accept_custody(&bundle, None).unwrap();
        assert!(manager.has_custody(&bundle_id));

        let released = manager.release_custody(&bundle_id);
        assert!(released.is_some());
        assert!(!manager.has_custody(&bundle_id));
    }
}
