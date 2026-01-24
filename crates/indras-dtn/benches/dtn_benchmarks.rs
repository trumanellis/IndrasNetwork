//! DTN (Delay-Tolerant Networking) performance benchmarks
//!
//! Benchmarks for critical DTN operations:
//! - Epidemic routing decisions
//! - PRoPHET probability calculations
//! - Custody management
//! - Bundle operations
//!
//! Run with: cargo bench -p indras-dtn

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use std::time::Duration;

use chrono::Duration as ChronoDuration;
use indras_core::{EncryptedPayload, NetworkTopology, Packet, PacketId, SimulationIdentity};
use indras_dtn::{
    AgeManager, Bundle, CustodyConfig, CustodyManager, DtnConfig, EpidemicConfig, EpidemicRouter,
    ExpirationConfig, ProphetConfig, ProphetState,
};

// ============================================================================
// Test Topology Implementation
// ============================================================================

struct BenchTopology {
    connections: HashMap<SimulationIdentity, Vec<SimulationIdentity>>,
    online: RwLock<HashSet<SimulationIdentity>>,
}

impl BenchTopology {
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

    fn with_star_topology(center: SimulationIdentity, peer_count: usize) -> Self {
        let mut topo = Self::new();
        for i in 0..peer_count {
            let peer = SimulationIdentity::new((b'B' + (i % 25) as u8) as char).unwrap();
            topo.add_connection(center, peer);
            topo.set_online(peer);
        }
        topo.set_online(center);
        topo
    }
}

impl NetworkTopology<SimulationIdentity> for BenchTopology {
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
// Helper Functions
// ============================================================================

fn make_id(c: char) -> SimulationIdentity {
    SimulationIdentity::new(c).unwrap()
}

fn make_bundle(source: char, dest: char, seq: u64) -> Bundle<SimulationIdentity> {
    let source_id = make_id(source);
    let dest_id = make_id(dest);
    let source_hash = (source as u64) << 8 | 0x1234;
    let id = PacketId::new(source_hash, seq);

    let packet = Packet::new(
        id,
        source_id,
        dest_id,
        EncryptedPayload::plaintext(vec![0u8; 100]),
        vec![],
    );

    Bundle::from_packet(packet, ChronoDuration::hours(1))
}

fn make_bundle_with_copies(
    source: char,
    dest: char,
    seq: u64,
    copies: u8,
) -> Bundle<SimulationIdentity> {
    make_bundle(source, dest, seq).with_copies(copies)
}

// ============================================================================
// Epidemic Router Benchmarks
// ============================================================================

fn bench_epidemic_routing(c: &mut Criterion) {
    let mut group = c.benchmark_group("epidemic_routing");

    // Small network (10 peers)
    let config = EpidemicConfig::default();
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config.clone());
    let small_topo = BenchTopology::with_star_topology(make_id('A'), 10);

    group.bench_function("route_decision_10_peers", |b| {
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            let bundle = make_bundle_with_copies('A', 'Z', seq, 4);
            router.route(
                black_box(&bundle),
                black_box(&make_id('A')),
                black_box(&small_topo),
            )
        })
    });

    // Medium network (25 peers)
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config.clone());
    let medium_topo = BenchTopology::with_star_topology(make_id('A'), 25);

    group.bench_function("route_decision_25_peers", |b| {
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            let bundle = make_bundle_with_copies('A', 'Z', seq, 4);
            router.route(
                black_box(&bundle),
                black_box(&make_id('A')),
                black_box(&medium_topo),
            )
        })
    });

    // Flood mode (not spray-and-wait)
    let flood_config = EpidemicConfig {
        spray_and_wait: false,
        ..Default::default()
    };
    let flood_router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(flood_config);

    group.bench_function("flood_all_25_peers", |b| {
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            let bundle = make_bundle('A', 'Z', seq);
            flood_router.route(
                black_box(&bundle),
                black_box(&make_id('A')),
                black_box(&medium_topo),
            )
        })
    });

    group.finish();
}

fn bench_duplicate_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("duplicate_detection");

    let config = EpidemicConfig::default();
    let router: EpidemicRouter<SimulationIdentity> = EpidemicRouter::new(config);

    // Pre-populate with seen bundles
    for i in 0..1000 {
        let bundle = make_bundle('A', 'Z', i);
        router.mark_seen(bundle.bundle_id);
    }

    group.bench_function("have_seen_1000_bundles", |b| {
        let bundle = make_bundle('A', 'Z', 500); // Check existing bundle
        b.iter(|| router.have_seen(black_box(&bundle.bundle_id)))
    });

    group.bench_function("mark_seen", |b| {
        let mut seq = 10000u64;
        b.iter(|| {
            seq += 1;
            let bundle = make_bundle('A', 'Z', seq);
            router.mark_seen(black_box(bundle.bundle_id))
        })
    });

    group.finish();
}

// ============================================================================
// PRoPHET Benchmarks
// ============================================================================

