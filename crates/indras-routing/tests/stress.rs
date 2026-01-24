//! Stress tests for indras-routing
//!
//! These tests verify the performance and correctness of routing components
//! under high load and concurrent access patterns.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use indras_core::{PacketId, RouteInfo, SimulationIdentity};
use indras_routing::{BackPropManager, BackPropStatus, MutualPeerTracker, RoutingTable};

// Test helpers
fn make_id(c: char) -> SimulationIdentity {
    SimulationIdentity::new(c).unwrap()
}

fn make_route(dest: char, next_hop: char, hop_count: u32) -> RouteInfo<SimulationIdentity> {
    RouteInfo::new(make_id(dest), make_id(next_hop), hop_count)
}

fn make_path(chars: &str) -> Vec<SimulationIdentity> {
    chars
        .chars()
        .map(|c| SimulationIdentity::new(c).unwrap())
        .collect()
}

#[test]
fn test_routing_table_throughput() {
    // Test inserting and retrieving 10,000+ routes
    const ROUTE_COUNT: usize = 10_000;

    let table: Arc<RoutingTable<SimulationIdentity>> =
        Arc::new(RoutingTable::new(Duration::from_secs(300)));

    let start = Instant::now();

    // Insert routes
    for i in 0..ROUTE_COUNT {
        // Create unique identities using combinations (both uppercase)
        let dest_char = (b'A' + (i % 26) as u8) as char;
        let next_hop_char = (b'A' + ((i / 26) % 26) as u8) as char;

        let dest = SimulationIdentity::new(dest_char)
            .unwrap_or_else(|| make_id('A'));
        let next_hop = SimulationIdentity::new(next_hop_char)
            .unwrap_or_else(|| make_id('B'));

        let mut route = RouteInfo::new(dest.clone(), next_hop, (i % 100) as u32);
        route.metric = i as u32;

        table.insert(&dest, route);
    }

    let insert_duration = start.elapsed();
    println!("Inserted {} routes in {:?}", ROUTE_COUNT, insert_duration);

    // Verify all routes can be retrieved
    let start = Instant::now();
    let mut found = 0;

    for i in 0..ROUTE_COUNT {
        let dest_char = (b'A' + (i % 26) as u8) as char;
        let dest = SimulationIdentity::new(dest_char)
            .unwrap_or_else(|| make_id('A'));

        if table.get(&dest).is_some() {
            found += 1;
        }
    }

    let get_duration = start.elapsed();
    println!("Retrieved {} routes in {:?}", found, get_duration);

    // With hash collisions, we expect fewer unique routes
    assert!(found > 0, "Should have found at least some routes");
    assert!(table.len() > 0, "Table should not be empty");

    // Test metric-based sorting
    let start = Instant::now();
    let sorted = table.routes_by_metric();
    let sort_duration = start.elapsed();

    println!("Sorted {} routes by metric in {:?}", sorted.len(), sort_duration);

    // Verify sorting is correct
    for i in 1..sorted.len().min(100) {
        assert!(
            sorted[i - 1].metric <= sorted[i].metric,
            "Routes should be sorted by metric"
        );
    }

    // Performance assertions (generous bounds)
    assert!(insert_duration < Duration::from_secs(5), "Insert should be fast");
    assert!(get_duration < Duration::from_secs(2), "Get should be fast");
    assert!(sort_duration < Duration::from_secs(10), "Sort should be fast");
}

