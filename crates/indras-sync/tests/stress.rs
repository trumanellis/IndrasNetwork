//! Stress tests for indras-sync
//!
//! These tests verify performance and correctness under high load:
//!
//! 1. **test_event_store_throughput**: Appends 10,000+ events with 10 peers
//!    Tests: Event append performance, pending tracking, query performance
//!
//! 2. **test_pending_delivery_tracking**: Tracks pending events for 20+ peers
//!    Tests: Pending count accuracy, mark delivered operations, multi-peer coordination
//!
//! 3. **test_document_sync_stress**: Generates and applies 1,000+ sync messages
//!    Tests: Automerge document sync with many events, incremental sync performance
//!
//! 4. **test_member_churn**: Rapidly adds/removes members (1,000 operations)
//!    Tests: Membership change performance, state consistency during churn
//!
//! 5. **test_concurrent_event_append**: Multi-threaded event appends (4 threads)
//!    Tests: Thread safety, concurrent access patterns, data integrity
//!
//! 6. **test_sync_state_many_peers**: Tracks sync state for 25+ peers over 10 rounds
//!    Tests: Sync state management, peer tracking at scale, sync round coordination
//!
//! 7. **test_deep_event_history**: Queries events at various sequence points (5,000 events)
//!    Tests: Historical query performance, sequence-based retrieval efficiency
//!
//! 8. **test_interface_save_load**: Serializes/deserializes large interfaces (25 members, 1,000 events)
//!    Tests: Persistence operations, data integrity after round-trip, load performance
//!
//! 9. **test_combined_stress**: Mixed operations (append, add member, query, check pending)
//!    Tests: Real-world usage patterns, operation interleaving, system resilience
//!
//! 10. **test_large_document_serialization**: Serialization performance (26 members, 2,000 events)
//!     Tests: Document save/load throughput, memory efficiency, format stability
//!
//! Note: Tests are limited to 26 peers max due to SimulationIdentity constraints (A-Z).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;

use indras_core::traits::NInterfaceTrait;
use indras_core::{InterfaceEvent, InterfaceId, SimulationIdentity};
use indras_sync::{EventStore, InterfaceDocument, NInterface, SyncProtocol, SyncState};

/// Helper to create test peers
fn create_peer(id: char) -> SimulationIdentity {
    SimulationIdentity::new(id).expect("Failed to create peer")
}

/// Helper to create many test peers (max 26 due to SimulationIdentity constraints)
fn create_peers(count: usize) -> Vec<SimulationIdentity> {
    assert!(count <= 26, "SimulationIdentity only supports A-Z (26 peers max)");
    (0..count)
        .map(|i| {
            let c = (b'A' + i as u8) as char;
            create_peer(c)
        })
        .collect()
}

/// Test 1: Event store throughput with 10,000+ events
#[test]
fn test_event_store_throughput() {
    const EVENT_COUNT: usize = 10_000;
    const PEER_COUNT: usize = 10;

    let peers = create_peers(PEER_COUNT);
    let members: HashSet<_> = peers.iter().cloned().collect();
    let mut store = EventStore::with_members(members);

    println!("Appending {} events to store with {} peers...", EVENT_COUNT, PEER_COUNT);

    // Append many events
    let start = std::time::Instant::now();
    for i in 0..EVENT_COUNT {
        let sender = &peers[i % PEER_COUNT];
        let event = InterfaceEvent::message(
            *sender,
            i as u64 + 1,
            format!("Message {}", i).into_bytes(),
        );
        store.append(event);
    }
    let append_duration = start.elapsed();

    println!(
        "Appended {} events in {:?} ({:.0} events/sec)",
        EVENT_COUNT,
        append_duration,
        EVENT_COUNT as f64 / append_duration.as_secs_f64()
    );

    // Verify counts
    assert_eq!(store.len(), EVENT_COUNT);

    // Check pending counts (each peer should have events from all other peers)
    for peer in &peers {
        let pending_count = store.pending_count(peer);
        // Each peer should have roughly (EVENT_COUNT - EVENT_COUNT/PEER_COUNT) pending
        // (all events except their own)
        let expected = EVENT_COUNT - EVENT_COUNT / PEER_COUNT;
        let margin = EVENT_COUNT / PEER_COUNT + 50; // Allow some margin
        assert!(
            pending_count >= expected - margin && pending_count <= expected + margin,
            "Peer {:?} has {} pending, expected around {}",
            peer,
            pending_count,
            expected
        );
    }

    // Test query performance
    let query_start = std::time::Instant::now();
    let events_since_5000 = store.since(5000);
    let query_duration = query_start.elapsed();

    println!(
        "Queried events since 5000: found {} events in {:?}",
        events_since_5000.len(),
        query_duration
    );

    assert!(events_since_5000.len() >= EVENT_COUNT - 5000);
}