fn bench_prophet_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("prophet");

    let config = ProphetConfig::default();

    // Encounter recording
    group.bench_function("encounter", |b| {
        let prophet = ProphetState::new(make_id('A'), config.clone());
        b.iter(|| prophet.encounter(black_box(&make_id('B'))))
    });

    // Probability lookup (warm cache)
    group.bench_function("get_probability", |b| {
        let prophet = ProphetState::new(make_id('A'), config.clone());
        prophet.encounter(&make_id('B'));
        b.iter(|| prophet.get_probability(black_box(&make_id('B'))))
    });

    // Transitive update with many destinations
    group.bench_function("transitive_update_50_destinations", |b| {
        let prophet_a = ProphetState::new(make_id('A'), config.clone());
        prophet_a.encounter(&make_id('B'));

        // Create intermediary's probabilities
        let probs: Vec<(SimulationIdentity, f64)> = (0..50)
            .map(|i| {
                let c = (b'C' + (i % 24) as u8) as char;
                (make_id(c), 0.5 + (i as f64 * 0.01))
            })
            .collect();

        b.iter(|| prophet_a.transitive_update(black_box(&make_id('B')), black_box(&probs)))
    });

    // Aging
    group.bench_function("force_age", |b| {
        let config = ProphetConfig {
            decay_interval: Duration::from_nanos(1), // Instant aging
            ..Default::default()
        };
        let prophet = ProphetState::new(make_id('A'), config);
        for c in 'B'..='Z' {
            prophet.encounter(&make_id(c));
        }
        b.iter(|| prophet.force_age())
    });

    // Best candidate selection
    group.bench_function("best_candidate_25_candidates", |b| {
        let prophet = ProphetState::new(make_id('A'), config.clone());
        for c in 'B'..='Z' {
            prophet.encounter(&make_id(c));
        }
        let candidates: Vec<SimulationIdentity> = ('B'..='Z').map(make_id).collect();

        b.iter(|| prophet.best_candidate(black_box(&make_id('Y')), black_box(&candidates)))
    });

    group.finish();
}

// ============================================================================
// Custody Manager Benchmarks
// ============================================================================

fn bench_custody_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("custody");

    let config = CustodyConfig::default();

    // Accept custody
    group.bench_function("accept_custody", |b| {
        let custody: CustodyManager<SimulationIdentity> = CustodyManager::new(config.clone());
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            let bundle = make_bundle('A', 'Z', seq);
            custody.accept_custody(black_box(&bundle), None)
        })
    });

    // Has custody check (positive case)
    group.bench_function("has_custody_positive", |b| {
        let custody: CustodyManager<SimulationIdentity> = CustodyManager::new(config.clone());
        let bundle = make_bundle('A', 'Z', 1);
        custody.accept_custody(&bundle, None).unwrap();
        let bundle_id = bundle.bundle_id;

        b.iter(|| custody.has_custody(black_box(&bundle_id)))
    });

    // Has custody check (negative case - not in custody)
    group.bench_function("has_custody_negative", |b| {
        let custody: CustodyManager<SimulationIdentity> = CustodyManager::new(config.clone());
        let bundle = make_bundle('A', 'Z', 1);
        let bundle_id = bundle.bundle_id;

        b.iter(|| custody.has_custody(black_box(&bundle_id)))
    });

    // Custody lookup in large set
    group.bench_function("has_custody_500_bundles", |b| {
        let custody: CustodyManager<SimulationIdentity> = CustodyManager::new(CustodyConfig {
            max_custody_bundles: 1000,
            ..config.clone()
        });
        for i in 0..500 {
            let bundle = make_bundle('A', 'Z', i);
            custody.accept_custody(&bundle, None).unwrap();
        }
        let target = make_bundle('A', 'Z', 250).bundle_id;

        b.iter(|| custody.has_custody(black_box(&target)))
    });

    group.finish();
}

// ============================================================================
// Bundle Operations Benchmarks
// ============================================================================

fn bench_bundle_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("bundle");

    // Bundle creation
    group.bench_function("create_bundle", |b| {
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            make_bundle('A', 'Z', seq)
        })
    });

    // Bundle age check
    group.bench_function("check_expiration", |b| {
        let bundle = make_bundle('A', 'Z', 1);
        b.iter(|| bundle.is_expired())
    });

    // Bundle copy decrement
    group.bench_function("decrement_copies", |b| {
        b.iter(|| {
            let mut bundle = make_bundle_with_copies('A', 'Z', 1, 10);
            for _ in 0..9 {
                bundle.decrement_copies();
            }
            bundle
        })
    });

    group.finish();
}

// ============================================================================
// Age Manager Benchmarks
// ============================================================================

fn bench_age_manager(c: &mut Criterion) {
    let mut group = c.benchmark_group("age_manager");

    let config = ExpirationConfig::default();

    // Track bundle
    group.bench_function("track_bundle", |b| {
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(config.clone());
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            let bundle = make_bundle('A', 'Z', seq);
            manager.track(black_box(&bundle))
        })
    });

    // Check expiration
    group.bench_function("is_expired", |b| {
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(config.clone());
        let bundle = make_bundle('A', 'Z', 1);
        manager.track(&bundle);
        let bundle_id = bundle.bundle_id;

        b.iter(|| manager.is_expired(black_box(&bundle_id)))
    });

    // Effective priority calculation
    group.bench_function("effective_priority", |b| {
        let manager: AgeManager<SimulationIdentity> = AgeManager::new(config.clone());
        let bundle = make_bundle('A', 'Z', 1);
        manager.track(&bundle);

        b.iter(|| manager.effective_priority(black_box(&bundle)))
    });

    group.finish();
}

// ============================================================================
// Config Validation Benchmarks
// ============================================================================

fn bench_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("config");

    group.bench_function("validate_default_config", |b| {
        let config = DtnConfig::default();
        b.iter(|| config.validate())
    });

    group.bench_function("create_challenged_network_config", |b| {
        b.iter(DtnConfig::challenged_network)
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_epidemic_routing,
    bench_duplicate_detection,
    bench_prophet_operations,
    bench_custody_operations,
    bench_bundle_operations,
    bench_age_manager,
    bench_config,
);

criterion_main!(benches);
