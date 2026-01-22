//! Epidemic routing for DTN bundles
//!
//! Epidemic routing maximizes delivery probability by flooding bundles
//! to all available neighbors. It's effective in networks with
//! intermittent connectivity but uses more bandwidth and storage.
//!
//! This module supports two modes:
//! - **Pure epidemic**: Forward to all neighbors
//! - **Spray-and-wait**: Forward limited copies, then wait for direct delivery

use std::time::{Duration, Instant};

use dashmap::DashMap;

use indras_core::{NetworkTopology, PeerIdentity};

use crate::bundle::{Bundle, BundleId};

/// Configuration for epidemic routing
#[derive(Debug, Clone)]
pub struct EpidemicConfig {
    /// Maximum copies of a bundle to forward (for spray-and-wait)
    pub max_copies: u8,
    /// Whether to use spray-and-wait mode (vs pure epidemic)
    pub spray_and_wait: bool,
    /// Initial spray count for spray-and-wait
    pub spray_count: u8,
    /// How long to remember seen bundles (for duplicate detection)
    pub seen_timeout: Duration,
    /// Maximum bundle age to accept
    pub max_bundle_age: Duration,
}

impl Default for EpidemicConfig {
    fn default() -> Self {
        Self {
            max_copies: 8,
            spray_and_wait: true,
            spray_count: 4,
            seen_timeout: Duration::from_secs(3600),
            max_bundle_age: Duration::from_secs(86400), // 24 hours
        }
    }
}

/// Record of a seen bundle for duplicate detection
#[derive(Debug, Clone)]
struct SeenRecord {
    /// When we first saw this bundle
    first_seen: Instant,
    /// Number of times we've seen it
    count: u32,
}

/// Reason for suppressing a bundle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressReason {
    /// Bundle was already seen (duplicate)
    Duplicate,
    /// No available neighbors to forward to
    NoNeighbors,
    /// In wait phase of spray-and-wait (only 1 copy left)
    WaitPhase,
}

/// Routing decision from epidemic router
#[derive(Debug, Clone)]
pub enum EpidemicDecision<I: PeerIdentity> {
    /// Forward to all online neighbors (pure epidemic)
    FloodAll { neighbors: Vec<I> },
    /// Forward to a subset of neighbors (spray-and-wait)
    SprayTo {
        targets: Vec<I>,
        copies_remaining: u8,
    },
    /// Direct delivery is possible (destination is a neighbor)
    DirectDelivery { destination: I },
    /// Hold bundle but don't forward (store for later delivery)
    Suppress { reason: SuppressReason },
    /// Bundle has expired
    Expired,
}

impl<I: PeerIdentity> EpidemicDecision<I> {
    /// Check if this decision results in forwarding
    pub fn is_forwarding(&self) -> bool {
        matches!(
            self,
            EpidemicDecision::FloodAll { .. }
                | EpidemicDecision::SprayTo { .. }
                | EpidemicDecision::DirectDelivery { .. }
        )
    }

    /// Check if this is a suppress decision
    pub fn is_suppress(&self) -> bool {
        matches!(self, EpidemicDecision::Suppress { .. })
    }

    /// Get the suppress reason if this is a suppress decision
    pub fn suppress_reason(&self) -> Option<SuppressReason> {
        match self {
            EpidemicDecision::Suppress { reason } => Some(*reason),
            _ => None,
        }
    }

    /// Get the targets for forwarding (if any)
    pub fn targets(&self) -> Vec<I> {
        match self {
            EpidemicDecision::FloodAll { neighbors } => neighbors.clone(),
            EpidemicDecision::SprayTo { targets, .. } => targets.clone(),
            EpidemicDecision::DirectDelivery { destination } => vec![destination.clone()],
            EpidemicDecision::Suppress { .. } | EpidemicDecision::Expired => vec![],
        }
    }
}

