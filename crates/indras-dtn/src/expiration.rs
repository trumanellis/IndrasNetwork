//! Age-based expiration and priority management for DTN bundles
//!
//! This module manages bundle expiration based on age (time since creation)
//! rather than hop count (TTL). It also handles age-based priority demotion
//! where older bundles are deprioritized to favor fresher data.

use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;

use indras_core::{PeerIdentity, Priority};

use crate::bundle::{Bundle, BundleId};

/// Configuration for expiration management
#[derive(Debug, Clone)]
pub struct ExpirationConfig {
    /// Default bundle lifetime if not specified
    pub default_lifetime: Duration,
    /// Maximum allowed lifetime (caps bundle-specified lifetimes)
    pub max_lifetime: Duration,
    /// Age thresholds for priority demotion
    ///
    /// Bundles older than the threshold get demoted to the specified priority.
    /// Should be sorted by age in ascending order.
    pub demotion_thresholds: Vec<(Duration, Priority)>,
    /// How often to check for expired bundles
    pub cleanup_interval: Duration,
}

impl Default for ExpirationConfig {
    fn default() -> Self {
        Self {
            default_lifetime: Duration::from_secs(3600),         // 1 hour
            max_lifetime: Duration::from_secs(86400 * 7),        // 7 days
            demotion_thresholds: vec![
                (Duration::from_secs(300), Priority::Normal),    // After 5 min: Normal
                (Duration::from_secs(900), Priority::Low),       // After 15 min: Low
            ],
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

/// Record of a tracked bundle for expiration
#[derive(Debug, Clone)]
pub struct ExpirationRecord {
    /// Bundle identifier
    pub bundle_id: BundleId,
    /// When the bundle was created
    pub created_at: DateTime<Utc>,
    /// When the bundle expires
    pub expires_at: DateTime<Utc>,
    /// Original priority at creation
    pub priority_at_creation: Priority,
}

/// Manages bundle expiration and age-based prioritization
pub struct AgeManager<I: PeerIdentity> {
    /// Tracked bundles with their expiration info
    tracked: DashMap<BundleId, ExpirationRecord>,
    /// Configuration
    config: ExpirationConfig,
    /// Type marker
    _marker: std::marker::PhantomData<I>,
}

impl<I: PeerIdentity> AgeManager<I> {
    /// Create a new age manager
    pub fn new(config: ExpirationConfig) -> Self {
        Self {
            tracked: DashMap::new(),
            config,
            _marker: std::marker::PhantomData,
        }
    }

    /// Track a bundle for expiration
    pub fn track(&self, bundle: &Bundle<I>) {
        let created_at = bundle.packet.created_at;
        let lifetime = bundle.lifetime;

        // Cap lifetime at max
        let capped_lifetime = std::cmp::min(
            Duration::from_millis(lifetime.num_milliseconds() as u64),
            self.config.max_lifetime,
        );

        let expires_at = created_at + chrono::Duration::from_std(capped_lifetime).unwrap_or(chrono::Duration::hours(1));

        let record = ExpirationRecord {
            bundle_id: bundle.bundle_id,
            created_at,
            expires_at,
            priority_at_creation: bundle.packet.priority,
        };

        self.tracked.insert(bundle.bundle_id, record);
    }

    /// Stop tracking a bundle
    pub fn untrack(&self, bundle_id: &BundleId) {
        self.tracked.remove(bundle_id);
    }

    /// Check if a bundle has expired
    pub fn is_expired(&self, bundle_id: &BundleId) -> bool {
        self.tracked
            .get(bundle_id)
            .map(|r| Utc::now() > r.expires_at)
            .unwrap_or(false)
    }

    /// Get effective priority for a bundle (may be demoted based on age)
    ///
    /// Returns the original priority if no demotion rules apply.
    pub fn effective_priority(&self, bundle: &Bundle<I>) -> Priority {
        let age = bundle.age();
        let age_duration = Duration::from_millis(age.num_milliseconds().max(0) as u64);
        let original_priority = bundle.packet.priority;

        // Find the highest demotion that applies
        let mut effective = original_priority;
        for (threshold, demoted_priority) in &self.config.demotion_thresholds {
            if age_duration >= *threshold && *demoted_priority < effective {
                effective = *demoted_priority;
            }
        }

        effective
    }

    /// Get all expired bundle IDs
    pub fn get_expired(&self) -> Vec<BundleId> {
        let now = Utc::now();
        self.tracked
            .iter()
            .filter(|r| now > r.expires_at)
            .map(|r| r.bundle_id)
            .collect()
    }

    /// Get time remaining for a bundle
    ///
    /// Returns None if bundle is not tracked or has expired.
    pub fn time_remaining(&self, bundle_id: &BundleId) -> Option<Duration> {
        self.tracked.get(bundle_id).and_then(|r| {
            let now = Utc::now();
            if now > r.expires_at {
                None
            } else {
                let remaining = r.expires_at - now;
                Some(Duration::from_millis(remaining.num_milliseconds().max(0) as u64))
            }
        })
    }

    /// Get the age of a tracked bundle
    pub fn bundle_age(&self, bundle_id: &BundleId) -> Option<Duration> {
        self.tracked.get(bundle_id).map(|r| {
            let age = Utc::now() - r.created_at;
            Duration::from_millis(age.num_milliseconds().max(0) as u64)
        })
    }

    /// Clean up expired tracking records
    ///
    /// Returns the IDs of removed bundles.
    pub fn cleanup(&self) -> Vec<BundleId> {
        let expired = self.get_expired();
        for bundle_id in &expired {
            self.tracked.remove(bundle_id);
        }
        expired
    }

    /// Get number of tracked bundles
    pub fn tracked_count(&self) -> usize {
        self.tracked.len()
    }

    /// Check if a bundle is being tracked
    pub fn is_tracked(&self, bundle_id: &BundleId) -> bool {
        self.tracked.contains_key(bundle_id)
    }

    /// Get bundles expiring soon (within the given threshold)
    pub fn expiring_soon(&self, threshold: Duration) -> Vec<BundleId> {
        self.tracked
            .iter()
            .filter_map(|r| {
                let remaining = self.time_remaining(&r.bundle_id)?;
                if remaining <= threshold {
                    Some(r.bundle_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get bundles sorted by expiration time (soonest first)
    pub fn by_expiration(&self) -> Vec<BundleId> {
        let mut bundles: Vec<_> = self
            .tracked
            .iter()
            .map(|r| (r.bundle_id, r.expires_at))
            .collect();
        bundles.sort_by_key(|(_, expires)| *expires);
        bundles.into_iter().map(|(id, _)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use indras_core::{EncryptedPayload, Packet, PacketId, SimulationIdentity};

    use crate::bundle::Bundle;

    fn make_test_bundle(lifetime_secs: i64) -> Bundle<SimulationIdentity> {
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

        Bundle::from_packet(packet, ChronoDuration::seconds(lifetime_secs))
    }

    #[test]
    fn test_track_bundle() {
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(ExpirationConfig::default());
        let bundle = make_test_bundle(3600);

        manager.track(&bundle);
        assert!(manager.is_tracked(&bundle.bundle_id));
        assert!(!manager.is_expired(&bundle.bundle_id));
    }

    #[test]
    fn test_untrack_bundle() {
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(ExpirationConfig::default());
        let bundle = make_test_bundle(3600);

        manager.track(&bundle);
        assert!(manager.is_tracked(&bundle.bundle_id));

        manager.untrack(&bundle.bundle_id);
        assert!(!manager.is_tracked(&bundle.bundle_id));
    }

    #[test]
    fn test_time_remaining() {
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(ExpirationConfig::default());
        let bundle = make_test_bundle(3600); // 1 hour lifetime

        manager.track(&bundle);
        let remaining = manager.time_remaining(&bundle.bundle_id);
        assert!(remaining.is_some());

        // Should be close to 1 hour (allowing for test execution time)
        let remaining = remaining.unwrap();
        assert!(remaining.as_secs() > 3500);
        assert!(remaining.as_secs() <= 3600);
    }

    #[test]
    fn test_priority_demotion() {
        let config = ExpirationConfig {
            demotion_thresholds: vec![
                (Duration::from_secs(1), Priority::Normal),
                (Duration::from_secs(2), Priority::Low),
            ],
            ..Default::default()
        };
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(config);

        // Create a bundle with High priority
        let mut bundle = make_test_bundle(3600);
        bundle.packet.priority = Priority::High;

        // Fresh bundle should keep its priority
        assert_eq!(manager.effective_priority(&bundle), Priority::High);

        // Note: Testing actual demotion would require simulating time passage
        // which is complex in unit tests. In practice, we'd use a mock clock.
    }

    #[test]
    fn test_lifetime_capping() {
        let config = ExpirationConfig {
            max_lifetime: Duration::from_secs(100),
            ..Default::default()
        };
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(config);

        // Create bundle with longer lifetime than max
        let bundle = make_test_bundle(1000); // 1000 seconds
        manager.track(&bundle);

        // Time remaining should be capped at 100 seconds
        let remaining = manager.time_remaining(&bundle.bundle_id).unwrap();
        assert!(remaining.as_secs() <= 100);
    }

    #[test]
    fn test_tracked_count() {
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(ExpirationConfig::default());

        assert_eq!(manager.tracked_count(), 0);

        let bundle1 = make_test_bundle(3600);
        manager.track(&bundle1);
        assert_eq!(manager.tracked_count(), 1);

        // Track same bundle again (should not duplicate)
        manager.track(&bundle1);
        assert_eq!(manager.tracked_count(), 1);
    }
}