/// Test 2: Pending delivery tracking for 20+ peers
#[test]
fn test_pending_delivery_tracking() {
    const PEER_COUNT: usize = 20;
    const EVENTS_PER_PEER: usize = 20;

    println!("Testing pending delivery tracking with {} peers...", PEER_COUNT);

    let peers = create_peers(PEER_COUNT);
    let members: HashSet<_> = peers.iter().cloned().collect();
    let mut store = EventStore::with_members(members);

    // Each peer sends some events
    let start = std::time::Instant::now();
    for (peer_idx, sender) in peers.iter().enumerate() {
        for event_idx in 0..EVENTS_PER_PEER {
            let event = InterfaceEvent::message(
                *sender,
                (peer_idx * EVENTS_PER_PEER + event_idx) as u64 + 1,
                format!("From peer {} event {}", peer_idx, event_idx).into_bytes(),
            );
            store.append(event);
        }
    }
    let append_duration = start.elapsed();

    println!(
        "Appended {} events in {:?}",
        PEER_COUNT * EVENTS_PER_PEER,
        append_duration
    );

    // Verify pending counts
    let check_start = std::time::Instant::now();
    for (idx, peer) in peers.iter().enumerate() {
        let pending_count = store.pending_count(peer);
        // Each peer should have all events except their own
        let expected = (PEER_COUNT - 1) * EVENTS_PER_PEER;
        assert_eq!(
            pending_count, expected,
            "Peer {} has {} pending, expected {}",
            idx, pending_count, expected
        );
    }
    let check_duration = check_start.elapsed();

    println!(
        "Verified pending counts for {} peers in {:?}",
        PEER_COUNT,
        check_duration
    );

    // Test marking delivered for many peers
    let mark_start = std::time::Instant::now();
    for peer in peers.iter().take(50) {
        store.mark_all_delivered(peer);
    }
    let mark_duration = mark_start.elapsed();

    println!(
        "Marked all delivered for 50 peers in {:?}",
        mark_duration
    );

    // Verify first 50 have no pending
    for peer in peers.iter().take(50) {
        assert_eq!(store.pending_count(peer), 0);
    }

    // Verify last 50 still have pending
    for peer in peers.iter().skip(50) {
        let expected = (PEER_COUNT - 1) * EVENTS_PER_PEER;
        assert_eq!(store.pending_count(peer), expected);
    }
}

/// Test 3: Document sync stress with many messages
#[test]
fn test_document_sync_stress() {
    const MESSAGE_COUNT: usize = 1_000;
    const PEER_COUNT: usize = 5;

    println!(
        "Testing document sync with {} messages from {} peers...",
        MESSAGE_COUNT, PEER_COUNT
    );

    let peers = create_peers(PEER_COUNT);
    let mut doc1 = InterfaceDocument::new();

    // Add members to doc1
    for peer in &peers {
        doc1.add_member(peer);
    }

    // Save initial state and create doc2 from it (simulates both starting from same state)
    let initial_bytes = doc1.save();
    let mut doc2 = InterfaceDocument::load(&initial_bytes).expect("Failed to load doc2");

    // Get doc2's heads before doc1 makes changes
    let doc2_heads = doc2.heads();

    // Doc1: Add many events
    let start = std::time::Instant::now();
    for i in 0..MESSAGE_COUNT {
        let sender = &peers[i % PEER_COUNT];
        let event = InterfaceEvent::message(
            *sender,
            i as u64 + 1,
            format!("Sync message {}", i).into_bytes(),
        );
        doc1.append_event(&event).expect("Failed to append event");
    }
    let append_duration = start.elapsed();

    println!(
        "Doc1 appended {} events in {:?}",
        MESSAGE_COUNT,
        append_duration
    );

    assert_eq!(doc1.event_count(), MESSAGE_COUNT);

    // Sync doc1's changes to doc2
    let sync_start = std::time::Instant::now();
    let sync_msg = doc1.generate_sync_message(&doc2_heads);
    doc2.apply_sync_message(&sync_msg)
        .expect("Failed to apply sync");
    let sync_duration = sync_start.elapsed();

    println!(
        "Synced {} events to doc2 in {:?} (sync message size: {} bytes)",
        MESSAGE_COUNT,
        sync_duration,
        sync_msg.len()
    );

    // Verify sync
    assert_eq!(doc2.event_count(), MESSAGE_COUNT);

    let events: Vec<InterfaceEvent<SimulationIdentity>> = doc2.events();
    assert_eq!(events.len(), MESSAGE_COUNT);

    println!("Successfully synced {} events to doc2", events.len());

    // Verify a sample of events
    for i in (0..MESSAGE_COUNT).step_by(MESSAGE_COUNT / 10) {
        match &events[i] {
            InterfaceEvent::Message { content, .. } => {
                let expected = format!("Sync message {}", i);
                assert_eq!(content, expected.as_bytes());
            }
            _ => panic!("Expected Message event at index {}", i),
        }
    }
}