/// Epidemic router for DTN bundles
///
/// Implements flood-based routing strategies to maximize delivery
/// probability in challenged networks.
pub struct EpidemicRouter<I: PeerIdentity> {
    /// Bundles we've seen (for duplicate suppression)
    seen_bundles: DashMap<BundleId, SeenRecord>,
    /// Configuration
    config: EpidemicConfig,
    /// Type marker
    _marker: std::marker::PhantomData<I>,
}

impl<I: PeerIdentity> EpidemicRouter<I> {
    /// Create a new epidemic router
    pub fn new(config: EpidemicConfig) -> Self {
        Self {
            seen_bundles: DashMap::new(),
            config,
            _marker: std::marker::PhantomData,
        }
    }

    /// Make a routing decision for a bundle
    pub fn route<T: NetworkTopology<I>>(
        &self,
        bundle: &Bundle<I>,
        current: &I,
        topology: &T,
    ) -> EpidemicDecision<I> {
        // Check if bundle has expired
        if bundle.is_expired() {
            return EpidemicDecision::Expired;
        }

        // Check bundle age against our maximum
        let age = bundle.age();
        if age.num_seconds() > self.config.max_bundle_age.as_secs() as i64 {
            return EpidemicDecision::Expired;
        }

        // Check for duplicate
        if self.have_seen(&bundle.bundle_id) {
            return EpidemicDecision::Suppress {
                reason: SuppressReason::Duplicate,
            };
        }

        // Mark as seen
        self.mark_seen(bundle.bundle_id);

        // Get online neighbors (excluding visited nodes and self)
        let neighbors: Vec<I> = topology
            .neighbors(current)
            .into_iter()
            .filter(|n| topology.is_online(n))
            .filter(|n| !bundle.packet.was_visited(n))
            .filter(|n| n != current)
            .collect();

        // Check for direct delivery
        let destination = bundle.destination();
        if neighbors.contains(destination) {
            return EpidemicDecision::DirectDelivery {
                destination: destination.clone(),
            };
        }

        if neighbors.is_empty() {
            return EpidemicDecision::Suppress {
                reason: SuppressReason::NoNeighbors,
            };
        }

        // Decide based on mode
        if self.config.spray_and_wait {
            self.spray_and_wait_decision(bundle, neighbors)
        } else {
            EpidemicDecision::FloodAll { neighbors }
        }
    }

    /// Make a spray-and-wait routing decision
    ///
    /// In spray phase (copies > 1): distribute copies among neighbors
    /// In wait phase (copies == 1): hold bundle, only deliver directly to destination
    fn spray_and_wait_decision(
        &self,
        bundle: &Bundle<I>,
        neighbors: Vec<I>,
    ) -> EpidemicDecision<I> {
        let copies = bundle.copies_remaining;

        if copies <= 1 {
            // Wait phase: hold the bundle and only deliver directly to destination
            // Direct delivery is already checked in route() before this is called,
            // so if we're here, we should hold the bundle for later.
            EpidemicDecision::Suppress {
                reason: SuppressReason::WaitPhase,
            }
        } else {
            // Spray phase: distribute half of copies among neighbors
            // Use ceiling division to ensure we spray at least 1 copy
            let copies_to_spray = ((copies + 1) / 2) as usize;
            let targets: Vec<I> = neighbors.into_iter().take(copies_to_spray).collect();
            let remaining = copies.saturating_sub(targets.len() as u8);

            EpidemicDecision::SprayTo {
                targets,
                copies_remaining: remaining,
            }
        }
    }

    /// Record that we've seen a bundle
    pub fn mark_seen(&self, bundle_id: BundleId) {
        self.seen_bundles
            .entry(bundle_id)
            .and_modify(|r| r.count += 1)
            .or_insert(SeenRecord {
                first_seen: Instant::now(),
                count: 1,
            });
    }

    /// Check if we've seen this bundle before
    pub fn have_seen(&self, bundle_id: &BundleId) -> bool {
        self.seen_bundles.contains_key(bundle_id)
    }