#[test]
fn test_backprop_manager_scale() {
    // Track 1000+ pending backprops
    const BACKPROP_COUNT: usize = 1_000;

    let manager: Arc<BackPropManager<SimulationIdentity>> =
        Arc::new(BackPropManager::new(Duration::from_secs(60)));

    let start = Instant::now();

    // Start many backprops
    for i in 0..BACKPROP_COUNT {
        let packet_id = PacketId::new((i as u64) << 32, i as u64);
        let path = make_path("ABCDEFGH"); // 8-hop path
        manager.start_backprop(packet_id, path);
    }

    let start_duration = start.elapsed();
    println!(
        "Started {} backprops in {:?}",
        BACKPROP_COUNT, start_duration
    );

    assert_eq!(
        manager.pending_count(),
        BACKPROP_COUNT,
        "All backprops should be pending"
    );

    // Advance half of them one hop
    // Path is ABCDEFGH (indices 0-7), starts at hop 7
    // next_confirmer at hop 7 is path[6] = 'G'
    let start = Instant::now();
    let g = make_id('G');

    for i in 0..BACKPROP_COUNT / 2 {
        let packet_id = PacketId::new((i as u64) << 32, i as u64);
        let status = manager.advance(&packet_id, &g);
        assert_eq!(
            status,
            BackPropStatus::InProgress(6),
            "Should advance to hop 6"
        );
    }

    let advance_duration = start.elapsed();
    println!(
        "Advanced {} backprops in {:?}",
        BACKPROP_COUNT / 2,
        advance_duration
    );

    // Check status of all
    let start = Instant::now();
    for i in 0..BACKPROP_COUNT {
        let packet_id = PacketId::new((i as u64) << 32, i as u64);
        let status = manager.status(&packet_id);

        if i < BACKPROP_COUNT / 2 {
            assert_eq!(
                status,
                BackPropStatus::InProgress(6),
                "Should be at hop 6"
            );
        } else {
            assert_eq!(
                status,
                BackPropStatus::InProgress(7),
                "Should still be at hop 7"
            );
        }
    }

    let status_duration = start.elapsed();
    println!(
        "Checked status of {} backprops in {:?}",
        BACKPROP_COUNT, status_duration
    );

    // Performance assertions
    assert!(
        start_duration < Duration::from_secs(2),
        "Starting backprops should be fast"
    );
    assert!(
        advance_duration < Duration::from_secs(2),
        "Advancing backprops should be fast"
    );
    assert!(
        status_duration < Duration::from_secs(2),
        "Checking status should be fast"
    );
}

#[test]
fn test_mutual_peer_tracker_load() {
    // Test tracking many peer connections
    use indras_core::NetworkTopology;
    use std::collections::{HashMap, HashSet};
    use std::sync::RwLock;

    // Simple test topology
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

    // Create a mesh topology with many connections
    let mut topology = TestTopology::new();
    const PEER_COUNT: usize = 100;

    let peers: Vec<SimulationIdentity> = (0..PEER_COUNT)
        .map(|i| {
            let c = (b'A' + (i % 26) as u8) as char;
            SimulationIdentity::new(c).unwrap_or_else(|| make_id('A'))
        })
        .collect();

    // Create partial mesh - each peer connects to next 5 peers
    for i in 0..peers.len() {
        for j in 1..=5 {
            let next_idx = (i + j) % peers.len();
            if i != next_idx {
                topology.add_connection(peers[i].clone(), peers[next_idx].clone());
            }
        }
    }

    let tracker: Arc<MutualPeerTracker<SimulationIdentity>> =
        Arc::new(MutualPeerTracker::new());

    // Track all connections
    let start = Instant::now();
    let mut connection_count = 0;

    for i in 0..peers.len() {
        for j in (i + 1)..peers.len() {
            tracker.on_connect(&peers[i], &peers[j], &topology);
            connection_count += 1;
        }
    }

    let connect_duration = start.elapsed();
    println!(
        "Tracked {} peer connections in {:?}",
        connection_count, connect_duration
    );

    // Query relays for many pairs
    let start = Instant::now();
    let mut relay_count = 0;

    for i in 0..peers.len().min(50) {
        for j in 0..peers.len().min(50) {
            if i != j {
                let relays = tracker.get_relays_for(&peers[i], &peers[j]);
                relay_count += relays.len();
            }
        }
    }

    let query_duration = start.elapsed();
    println!(
        "Queried relay candidates (found {}) in {:?}",
        relay_count, query_duration
    );

    assert!(tracker.len() > 0, "Should have cached connections");
    assert!(
        connect_duration < Duration::from_secs(30),
        "Connection tracking should be reasonably fast"
    );
}

