//! Network Fault Injection Tests
//!
//! Tests the resilience of Indras DTN components to various failure conditions:
//! - Connection drops
//! - Intermittent connectivity
//! - Network partitions
//! - Message retry and recovery
//!
//! These tests use simulation identities and don't require real network connectivity.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use std::time::Duration;

use chrono::Duration as ChronoDuration;
use indras_core::{
    EncryptedPayload, NetworkTopology, Packet, PacketId, Priority, SimulationIdentity,
};
use indras_dtn::{
    AgeManager, Bundle, CustodyConfig, CustodyManager, DtnConfig, EpidemicConfig, EpidemicDecision,
    EpidemicRouter, ExpirationConfig, ProphetConfig, ProphetState, SuppressReason,
};

// ============================================================================
// Test Topology Implementation
// ============================================================================

/// Simple test topology for fault injection tests
struct TestTopology {
    connections: HashMap<SimulationIdentity, Vec<SimulationIdentity>>,
    online: RwLock<HashSet<SimulationIdentity>>,
}

impl TestTopology {
    fn new() -> Self {
        Self {
            connections: HashMap::new(),
            online: RwLock::new(HashSet::new()),
        }
    }

    fn add_connection(&mut self, a: SimulationIdentity, b: SimulationIdentity) {
        self.connections.entry(a).or_default().push(b);
        self.connections.entry(b).or_default().push(a);
    }

    fn set_online(&self, peer: SimulationIdentity) {
        self.online.write().unwrap().insert(peer);
    }

    fn set_offline(&self, peer: &SimulationIdentity) {
        self.online.write().unwrap().remove(peer);
    }