    /// Get the number of times we've seen a bundle
    pub fn seen_count(&self, bundle_id: &BundleId) -> u32 {
        self.seen_bundles
            .get(bundle_id)
            .map(|r| r.count)
            .unwrap_or(0)
    }

    /// Clean up old seen records
    ///
    /// **Important**: This method should be called periodically (e.g., via a background task)
    /// to prevent unbounded memory growth. The seen_bundles map will grow indefinitely
    /// if cleanup is never called. Recommended interval: `config.seen_timeout / 2`.
    ///
    /// Returns the number of records removed.
    pub fn cleanup_seen(&self) -> usize {
        let now = Instant::now();
        let mut removed = 0;

        self.seen_bundles.retain(|_, record| {
            let keep = now.duration_since(record.first_seen) < self.config.seen_timeout;
            if !keep {
                removed += 1;
            }
            keep
        });

        if removed > 0 {
            tracing::debug!(removed, remaining = self.seen_bundles.len(), "Cleaned up seen bundle records");
        }

        removed
    }

    /// Get the number of seen bundles being tracked
    pub fn seen_bundle_count(&self) -> usize {
        self.seen_bundles.len()
    }

    /// Update bundle copies after spray decision
    ///
    /// Returns true if the bundle should be kept (has copies remaining).
    pub fn decrement_copies(&self, bundle: &mut Bundle<I>) -> bool {
        bundle.decrement_copies()
    }