#[test]
fn test_concurrent_route_updates() {
    // Test concurrent multi-threaded route changes
    let table: Arc<RoutingTable<SimulationIdentity>> =
        Arc::new(RoutingTable::new(Duration::from_secs(300)));

    const THREAD_COUNT: usize = 10;
    const UPDATES_PER_THREAD: usize = 1_000;

    let start = Instant::now();
    let mut handles = vec![];

    // Spawn threads that concurrently insert/update routes
    for thread_id in 0..THREAD_COUNT {
        let table_clone = Arc::clone(&table);

        let handle = thread::spawn(move || {
            for i in 0..UPDATES_PER_THREAD {
                let dest_char = (b'A' + ((thread_id + i) % 26) as u8) as char;
                let next_hop_char = (b'A' + ((i / 10) % 26) as u8) as char;

                let dest = SimulationIdentity::new(dest_char)
                    .unwrap_or_else(|| make_id('A'));
                let next_hop = SimulationIdentity::new(next_hop_char)
                    .unwrap_or_else(|| make_id('B'));

                let mut route = RouteInfo::new(dest.clone(), next_hop, (i % 50) as u32);
                route.metric = (thread_id * 1000 + i) as u32;

                table_clone.insert(&dest, route);

                // Occasionally update metrics
                if i % 10 == 0 {
                    table_clone.update_metric(&dest, (i * 2) as u32);
                }

                // Occasionally confirm routes
                if i % 15 == 0 {
                    table_clone.confirm(&dest);
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete successfully");
    }

    let duration = start.elapsed();
    println!(
        "Completed {} concurrent route updates in {:?}",
        THREAD_COUNT * UPDATES_PER_THREAD,
        duration
    );

    // Verify table state
    assert!(table.len() > 0, "Table should have routes");
    println!("Final table size: {}", table.len());

    // Verify we can still query routes
    let routes = table.routes_by_metric();
    assert!(!routes.is_empty(), "Should be able to query routes");

    assert!(
        duration < Duration::from_secs(10),
        "Concurrent updates should complete in reasonable time"
    );
}

#[test]
fn test_backprop_timeout_stress() {
    // Test many concurrent timeouts
    const TIMEOUT_COUNT: usize = 500;

    let manager: Arc<BackPropManager<SimulationIdentity>> =
        Arc::new(BackPropManager::new(Duration::from_millis(50)));

    // Start backprops with short timeout
    for i in 0..TIMEOUT_COUNT {
        let packet_id = PacketId::new((i as u64) << 32, i as u64);
        let path = make_path("ABCDE");
        manager.start_backprop(packet_id, path);
    }

    assert_eq!(manager.pending_count(), TIMEOUT_COUNT);

    // Wait for all to timeout
    thread::sleep(Duration::from_millis(100));

    let start = Instant::now();
    let timed_out = manager.check_timeouts();
    let check_duration = start.elapsed();

    println!(
        "Checked {} backprops for timeout ({} timed out) in {:?}",
        TIMEOUT_COUNT,
        timed_out.len(),
        check_duration
    );

    assert_eq!(
        timed_out.len(),
        TIMEOUT_COUNT,
        "All backprops should have timed out"
    );

    // Verify advancing timed-out backprops returns TimedOut
    let start = Instant::now();
    for packet_id in &timed_out {
        let status = manager.advance(packet_id, &make_id('E'));
        assert_eq!(
            status,
            BackPropStatus::TimedOut,
            "Should return TimedOut status"
        );
    }
    let advance_duration = start.elapsed();

    println!(
        "Advanced {} timed-out backprops in {:?}",
        timed_out.len(),
        advance_duration
    );

    // All should be removed after advancing
    assert_eq!(
        manager.pending_count(),
        0,
        "All timed-out backprops should be removed"
    );

    assert!(
        check_duration < Duration::from_secs(1),
        "Timeout checking should be fast"
    );
    assert!(
        advance_duration < Duration::from_secs(1),
        "Advancing timeouts should be fast"
    );
}

#[test]
fn test_routing_table_staleness() {
    // Prune stale routes repeatedly under load
    let table: Arc<RoutingTable<SimulationIdentity>> =
        Arc::new(RoutingTable::new(Duration::from_millis(10)));

    const CYCLES: usize = 100;
    const ROUTES_PER_CYCLE: usize = 50;

    let start = Instant::now();

    for cycle in 0..CYCLES {
        // Insert routes
        for i in 0..ROUTES_PER_CYCLE {
            let dest_char = (b'A' + ((cycle + i) % 26) as u8) as char;
            let dest = SimulationIdentity::new(dest_char)
                .unwrap_or_else(|| make_id('A'));
            let route = make_route(dest_char, 'X', i as u32);
            table.insert(&dest, route);
        }

        // Wait a bit for some routes to become stale
        thread::sleep(Duration::from_millis(5));

        // Prune stale routes
        table.prune_stale();

        // Check staleness of a few routes
        for i in 0..5 {
            let dest_char = (b'A' + ((cycle + i) % 26) as u8) as char;
            let dest = SimulationIdentity::new(dest_char)
                .unwrap_or_else(|| make_id('A'));
            let _ = table.is_stale(&dest);
        }
    }

    let duration = start.elapsed();
    println!(
        "Completed {} insert/prune cycles in {:?}",
        CYCLES, duration
    );

    // Final prune
    thread::sleep(Duration::from_millis(20));
    table.prune_stale();

    // Most routes should be pruned
    println!("Final table size: {}", table.len());

    assert!(
        duration < Duration::from_secs(15),
        "Staleness management should be efficient"
    );
}

#[test]
fn test_route_metric_updates() {
    // Update metrics frequently for many routes
    let table: Arc<RoutingTable<SimulationIdentity>> =
        Arc::new(RoutingTable::new(Duration::from_secs(300)));

    const ROUTE_COUNT: usize = 100;
    const UPDATE_CYCLES: usize = 1_000;

    // Insert initial routes
    for i in 0..ROUTE_COUNT {
        let dest_char = (b'A' + (i % 26) as u8) as char;
        let dest = SimulationIdentity::new(dest_char)
            .unwrap_or_else(|| make_id('A'));
        let route = make_route(dest_char, 'X', i as u32);
        table.insert(&dest, route);
    }

    let start = Instant::now();

    // Rapidly update metrics
    for cycle in 0..UPDATE_CYCLES {
        let route_idx = cycle % ROUTE_COUNT;
        let dest_char = (b'A' + (route_idx % 26) as u8) as char;
        let dest = SimulationIdentity::new(dest_char)
            .unwrap_or_else(|| make_id('A'));

        table.update_metric(&dest, cycle as u32);

        // Occasionally re-sort
        if cycle % 100 == 0 {
            let _ = table.routes_by_metric();
        }
    }

    let duration = start.elapsed();
    println!(
        "Completed {} metric updates in {:?}",
        UPDATE_CYCLES, duration
    );

    // Verify final state
    let sorted = table.routes_by_metric();
    assert!(!sorted.is_empty(), "Should have routes");

    // Verify sorting
    for i in 1..sorted.len() {
        assert!(
            sorted[i - 1].metric <= sorted[i].metric,
            "Routes should be sorted"
        );
    }

    assert!(
        duration < Duration::from_secs(5),
        "Metric updates should be fast"
    );
}

#[test]
fn test_deep_relay_chains() {
    // Test long relay paths with many hops
    const PATH_LENGTH: usize = 50;
    const CHAIN_COUNT: usize = 100;

    let manager: Arc<BackPropManager<SimulationIdentity>> =
        Arc::new(BackPropManager::new(Duration::from_secs(60)));

    // Create deep paths
    let deep_path: Vec<SimulationIdentity> = (0..PATH_LENGTH)
        .map(|i| {
            let c = (b'A' + (i % 26) as u8) as char;
            SimulationIdentity::new(c).unwrap_or_else(|| make_id('A'))
        })
        .collect();

    let start = Instant::now();

    // Start backprops with deep paths
    for i in 0..CHAIN_COUNT {
        let packet_id = PacketId::new((i as u64) << 32, i as u64);
        manager.start_backprop(packet_id, deep_path.clone());
    }

    let start_duration = start.elapsed();
    println!(
        "Started {} backprops with {}-hop paths in {:?}",
        CHAIN_COUNT, PATH_LENGTH, start_duration
    );

    // Advance first chain completely
    let start = Instant::now();
    let packet_id = PacketId::new(0, 0);

    for hop_idx in (1..PATH_LENGTH).rev() {
        let peer = &deep_path[hop_idx - 1];
        let status = manager.advance(&packet_id, peer);

        if hop_idx > 1 {
            assert!(
                matches!(status, BackPropStatus::InProgress(_)),
                "Should be in progress"
            );
        } else {
            assert_eq!(status, BackPropStatus::Complete, "Should complete");
        }
    }

    let complete_duration = start.elapsed();
    println!(
        "Completed full {}-hop backprop in {:?}",
        PATH_LENGTH, complete_duration
    );

    // Verify state
    assert_eq!(
        manager.status(&packet_id),
        BackPropStatus::NotFound,
        "Completed backprop should be removed"
    );

    assert_eq!(
        manager.pending_count(),
        CHAIN_COUNT - 1,
        "Should have one fewer pending"
    );

    // Test partial advancement of remaining chains
    let start = Instant::now();
    for i in 1..CHAIN_COUNT.min(50) {
        let packet_id = PacketId::new((i as u64) << 32, i as u64);

        // Advance 10 hops
        for hop_idx in (PATH_LENGTH - 10..PATH_LENGTH).rev() {
            let peer = &deep_path[hop_idx - 1];
            manager.advance(&packet_id, peer);
        }
    }

    let partial_duration = start.elapsed();
    println!(
        "Partially advanced {} deep chains in {:?}",
        CHAIN_COUNT.min(50) - 1,
        partial_duration
    );

    assert!(
        start_duration < Duration::from_secs(2),
        "Starting deep backprops should be fast"
    );
    assert!(
        complete_duration < Duration::from_secs(1),
        "Completing deep backprop should be fast"
    );
}

#[test]
fn test_mixed_workload_stress() {
    // Combined stress test with routing table, backprop, and concurrent access
    let table: Arc<RoutingTable<SimulationIdentity>> =
        Arc::new(RoutingTable::new(Duration::from_secs(60)));
    let manager: Arc<BackPropManager<SimulationIdentity>> =
        Arc::new(BackPropManager::new(Duration::from_secs(60)));

    const OPERATIONS: usize = 5_000;

    let start = Instant::now();
    let mut handles = vec![];

    // Thread 1: Insert routes
    {
        let table = Arc::clone(&table);
        handles.push(thread::spawn(move || {
            for i in 0..OPERATIONS {
                let dest_char = (b'A' + (i % 26) as u8) as char;
                let dest = SimulationIdentity::new(dest_char)
                    .unwrap_or_else(|| make_id('A'));
                let route = make_route(dest_char, 'X', i as u32);
                table.insert(&dest, route);
            }
        }));
    }

    // Thread 2: Update metrics
    {
        let table = Arc::clone(&table);
        handles.push(thread::spawn(move || {
            thread::sleep(Duration::from_millis(10)); // Let some routes insert first
            for i in 0..OPERATIONS {
                let dest_char = (b'A' + (i % 26) as u8) as char;
                let dest = SimulationIdentity::new(dest_char)
                    .unwrap_or_else(|| make_id('A'));
                table.update_metric(&dest, (i * 2) as u32);
            }
        }));
    }

    // Thread 3: Start backprops
    {
        let manager = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            for i in 0..OPERATIONS {
                let packet_id = PacketId::new((i as u64) << 32, i as u64);
                let path = make_path("ABCDEF");
                manager.start_backprop(packet_id, path);
            }
        }));
    }

    // Thread 4: Advance backprops
    {
        let manager = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            thread::sleep(Duration::from_millis(20)); // Let some backprops start
            let f = make_id('F');
            for i in 0..OPERATIONS / 2 {
                let packet_id = PacketId::new((i as u64) << 32, i as u64);
                manager.advance(&packet_id, &f);
            }
        }));
    }

    // Wait for all operations
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    let duration = start.elapsed();
    println!(
        "Completed mixed workload ({} ops) in {:?}",
        OPERATIONS * 4,
        duration
    );

    println!("Final routing table size: {}", table.len());
    println!("Final pending backprops: {}", manager.pending_count());

    assert!(
        duration < Duration::from_secs(15),
        "Mixed workload should complete in reasonable time"
    );
}