    fn remove_connection(&mut self, a: &SimulationIdentity, b: &SimulationIdentity) {
        if let Some(neighbors) = self.connections.get_mut(a) {
            neighbors.retain(|n| n != b);
        }
        if let Some(neighbors) = self.connections.get_mut(b) {
            neighbors.retain(|n| n != a);
        }
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

// ============================================================================
// Helper functions for test setup
// ============================================================================

fn make_id(c: char) -> SimulationIdentity {
    SimulationIdentity::new(c).unwrap()
}

fn make_packet(source: char, dest: char, seq: u64) -> Packet<SimulationIdentity> {
    let source_id = make_id(source);
    let dest_id = make_id(dest);
    // Use source char as part of hash for uniqueness
    let source_hash = (source as u64) << 8 | 0x1234;
    let id = PacketId::new(source_hash, seq);

    Packet::new(
        id,
        source_id,
        dest_id,
        EncryptedPayload::plaintext(vec![0u8; 100]),
        vec![],
    )
}

fn make_bundle(source: char, dest: char, seq: u64) -> Bundle<SimulationIdentity> {
    let packet = make_packet(source, dest, seq);
    Bundle::from_packet(packet, ChronoDuration::hours(1))
}

fn make_bundle_with_lifetime(
    source: char,
    dest: char,
    seq: u64,
    lifetime: ChronoDuration,
) -> Bundle<SimulationIdentity> {
    let packet = make_packet(source, dest, seq);
    Bundle::from_packet(packet, lifetime)
}

fn make_bundle_with_copies(
    source: char,
    dest: char,
    seq: u64,
    copies: u8,
) -> Bundle<SimulationIdentity> {
    let packet = make_packet(source, dest, seq);
    Bundle::from_packet(packet, ChronoDuration::hours(1)).with_copies(copies)
}

// ============================================================================
// Connection Drop Tests
// ============================================================================

/// Test that epidemic router handles peer disconnection gracefully
#[test]
fn test_epidemic_router_peer_disconnect() {
    let config = EpidemicConfig {
        spray_and_wait: false, // Pure epidemic for this test
        ..Default::default()
    };
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');
    let b = make_id('B');
    let c = make_id('C');

    // Setup: A connected to B and C, Z is destination
    topology.add_connection(a, b);
    topology.add_connection(a, c);
    topology.set_online(b);
    topology.set_online(c);

    let bundle = make_bundle('A', 'Z', 1);

    // Phase 1: Both peers online - should flood to both
    let decision1 = router.route(&bundle, &a, &topology);
    match decision1 {
        EpidemicDecision::FloodAll { neighbors } => {
            assert_eq!(neighbors.len(), 2, "Should flood to both online neighbors");
        }
        _ => panic!("Expected FloodAll decision, got {:?}", decision1),
    }

    // Phase 2: B goes offline
    topology.set_offline(&b);

    // Create new bundle (old one is marked as seen)
    let bundle2 = make_bundle('A', 'Z', 2);
    let decision2 = router.route(&bundle2, &a, &topology);

    match decision2 {
        EpidemicDecision::FloodAll { neighbors } => {
            assert_eq!(neighbors.len(), 1, "Should only flood to online peer C");
            assert!(neighbors.contains(&c));
            assert!(!neighbors.contains(&b));
        }
        _ => panic!("Expected FloodAll decision, got {:?}", decision2),
    }

    // Phase 3: B comes back online
    topology.set_online(b);

    let bundle3 = make_bundle('A', 'Z', 3);
    let decision3 = router.route(&bundle3, &a, &topology);

    match decision3 {
        EpidemicDecision::FloodAll { neighbors } => {
            assert_eq!(neighbors.len(), 2, "Should flood to both after B recovers");
        }
        _ => panic!("Expected FloodAll after recovery, got {:?}", decision3),
    }
}

/// Test custody transfer on simulated link failure
#[test]
fn test_custody_transfer_link_failure() {
    let config = CustodyConfig::default();
    let custody_a: CustodyManager<SimulationIdentity> = CustodyManager::new(config.clone());
    let _custody_b: CustodyManager<SimulationIdentity> = CustodyManager::new(config);

    // Node A accepts custody of a bundle
    let bundle = make_bundle('X', 'Z', 1);
    let bundle_id = bundle.bundle_id;

    let accepted = custody_a.accept_custody(&bundle, None);
    assert!(accepted.is_ok(), "Should accept custody");
    assert!(custody_a.has_custody(&bundle_id));

    // Node A offers to transfer to B
    let offer = custody_a.offer_custody(bundle_id, make_id('B'));
    assert!(offer.is_ok(), "Should be able to offer custody");

    // Simulate link failure - B never responds
    // After timeout, A should still have custody
    assert!(
        custody_a.has_custody(&bundle_id),
        "A should retain custody during pending transfer"
    );

    // Check for timed-out transfers (would need to wait, but for unit test we verify API)
    let _timed_out = custody_a.check_timeouts();

    // Simulate successful acceptance
    let result = custody_a.handle_acceptance(bundle_id, true);
    match result {
        indras_dtn::CustodyTransferResult::Accepted { new_custodian, .. } => {
            assert_eq!(new_custodian, make_id('B'));
        }
        _ => panic!("Expected accepted result"),
    }

    // A should have released custody
    assert!(
        !custody_a.has_custody(&bundle_id),
        "A should release custody after successful transfer"
    );
}

// ============================================================================
// Intermittent Connectivity Tests
// ============================================================================

/// Test PRoPHET routing with intermittent encounters
#[test]
fn test_prophet_intermittent_connectivity() {
    let config = ProphetConfig {
        decay_interval: Duration::from_millis(1), // Fast decay for testing
        aging_constant: 0.5,                      // Aggressive aging
        initial_probability: 0.75,
        ..Default::default()
    };

    let prophet_a = ProphetState::new(make_id('A'), config);

    // First encounter - establish relationship
    prophet_a.encounter(&make_id('B'));
    let p1 = prophet_a.get_probability(&make_id('B'));
    assert!(p1 > 0.0, "Should have probability after encounter");
    assert!(
        (p1 - 0.75).abs() < 0.01,
        "Initial probability should be ~0.75"
    );

    // Second encounter - probability increases
    prophet_a.encounter(&make_id('B'));
    let p2 = prophet_a.get_probability(&make_id('B'));
    assert!(p2 > p1, "Probability should increase with encounters");

    // Simulate time passing without encounters (force aging)
    std::thread::sleep(Duration::from_millis(10));
    prophet_a.force_age();

    let p3 = prophet_a.get_probability(&make_id('B'));
    assert!(p3 < p2, "Probability should decay without encounters");

    // Reconnect - probability increases again
    prophet_a.encounter(&make_id('B'));
    let p4 = prophet_a.get_probability(&make_id('B'));
    assert!(p4 > p3, "Probability should recover on reconnection");
}

/// Test transitive routing through intermittent peers
#[test]
fn test_prophet_transitive_routing() {
    let config = ProphetConfig::default();

    let prophet_a = ProphetState::new(make_id('A'), config.clone());
    let prophet_b = ProphetState::new(make_id('B'), config);

    // B has frequent contact with Z (destination)
    prophet_b.encounter(&make_id('Z'));
    prophet_b.encounter(&make_id('Z'));
    prophet_b.encounter(&make_id('Z'));

    let b_prob_to_z = prophet_b.get_probability(&make_id('Z'));
    assert!(b_prob_to_z > 0.9, "B should have high probability to Z");

    // A encounters B
    prophet_a.encounter(&make_id('B'));

    // A doesn't know Z directly
    assert_eq!(prophet_a.get_probability(&make_id('Z')), 0.0);

    // A learns about Z through B (transitive update)
    let b_probs = prophet_b.all_probabilities();
    prophet_a.transitive_update(&make_id('B'), &b_probs);

    // Now A should have some probability to Z through B
    let a_prob_to_z = prophet_a.get_probability(&make_id('Z'));
    assert!(
        a_prob_to_z > 0.0,
        "A should have transitive probability to Z"
    );
}

/// Test PRoPHET best candidate selection
#[test]
fn test_prophet_best_candidate_selection() {
    let config = ProphetConfig::default();
    let prophet_a = ProphetState::new(make_id('A'), config);

    // A knows B well (multiple encounters)
    for _ in 0..5 {
        prophet_a.encounter(&make_id('B'));
    }

    // A knows C less well (single encounter)
    prophet_a.encounter(&make_id('C'));

    let prob_b = prophet_a.get_probability(&make_id('B'));
    let prob_c = prophet_a.get_probability(&make_id('C'));

    assert!(prob_b > prob_c, "B should have higher probability than C");

    // For destination Z (unknown), best_candidate returns the candidate we can reach
    // with highest probability IF that's higher than our probability to Z.
    // Since we know B well (prob_b > 0) and don't know Z (prob = 0), B should be selected.
    let candidates = vec![make_id('B'), make_id('C')];
    let best = prophet_a.best_candidate(&make_id('Z'), &candidates);

    // best_candidate returns the peer we have highest probability to, if > our prob to dest
    // Since prob_b > 0 = our_prob_to_Z, B should be selected
    assert!(best.is_some(), "Should select B as best candidate");
    assert_eq!(
        best.unwrap(),
        make_id('B'),
        "B should be selected (highest probability peer)"
    );
}

// ============================================================================
// Network Partition Tests
// ============================================================================

/// Simulate network partition and recovery with epidemic routing
#[test]
fn test_partition_and_recovery_epidemic() {
    let config = EpidemicConfig {
        spray_and_wait: true,
        spray_count: 4,
        ..Default::default()
    };
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');
    let b = make_id('B');
    let c = make_id('C');
    let d = make_id('D');

    // Phase 1: Normal operation - A connected to B, C, D
    topology.add_connection(a, b);
    topology.add_connection(a, c);
    topology.add_connection(a, d);
    topology.set_online(b);
    topology.set_online(c);
    topology.set_online(d);

    let bundle1 = make_bundle_with_copies('A', 'Z', 1, 4);
    let decision1 = router.route(&bundle1, &a, &topology);

    match decision1 {
        EpidemicDecision::SprayTo { targets, .. } => {
            assert!(!targets.is_empty(), "Should spray to available peers");
        }
        _ => panic!("Expected SprayTo decision, got {:?}", decision1),
    }

    // Phase 2: Partition - B and C go offline
    topology.set_offline(&b);
    topology.set_offline(&c);

    let bundle2 = make_bundle_with_copies('A', 'Z', 2, 4);
    let decision2 = router.route(&bundle2, &a, &topology);

    match decision2 {
        EpidemicDecision::SprayTo { targets, .. } => {
            assert_eq!(targets.len(), 1, "Should only spray to D");
            assert!(targets.contains(&d));
        }
        EpidemicDecision::DirectDelivery { .. } => {}
        _ => panic!("Expected SprayTo or DirectDelivery, got {:?}", decision2),
    }

    // Phase 3: Recovery - B comes back online
    topology.set_online(b);

    let bundle3 = make_bundle_with_copies('A', 'Z', 3, 4);
    let decision3 = router.route(&bundle3, &a, &topology);

    match decision3 {
        EpidemicDecision::SprayTo { targets, .. } => {
            assert_eq!(targets.len(), 2, "Should spray to B and D");
        }
        _ => panic!("Expected SprayTo after recovery, got {:?}", decision3),
    }
}

/// Test topology changes during routing
#[test]
fn test_topology_mutation() {
    let config = EpidemicConfig {
        spray_and_wait: false, // Pure epidemic
        ..Default::default()
    };
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');
    let b = make_id('B');
    let c = make_id('C');

    // Initial: A-B only
    topology.add_connection(a, b);
    topology.set_online(b);

    let bundle1 = make_bundle('A', 'Z', 1);
    let decision1 = router.route(&bundle1, &a, &topology);

    match decision1 {
        EpidemicDecision::FloodAll { neighbors } => {
            assert_eq!(neighbors.len(), 1);
            assert!(neighbors.contains(&b));
        }
        _ => panic!("Expected FloodAll, got {:?}", decision1),
    }

    // Add new connection A-C
    topology.add_connection(a, c);
    topology.set_online(c);

    let bundle2 = make_bundle('A', 'Z', 2);
    let decision2 = router.route(&bundle2, &a, &topology);

    match decision2 {
        EpidemicDecision::FloodAll { neighbors } => {
            assert_eq!(neighbors.len(), 2);
        }
        _ => panic!("Expected FloodAll with 2 neighbors, got {:?}", decision2),
    }

    // Remove connection A-B
    topology.remove_connection(&a, &b);

    let bundle3 = make_bundle('A', 'Z', 3);
    let decision3 = router.route(&bundle3, &a, &topology);

    match decision3 {
        EpidemicDecision::FloodAll { neighbors } => {
            assert_eq!(neighbors.len(), 1);
            assert!(neighbors.contains(&c));
        }
        _ => panic!("Expected FloodAll with 1 neighbor, got {:?}", decision3),
    }
}

// ============================================================================
// Message Retry and Recovery Tests
// ============================================================================

/// Test bundle expiration handling during outages
#[test]
fn test_bundle_expiration_during_outage() {
    let config = ExpirationConfig {
        default_lifetime: Duration::from_millis(100), // Short lifetime for testing
        max_lifetime: Duration::from_millis(500),
        demotion_thresholds: vec![(Duration::from_millis(50), Priority::Normal)],
        cleanup_interval: Duration::from_millis(10),
    };

    let age_manager: AgeManager<SimulationIdentity> = AgeManager::new(config);

    // Create a bundle with short lifetime
    let bundle = make_bundle_with_lifetime('A', 'Z', 1, ChronoDuration::milliseconds(100));
    let bundle_id = bundle.bundle_id;
    age_manager.track(&bundle);

    // Initially not expired
    assert!(
        !age_manager.is_expired(&bundle_id),
        "Should not be expired initially"
    );
    assert!(age_manager.is_tracked(&bundle_id));

    // Wait for expiration
    std::thread::sleep(Duration::from_millis(150));

    // Now should be expired
    assert!(
        age_manager.is_expired(&bundle_id),
        "Should be expired after lifetime"
    );

    // Run cleanup
    let expired = age_manager.cleanup();
    assert!(
        expired.contains(&bundle_id),
        "Expired bundle should be cleaned up"
    );
    assert!(!age_manager.is_tracked(&bundle_id));
}

/// Test spray-and-wait strategy under failures
#[test]
fn test_spray_and_wait_with_failures() {
    let config = EpidemicConfig {
        spray_count: 4,
        spray_and_wait: true,
        max_copies: 8,
        ..Default::default()
    };

    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');

    // Setup with 4 peers
    for c in ['B', 'C', 'D', 'E'] {
        let peer = make_id(c);
        topology.add_connection(a, peer);
        topology.set_online(peer);
    }

    // Bundle with 4 copies
    let bundle = make_bundle_with_copies('A', 'Z', 1, 4);

    let decision = router.route(&bundle, &a, &topology);

    match decision {
        EpidemicDecision::SprayTo {
            targets,
            copies_remaining,
        } => {
            // With 4 copies, should spray to ceil(4/2) = 2 targets
            assert_eq!(targets.len(), 2, "Should spray to 2 targets");
            assert_eq!(copies_remaining, 2, "Should have 2 copies remaining");
        }
        _ => panic!("Expected SprayTo decision, got {:?}", decision),
    }
}

/// Test wait phase behavior
#[test]
fn test_spray_and_wait_phase() {
    let config = EpidemicConfig {
        spray_and_wait: true,
        ..Default::default()
    };

    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');
    let b = make_id('B');

    topology.add_connection(a, b);
    topology.set_online(b);

    // Bundle with only 1 copy - wait phase
    let bundle = make_bundle_with_copies('A', 'Z', 1, 1);

    let decision = router.route(&bundle, &a, &topology);

    match decision {
        EpidemicDecision::Suppress { reason } => {
            assert_eq!(reason, SuppressReason::WaitPhase);
        }
        _ => panic!("Expected Suppress with WaitPhase, got {:?}", decision),
    }
}

/// Test direct delivery when destination is neighbor
#[test]
fn test_direct_delivery() {
    let config = EpidemicConfig::default();
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');
    let z = make_id('Z');

    // A is directly connected to destination Z
    topology.add_connection(a, z);
    topology.set_online(z);

    let bundle = make_bundle('A', 'Z', 1);

    let decision = router.route(&bundle, &a, &topology);

    match decision {
        EpidemicDecision::DirectDelivery { destination } => {
            assert_eq!(destination, z);
        }
        _ => panic!("Expected DirectDelivery, got {:?}", decision),
    }
}

// ============================================================================
// Duplicate Detection Tests
// ============================================================================

/// Test duplicate suppression
#[test]
fn test_duplicate_suppression() {
    let config = EpidemicConfig::default();
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');
    let b = make_id('B');

    topology.add_connection(a, b);
    topology.set_online(b);

    let bundle = make_bundle('X', 'Z', 1);
    let bundle_id = bundle.bundle_id;

    // First routing - should proceed
    let decision1 = router.route(&bundle, &a, &topology);
    assert!(
        decision1.is_forwarding()
            || matches!(
                decision1,
                EpidemicDecision::Suppress {
                    reason: SuppressReason::WaitPhase
                }
            )
    );

    // Mark as seen (simulating receipt from another path)
    router.mark_seen(bundle_id);

    // Create same bundle again
    let bundle2 = make_bundle('X', 'Z', 1);
    let decision2 = router.route(&bundle2, &a, &topology);

    match decision2 {
        EpidemicDecision::Suppress { reason } => {
            assert_eq!(reason, SuppressReason::Duplicate);
        }
        _ => panic!("Expected Suppress(Duplicate), got {:?}", decision2),
    }
}

/// Test seen bundle cleanup
#[test]
fn test_seen_cleanup() {
    let config = EpidemicConfig {
        seen_timeout: Duration::from_millis(50), // Very short for testing
        ..Default::default()
    };
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    // Mark many bundles as seen
    for i in 0..100 {
        let bundle = make_bundle('A', 'Z', i);
        router.mark_seen(bundle.bundle_id);
    }

    assert_eq!(router.seen_bundle_count(), 100);

    // Wait for timeout
    std::thread::sleep(Duration::from_millis(100));

    // Cleanup
    let removed = router.cleanup_seen();
    assert_eq!(removed, 100);
    assert_eq!(router.seen_bundle_count(), 0);
}

// ============================================================================
// Custody Manager Tests
// ============================================================================

/// Test custody capacity limits
#[test]
fn test_custody_capacity() {
    let config = CustodyConfig {
        max_custody_bundles: 3,
        ..Default::default()
    };
    let custody: CustodyManager<SimulationIdentity> = CustodyManager::new(config);

    // Accept up to capacity
    for i in 0..3 {
        let bundle = make_bundle('A', 'Z', i);
        let result = custody.accept_custody(&bundle, None);
        assert!(result.is_ok(), "Should accept bundle {}", i);
    }

    assert_eq!(custody.custody_count(), 3);
    assert_eq!(custody.remaining_capacity(), 0);

    // Try to exceed capacity
    let bundle4 = make_bundle('A', 'Z', 4);
    let result = custody.accept_custody(&bundle4, None);
    assert!(result.is_err(), "Should reject when at capacity");
}

/// Test custody release
#[test]
fn test_custody_release() {
    let custody: CustodyManager<SimulationIdentity> = CustodyManager::new(CustodyConfig::default());

    let bundle = make_bundle('A', 'Z', 1);
    let bundle_id = bundle.bundle_id;

    custody.accept_custody(&bundle, None).unwrap();
    assert!(custody.has_custody(&bundle_id));

    let released = custody.release_custody(&bundle_id);
    assert!(released.is_some());
    assert!(!custody.has_custody(&bundle_id));
}

// ============================================================================
// Stress Tests for Fault Handling
// ============================================================================

/// Test handling many simultaneous routing decisions
#[test]
fn test_mass_routing_decisions() {
    let config = EpidemicConfig {
        spray_and_wait: true,
        spray_count: 4,
        ..Default::default()
    };
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');

    // Connect many peers (B through Z = 25 peers)
    for i in 0..25 {
        let peer = make_id((b'B' + i) as char);
        topology.add_connection(a, peer);
        topology.set_online(peer);
    }

    // Route many bundles
    for i in 0..1000 {
        let bundle = make_bundle_with_copies('A', 'Z', i, 4);
        let decision = router.route(&bundle, &a, &topology);

        // Should either spray or suppress (after seeing)
        assert!(
            decision.is_forwarding() || decision.is_suppress(),
            "Decision {} should be valid: {:?}",
            i,
            decision
        );
    }
}

/// Test rapid PRoPHET probability updates
#[test]
fn test_rapid_prophet_updates() {
    let config = ProphetConfig::default();
    let prophet = ProphetState::new(make_id('A'), config);

    // Rapid encounters with many peers
    for _ in 0..100 {
        for c in 'B'..='Z' {
            prophet.encounter(&make_id(c));
        }
    }

    // All peers should have high probability
    for c in 'B'..='Z' {
        let prob = prophet.get_probability(&make_id(c));
        assert!(prob > 0.9, "Peer {} should have high probability", c);
    }

    assert_eq!(prophet.known_destinations(), 25);
}

// ============================================================================
// Config Validation Tests
// ============================================================================

/// Test DTN config validation with edge cases
#[test]
fn test_config_validation_edge_cases() {
    // Valid default config
    assert!(DtnConfig::default().is_valid());

    // Valid preset configs
    assert!(DtnConfig::low_latency().is_valid());
    assert!(DtnConfig::challenged_network().is_valid());
    assert!(DtnConfig::resource_constrained().is_valid());

    // Config with invalid settings
    let mut invalid = DtnConfig::default();
    invalid.expiration.default_lifetime = Duration::from_secs(86400 * 30);
    invalid.expiration.max_lifetime = Duration::from_secs(3600);

    let warnings = invalid.validate();
    assert!(!warnings.is_empty(), "Should detect invalid config");
}

/// Test bundle creation and ID uniqueness
#[test]
fn test_bundle_id_uniqueness() {
    let mut ids = HashSet::new();

    for i in 0..1000 {
        let bundle = make_bundle('A', 'Z', i);
        assert!(ids.insert(bundle.bundle_id), "Bundle IDs should be unique");
    }

    assert_eq!(ids.len(), 1000);
}

// ============================================================================
// No Neighbors Edge Case
// ============================================================================

/// Test routing with no available neighbors
#[test]
fn test_no_neighbors() {
    let config = EpidemicConfig::default();
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let topology = TestTopology::new(); // Empty topology
    let a = make_id('A');
    let bundle = make_bundle('A', 'Z', 1);

    let decision = router.route(&bundle, &a, &topology);

    match decision {
        EpidemicDecision::Suppress { reason } => {
            assert_eq!(reason, SuppressReason::NoNeighbors);
        }
        _ => panic!("Expected Suppress(NoNeighbors), got {:?}", decision),
    }
}

/// Test routing when all neighbors are offline
#[test]
fn test_all_neighbors_offline() {
    let config = EpidemicConfig::default();
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    let mut topology = TestTopology::new();
    let a = make_id('A');
    let b = make_id('B');
    let c = make_id('C');

    topology.add_connection(a, b);
    topology.add_connection(a, c);
    // Don't set any peer online

    let bundle = make_bundle('A', 'Z', 1);
    let decision = router.route(&bundle, &a, &topology);

    match decision {
        EpidemicDecision::Suppress { reason } => {
            assert_eq!(reason, SuppressReason::NoNeighbors);
        }
        _ => panic!(
            "Expected Suppress(NoNeighbors) when all offline, got {:?}",
            decision
        ),
    }
}