    /// Calculate spray targets for a bundle with given copies
    ///
    /// Uses ceiling division to ensure copies are distributed fairly.
    pub fn calculate_spray_targets(&self, copies: u8, available_neighbors: usize) -> usize {
        if copies <= 1 {
            0 // Wait phase
        } else {
            // Spray half of copies using ceiling division, limited by available neighbors
            let spray = ((copies + 1) / 2) as usize;
            spray.min(available_neighbors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use indras_core::{EncryptedPayload, Packet, PacketId, SimulationIdentity};
    use std::collections::HashMap;
    use std::sync::RwLock;

    use crate::bundle::Bundle;

    /// Simple test topology
    struct TestTopology {
        connections: HashMap<SimulationIdentity, Vec<SimulationIdentity>>,
        online: RwLock<std::collections::HashSet<SimulationIdentity>>,
    }

    impl TestTopology {
        fn new() -> Self {
            Self {
                connections: HashMap::new(),
                online: RwLock::new(std::collections::HashSet::new()),
            }
        }

        fn add_connection(&mut self, a: SimulationIdentity, b: SimulationIdentity) {
            self.connections.entry(a).or_default().push(b);
            self.connections.entry(b).or_default().push(a);
        }

        fn set_online(&self, peer: SimulationIdentity) {
            self.online.write().unwrap().insert(peer);
        }
    }

    impl NetworkTopology<SimulationIdentity> for TestTopology {
        fn peers(&self) -> Vec<SimulationIdentity> {
            self.connections.keys().cloned().collect()
        }

        fn neighbors(&self, peer: &SimulationIdentity) -> Vec<SimulationIdentity> {
            self.connections.get(peer).cloned().unwrap_or_default()
        }

        fn are_connected(&self, a: &SimulationIdentity, b: &SimulationIdentity) -> bool {
            self.connections
                .get(a)
                .map(|n| n.contains(b))
                .unwrap_or(false)
        }

        fn is_online(&self, peer: &SimulationIdentity) -> bool {
            self.online.read().unwrap().contains(peer)
        }
    }

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

        Bundle::from_packet(packet, ChronoDuration::hours(1)).with_copies(4)
    }

    #[test]
    fn test_epidemic_flood_all() {
        let config = EpidemicConfig {
            spray_and_wait: false,
            ..Default::default()
        };
        let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();

        topology.add_connection(a, b);
        topology.add_connection(a, c);
        topology.set_online(b);
        topology.set_online(c);

        let bundle = make_test_bundle();
        let decision = router.route(&bundle, &a, &topology);

        match decision {
            EpidemicDecision::FloodAll { neighbors } => {
                assert_eq!(neighbors.len(), 2);
            }
            _ => panic!("Expected FloodAll decision"),
        }
    }

    #[test]
    fn test_spray_and_wait() {
        let config = EpidemicConfig {
            spray_and_wait: true,
            spray_count: 4,
            ..Default::default()
        };
        let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();
        let d = SimulationIdentity::new('D').unwrap();

        topology.add_connection(a, b);
        topology.add_connection(a, c);
        topology.add_connection(a, d);
        topology.set_online(b);
        topology.set_online(c);
        topology.set_online(d);

        let bundle = make_test_bundle(); // Has 4 copies
        let decision = router.route(&bundle, &a, &topology);

        match decision {
            EpidemicDecision::SprayTo {
                targets,
                copies_remaining,
            } => {
                // With 4 copies, should spray to 2 targets (4/2)
                assert_eq!(targets.len(), 2);
                assert_eq!(copies_remaining, 2);
            }
            _ => panic!("Expected SprayTo decision, got {:?}", decision),
        }
    }

    #[test]
    fn test_direct_delivery() {
        let router: EpidemicRouter<SimulationIdentity> =
            EpidemicRouter::new(EpidemicConfig::default());

        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let z = SimulationIdentity::new('Z').unwrap();

        topology.add_connection(a, z);
        topology.set_online(z);

        let bundle = make_test_bundle();
        let decision = router.route(&bundle, &a, &topology);

        assert!(matches!(
            decision,
            EpidemicDecision::DirectDelivery { .. }
        ));
    }

    #[test]
    fn test_duplicate_suppression() {
        let router: EpidemicRouter<SimulationIdentity> =
            EpidemicRouter::new(EpidemicConfig::default());

        let bundle = make_test_bundle();
        let bundle_id = bundle.bundle_id;

        assert!(!router.have_seen(&bundle_id));

        router.mark_seen(bundle_id);
        assert!(router.have_seen(&bundle_id));
        assert_eq!(router.seen_count(&bundle_id), 1);

        router.mark_seen(bundle_id);
        assert_eq!(router.seen_count(&bundle_id), 2);
    }

    #[test]
    fn test_spray_target_calculation() {
        let router: EpidemicRouter<SimulationIdentity> =
            EpidemicRouter::new(EpidemicConfig::default());

        // 4 copies, 10 neighbors -> spray to ceil(4/2) = 2
        assert_eq!(router.calculate_spray_targets(4, 10), 2);

        // 3 copies, 10 neighbors -> spray to ceil(3/2) = 2
        assert_eq!(router.calculate_spray_targets(3, 10), 2);

        // 8 copies, 3 neighbors -> spray to 3 (limited by neighbors)
        assert_eq!(router.calculate_spray_targets(8, 3), 3);

        // 1 copy -> wait phase, no spray
        assert_eq!(router.calculate_spray_targets(1, 10), 0);

        // 2 copies -> spray 1, keep 1
        assert_eq!(router.calculate_spray_targets(2, 10), 1);
    }

    #[test]
    fn test_wait_phase() {
        let config = EpidemicConfig {
            spray_and_wait: true,
            ..Default::default()
        };
        let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

        let mut topology = TestTopology::new();
        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();

        topology.add_connection(a, b);
        topology.set_online(b);

        // Bundle with only 1 copy (wait phase)
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('Z').unwrap();
        let id = PacketId::new(0x5678, 2);
        let packet = Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(b"test".to_vec()),
            vec![],
        );
        let bundle = Bundle::from_packet(packet, ChronoDuration::hours(1)).with_copies(1);

        let decision = router.route(&bundle, &a, &topology);

        match decision {
            EpidemicDecision::Suppress { reason } => {
                assert_eq!(reason, SuppressReason::WaitPhase);
            }
            _ => panic!("Expected Suppress with WaitPhase reason, got {:?}", decision),
        }
    }
}