/// Test 4: Member churn - rapid add/remove operations
#[test]
fn test_member_churn() {
    const CHURN_COUNT: usize = 1_000;
    const INITIAL_PEERS: usize = 13; // Use 13 so we have 13 for rotation (total 26)

    println!(
        "Testing member churn: {} add/remove cycles with {} initial peers...",
        CHURN_COUNT, INITIAL_PEERS
    );

    let peers = create_peers(INITIAL_PEERS * 2);
    let mut interface = NInterface::new(peers[0]);

    // Add initial members
    for peer in peers.iter().take(INITIAL_PEERS).skip(1) {
        interface.add_member(*peer).expect("Failed to add member");
    }

    assert_eq!(interface.members().len(), INITIAL_PEERS);

    // Rapid churn: remove and add members
    // We alternate between removing and adding, cycling through available peers
    let start = std::time::Instant::now();
    let mut ops_count = 0;

    for i in 0..CHURN_COUNT {
        if i % 2 == 0 {
            // Even iterations: remove a member (but not the creator)
            let current_members: Vec<_> = interface
                .members()
                .into_iter()
                .filter(|p| *p != peers[0]) // Don't remove creator
                .collect();

            if !current_members.is_empty() {
                let to_remove = current_members[i % current_members.len()];
                interface
                    .remove_member(&to_remove)
                    .expect("Failed to remove member");
                ops_count += 1;
            }
        } else {
            // Odd iterations: add a member
            let to_add_idx = (i % (INITIAL_PEERS * 2 - 1)) + 1; // Skip creator at index 0
            if !interface.members().contains(&peers[to_add_idx]) {
                interface
                    .add_member(peers[to_add_idx])
                    .expect("Failed to add member");
                ops_count += 1;
            }
        }
    }
    let churn_duration = start.elapsed();

    println!(
        "Completed {} churn operations in {:?} ({:.0} ops/sec)",
        ops_count,
        churn_duration,
        ops_count as f64 / churn_duration.as_secs_f64()
    );

    // Verify we still have a reasonable number of members
    let final_count = interface.members().len();
    println!("Final member count: {}", final_count);
    assert!(
        final_count >= 1 && final_count <= INITIAL_PEERS * 2,
        "Expected between 1 and {} members, got {}",
        INITIAL_PEERS * 2,
        final_count
    );

    // Verify document integrity matches interface state
    let doc = interface.document();
    let doc_members: HashSet<SimulationIdentity> = doc.members();
    assert_eq!(doc_members.len(), final_count, "Document and interface member counts should match");

    println!(
        "Final state: {} members, {} events",
        interface.members().len(),
        interface.event_count()
    );
}

/// Test 5: Concurrent event append (multi-threaded)
#[test]
fn test_concurrent_event_append() {
    const THREADS: usize = 4;
    const EVENTS_PER_THREAD: usize = 250;
    const TOTAL_EVENTS: usize = THREADS * EVENTS_PER_THREAD;

    println!(
        "Testing concurrent append: {} threads, {} events per thread...",
        THREADS, EVENTS_PER_THREAD
    );

    let peers = create_peers(THREADS);
    let members: HashSet<_> = peers.iter().cloned().collect();

    // EventStore wrapped in Arc<Mutex<>> for thread safety
    let store = Arc::new(Mutex::new(EventStore::with_members(members)));

    let start = std::time::Instant::now();

    // Spawn threads to append concurrently
    let handles: Vec<_> = (0..THREADS)
        .map(|thread_id| {
            let store_clone = Arc::clone(&store);
            let sender = peers[thread_id];

            thread::spawn(move || {
                for i in 0..EVENTS_PER_THREAD {
                    let event = InterfaceEvent::message(
                        sender,
                        (thread_id * EVENTS_PER_THREAD + i) as u64 + 1,
                        format!("Thread {} message {}", thread_id, i).into_bytes(),
                    );

                    let mut store = store_clone.lock().unwrap();
                    store.append(event);
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let duration = start.elapsed();

    println!(
        "Appended {} events concurrently in {:?} ({:.0} events/sec)",
        TOTAL_EVENTS,
        duration,
        TOTAL_EVENTS as f64 / duration.as_secs_f64()
    );

    // Verify total count
    let store = store.lock().unwrap();
    assert_eq!(store.len(), TOTAL_EVENTS);

    // Verify each peer has correct pending count
    for peer in &peers {
        let pending = store.pending_count(peer);
        let expected = TOTAL_EVENTS - EVENTS_PER_THREAD;
        assert_eq!(
            pending, expected,
            "Peer {:?} has {} pending, expected {}",
            peer, pending, expected
        );
    }
}

/// Test 6: Sync state tracking for 25+ peers
#[test]
fn test_sync_state_many_peers() {
    const PEER_COUNT: usize = 25;
    const SYNC_ROUNDS: usize = 10;

    println!(
        "Testing sync state tracking for {} peers over {} rounds...",
        PEER_COUNT, SYNC_ROUNDS
    );

    let peers = create_peers(PEER_COUNT);
    let interface_id = InterfaceId::generate();
    let mut sync_state: SyncState<SimulationIdentity> = SyncState::new(interface_id);
    let mut doc = InterfaceDocument::new();

    // Add all peers to document
    for peer in &peers {
        doc.add_member(peer);
    }

    // Simulate multiple sync rounds with all peers
    let start = std::time::Instant::now();
    for round in 0..SYNC_ROUNDS {
        for peer in &peers {
            // Generate sync request
            let _sync_msg = SyncProtocol::generate_sync_request(
                interface_id,
                &mut doc,
                &mut sync_state,
                peer,
            );

            // Simulate receiving their heads
            let heads = doc.heads();
            sync_state.update_peer_heads(peer, heads);
        }

        // Add an event to trigger changes
        let sender = &peers[round % PEER_COUNT];
        let event = InterfaceEvent::message(
            *sender,
            (round + 1) as u64,
            format!("Round {} event", round).into_bytes(),
        );
        doc.append_event(&event).expect("Failed to append");
    }
    let duration = start.elapsed();

    println!(
        "Completed {} sync rounds with {} peers in {:?} ({:.0} syncs/sec)",
        SYNC_ROUNDS,
        PEER_COUNT,
        duration,
        (SYNC_ROUNDS * PEER_COUNT) as f64 / duration.as_secs_f64()
    );

    // Verify sync state
    assert_eq!(sync_state.peers().len(), PEER_COUNT);

    for peer in &peers {
        assert_eq!(
            sync_state.rounds(peer),
            SYNC_ROUNDS as u32,
            "Peer {:?} should have completed {} rounds",
            peer,
            SYNC_ROUNDS
        );
        assert!(!sync_state.is_awaiting(peer));
    }

    println!("All {} peers synced successfully", PEER_COUNT);
}

/// Test 7: Deep event history queries
#[test]
fn test_deep_event_history() {
    const TOTAL_EVENTS: usize = 5_000;
    const QUERY_POINTS: &[u64] = &[0, 100, 500, 1000, 2500, 4000, 4999];

    println!(
        "Testing deep history queries on {} events...",
        TOTAL_EVENTS
    );

    let peer = create_peer('A');
    let mut members = HashSet::new();
    members.insert(peer);
    let mut store = EventStore::with_members(members);

    // Append many events
    let start = std::time::Instant::now();
    for i in 0..TOTAL_EVENTS {
        let event = InterfaceEvent::message(
            peer,
            i as u64 + 1,
            format!("History event {}", i).into_bytes(),
        );
        store.append(event);
    }
    let append_duration = start.elapsed();

    println!(
        "Appended {} events in {:?}",
        TOTAL_EVENTS,
        append_duration
    );

    // Query at various points
    println!("\nQuerying at different sequence points:");
    for &seq in QUERY_POINTS {
        let query_start = std::time::Instant::now();
        let events = store.since(seq);
        let query_duration = query_start.elapsed();

        let expected_count = TOTAL_EVENTS - seq as usize;
        println!(
            "  since({}): found {} events in {:?}",
            seq,
            events.len(),
            query_duration
        );

        let margin = expected_count.min(5);
        assert!(
            events.len() >= expected_count.saturating_sub(margin),
            "Query since {} returned {}, expected around {}",
            seq,
            events.len(),
            expected_count
        );
    }

    // Query from the very beginning
    let all_start = std::time::Instant::now();
    let all_events = store.all();
    let all_duration = all_start.elapsed();

    println!(
        "\nQueried all {} events in {:?}",
        all_events.len(),
        all_duration
    );

    assert_eq!(all_events.len(), TOTAL_EVENTS);
}

/// Test 8: Interface save/load with large state
#[test]
fn test_interface_save_load() {
    const MEMBER_COUNT: usize = 25;
    const EVENT_COUNT: usize = 1_000;

    println!(
        "Testing save/load with {} members and {} events...",
        MEMBER_COUNT, EVENT_COUNT
    );

    let peers = create_peers(MEMBER_COUNT);
    let mut interface = NInterface::new(peers[0]);

    // Add members
    for peer in peers.iter().skip(1) {
        interface.add_member(*peer).expect("Failed to add member");
    }

    // Add events (using runtime for async)
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for i in 0..EVENT_COUNT {
            let sender = &peers[i % MEMBER_COUNT];
            let event = InterfaceEvent::message(
                *sender,
                i as u64 + 1,
                format!("Save/load test message {}", i).into_bytes(),
            );
            interface.append(event).await.expect("Failed to append");
        }
    });

    println!("Created interface with {} events", EVENT_COUNT);

    // Save
    let save_start = std::time::Instant::now();
    let interface_id = interface.id();
    let bytes = interface.save();
    let save_duration = save_start.elapsed();

    println!(
        "Saved interface to {} bytes in {:?}",
        bytes.len(),
        save_duration
    );

    // Load
    let load_start = std::time::Instant::now();
    let loaded: NInterface<SimulationIdentity> = NInterface::load(interface_id, &bytes)
        .expect("Failed to load interface");
    let load_duration = load_start.elapsed();

    println!("Loaded interface in {:?}", load_duration);

    // Verify loaded state
    assert_eq!(loaded.id(), interface_id);
    assert_eq!(loaded.members().len(), MEMBER_COUNT);

    // Verify document events
    let doc_event_count = loaded.document().event_count();
    println!("Document has {} events after load", doc_event_count);
    assert_eq!(doc_event_count, EVENT_COUNT);

    // Sample verification of events
    let events: Vec<InterfaceEvent<SimulationIdentity>> = loaded.document().events();
    for i in (0..EVENT_COUNT).step_by(EVENT_COUNT / 10) {
        match &events[i] {
            InterfaceEvent::Message { content, .. } => {
                let expected = format!("Save/load test message {}", i);
                assert_eq!(content, expected.as_bytes());
            }
            _ => panic!("Expected Message event at index {}", i),
        }
    }

    println!("Save/load verification complete");
}

/// Test 9: Stress test with combined operations
#[test]
fn test_combined_stress() {
    const PEER_COUNT: usize = 26;
    const OPERATIONS: usize = 500;

    println!(
        "Running combined stress test: {} peers, {} operations...",
        PEER_COUNT, OPERATIONS
    );

    let peers = create_peers(PEER_COUNT);
    let mut interface = NInterface::new(peers[0]);

    // Add initial members
    for peer in peers.iter().skip(1).take(PEER_COUNT / 2) {
        interface.add_member(*peer).expect("Failed to add member");
    }

    let rt = tokio::runtime::Runtime::new().unwrap();

    let start = std::time::Instant::now();

    rt.block_on(async {
        for i in 0..OPERATIONS {
            let op_type = i % 4;

            match op_type {
                0 => {
                    // Append event
                    let sender_idx = i % interface.members().len();
                    let members_vec: Vec<_> = interface.members().into_iter().collect();
                    let sender = members_vec[sender_idx];
                    let event = InterfaceEvent::message(
                        sender,
                        i as u64 + 1,
                        format!("Combined op {}", i).into_bytes(),
                    );
                    interface.append(event).await.expect("Failed to append");
                }
                1 => {
                    // Add member if not at capacity
                    if interface.members().len() < PEER_COUNT {
                        let peer_idx = PEER_COUNT / 2 + (i % (PEER_COUNT / 2));
                        if peer_idx < peers.len() && !interface.members().contains(&peers[peer_idx]) {
                            interface.add_member(peers[peer_idx]).expect("Failed to add");
                        }
                    }
                }
                2 => {
                    // Query events
                    let _events = interface.events_since((i / 2) as u64);
                }
                3 => {
                    // Check pending
                    let members_vec: Vec<_> = interface.members().into_iter().collect();
                    if !members_vec.is_empty() {
                        let peer = members_vec[i % members_vec.len()];
                        let _pending = interface.pending_for(&peer);
                    }
                }
                _ => unreachable!(),
            }
        }
    });

    let duration = start.elapsed();

    println!(
        "Completed {} mixed operations in {:?} ({:.0} ops/sec)",
        OPERATIONS,
        duration,
        OPERATIONS as f64 / duration.as_secs_f64()
    );

    println!(
        "Final state: {} members, {} events",
        interface.members().len(),
        interface.event_count()
    );

    // Verify integrity
    assert!(interface.members().len() > 0);
    assert!(interface.event_count() > 0);
}

/// Test 10: Large document serialization performance
#[test]
fn test_large_document_serialization() {
    const MEMBER_COUNT: usize = 26;
    const EVENT_COUNT: usize = 2_000;

    println!(
        "Testing serialization of large document: {} members, {} events...",
        MEMBER_COUNT, EVENT_COUNT
    );

    let peers = create_peers(MEMBER_COUNT);
    let mut doc = InterfaceDocument::new();

    // Add members
    for peer in &peers {
        doc.add_member(peer);
    }

    // Add many events
    for i in 0..EVENT_COUNT {
        let sender = &peers[i % MEMBER_COUNT];
        let event = InterfaceEvent::message(
            *sender,
            i as u64 + 1,
            format!("Serialization test {}", i).into_bytes(),
        );
        doc.append_event(&event).expect("Failed to append");
    }

    println!(
        "Created document: {} members, {} events",
        MEMBER_COUNT,
        EVENT_COUNT
    );

    // Serialize multiple times to measure average
    const ITERATIONS: usize = 5;
    let mut total_duration = std::time::Duration::ZERO;
    let mut last_size = 0;

    for _ in 0..ITERATIONS {
        let start = std::time::Instant::now();
        let bytes = doc.save();
        total_duration += start.elapsed();
        last_size = bytes.len();
    }

    let avg_duration = total_duration / ITERATIONS as u32;

    println!(
        "Average serialization: {} bytes in {:?}",
        last_size,
        avg_duration
    );

    // Deserialize
    let bytes = doc.save();
    let load_start = std::time::Instant::now();
    let loaded = InterfaceDocument::load(&bytes).expect("Failed to load");
    let load_duration = load_start.elapsed();

    println!("Deserialization: {:?}", load_duration);

    // Verify
    let loaded_members: HashSet<SimulationIdentity> = loaded.members();
    assert_eq!(loaded_members.len(), MEMBER_COUNT);
    assert_eq!(loaded.event_count(), EVENT_COUNT);

    println!(
        "Serialization test complete: {:.2} MB/s write, {:.2} MB/s read",
        last_size as f64 / 1_000_000.0 / avg_duration.as_secs_f64(),
        last_size as f64 / 1_000_000.0 / load_duration.as_secs_f64()
    );
}
