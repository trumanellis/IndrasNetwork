//! Stress tests for indras-logging correlation and peer context
//!
//! These tests focus on high-volume scenarios to verify that CorrelationContext
//! and PeerContextGuard handle extreme loads without issues.

use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use indras_core::SimulationIdentity;
use indras_logging::correlation::CorrelationContext;
use indras_logging::context::PeerContextGuard;

/// Test creating a long chain of child contexts (1000+ deep)
#[test]
fn test_correlation_context_chain() {
    let start = Instant::now();
    let mut ctx = CorrelationContext::new_root();

    // Create 1000 child contexts in a chain
    for i in 0..1000 {
        ctx = ctx.child();

        // Verify hop count increases
        assert_eq!(ctx.hop_count, i + 1);

        // Verify trace_id remains constant
        if i == 0 {
            assert_eq!(ctx.trace_id, ctx.trace_id);
        }
    }

    // Final verification
    assert_eq!(ctx.hop_count, 1000);

    let elapsed = start.elapsed();
    println!("Created 1000 child contexts in {:?}", elapsed);
}

/// Test concurrent peer context creation and switching across multiple threads
#[test]
fn test_concurrent_peer_contexts() {
    const NUM_THREADS: usize = 50;
    const ITERATIONS: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let mut handles = vec![];

    let start = Instant::now();

    for thread_id in 0..NUM_THREADS {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            // Use different peer identities across threads
            let peer_char = (b'A' + (thread_id % 26) as u8) as char;
            let peer = SimulationIdentity::new(peer_char).unwrap();

            // Wait for all threads to be ready
            barrier.wait();

            // Create and drop guards repeatedly
            for _ in 0..ITERATIONS {
                let _guard = PeerContextGuard::new(&peer);

                // Verify context is set correctly
                let ctx = PeerContextGuard::current().unwrap();
                assert_eq!(ctx.peer_id, peer_char.to_string());

                // Guard drops here, context should be restored
            }

            // After all guards drop, context should be None
            assert!(PeerContextGuard::current().is_none());
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    println!(
        "Completed {} peer context operations across {} threads in {:?}",
        NUM_THREADS * ITERATIONS,
        NUM_THREADS,
        elapsed
    );
}

/// Test very deep correlation chain (100+ levels)
#[test]
fn test_deep_correlation_chain() {
    let start = Instant::now();

    // Create root with packet ID
    let root = CorrelationContext::new_root().with_packet_id("packet_0001");
    let original_trace_id = root.trace_id;

    // Create 100 levels of nested children
    let mut contexts = vec![root];
    for level in 1..=100 {
        let parent = &contexts[level - 1];
        let child = parent.child();

        // Verify parent relationship
        assert_eq!(child.parent_span_id, Some(parent.span_id));
        assert_eq!(child.trace_id, original_trace_id);
        assert_eq!(child.hop_count, level as u32);
        assert_eq!(child.packet_id, Some("packet_0001".to_string()));

        contexts.push(child);
    }

    let elapsed = start.elapsed();
    println!("Created 100-level deep context chain in {:?}", elapsed);

    // Verify the final context
    let final_ctx = &contexts[100];
    assert_eq!(final_ctx.hop_count, 100);
    assert_eq!(final_ctx.trace_id, original_trace_id);
}

/// Test rapid context switching (10,000+ switches)
#[test]
fn test_rapid_context_switching() {
    const NUM_SWITCHES: usize = 10_000;

    let peer_a = SimulationIdentity::new('A').unwrap();
    let peer_b = SimulationIdentity::new('B').unwrap();

    let start = Instant::now();

    for i in 0..NUM_SWITCHES {
        let peer = if i % 2 == 0 { &peer_a } else { &peer_b };
        let _guard = PeerContextGuard::new(peer);

        // Verify context is correct
        let ctx = PeerContextGuard::current().unwrap();
        let expected_id = if i % 2 == 0 { "A" } else { "B" };
        assert_eq!(ctx.peer_id, expected_id);

        // Guard drops, context restored
    }

    let elapsed = start.elapsed();
    println!("Completed {} context switches in {:?}", NUM_SWITCHES, elapsed);

    // Ensure no context remains
    assert!(PeerContextGuard::current().is_none());
}

/// Test traceparent parsing and formatting with many iterations
#[test]
fn test_traceparent_roundtrip() {
    const NUM_ROUNDTRIPS: usize = 1000;

    let start = Instant::now();

    for _ in 0..NUM_ROUNDTRIPS {
        // Create context
        let ctx = CorrelationContext::new_root();
        let original_trace_id = ctx.trace_id;

        // Convert to traceparent format
        let traceparent = ctx.to_traceparent();

        // Verify format
        assert!(traceparent.starts_with("00-"));
        assert_eq!(traceparent.split('-').count(), 4);

        // Parse back
        let parsed = CorrelationContext::from_traceparent(&traceparent);
        assert!(parsed.is_some());

        let parsed = parsed.unwrap();
        assert_eq!(parsed.trace_id, original_trace_id);
    }

    let elapsed = start.elapsed();
    println!("Completed {} traceparent roundtrips in {:?}", NUM_ROUNDTRIPS, elapsed);
}

/// Test correlation contexts with many packet IDs
#[test]
fn test_correlation_with_packet_id() {
    const NUM_PACKETS: usize = 5000;

    let start = Instant::now();

    for i in 0..NUM_PACKETS {
        let packet_id = format!("{:04x}#{}", i, i % 10);
        let ctx = CorrelationContext::new_root().with_packet_id(&packet_id);

        // Verify packet ID is set
        assert_eq!(ctx.packet_id, Some(packet_id.clone()));

        // Create child and verify packet ID is preserved
        let child = ctx.child();
        assert_eq!(child.packet_id, Some(packet_id));
        assert_eq!(child.hop_count, 1);
    }

    let elapsed = start.elapsed();
    println!("Created {} contexts with packet IDs in {:?}", NUM_PACKETS, elapsed);
}

/// Test concurrent correlation context creation across threads
#[test]
fn test_concurrent_correlation_contexts() {
    const NUM_THREADS: usize = 50;
    const CONTEXTS_PER_THREAD: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let mut handles = vec![];

    let start = Instant::now();

    for thread_id in 0..NUM_THREADS {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait();

            let mut trace_ids = Vec::new();

            // Create many root contexts
            for i in 0..CONTEXTS_PER_THREAD {
                let ctx = CorrelationContext::new_root()
                    .with_packet_id(format!("t{}_p{}", thread_id, i));

                // Verify each has a unique trace ID
                assert!(!trace_ids.contains(&ctx.trace_id));
                trace_ids.push(ctx.trace_id);

                // Create a child
                let child = ctx.child();
                assert_eq!(child.trace_id, ctx.trace_id);
                assert_eq!(child.hop_count, 1);
                assert_eq!(child.parent_span_id, Some(ctx.span_id));
            }

            trace_ids
        });

        handles.push(handle);
    }

    // Collect all trace IDs from all threads
    let mut all_trace_ids = Vec::new();
    for handle in handles {
        let trace_ids = handle.join().unwrap();
        all_trace_ids.extend(trace_ids);
    }

    let elapsed = start.elapsed();

    // Verify all trace IDs are unique across all threads
    let unique_count = all_trace_ids.len();
    all_trace_ids.sort();
    all_trace_ids.dedup();
    assert_eq!(all_trace_ids.len(), unique_count, "Found duplicate trace IDs");

    println!(
        "Created {} unique correlation contexts across {} threads in {:?}",
        NUM_THREADS * CONTEXTS_PER_THREAD,
        NUM_THREADS,
        elapsed
    );
}

/// Test hop count scaling with large chains
#[test]
fn test_hop_count_scaling() {
    const CHAIN_LENGTH: u32 = 5000;

    let start = Instant::now();

    let mut ctx = CorrelationContext::new_root();
    let root_trace_id = ctx.trace_id;

    // Build a very long chain
    for expected_hop in 1..=CHAIN_LENGTH {
        ctx = ctx.child();
        assert_eq!(ctx.hop_count, expected_hop);
        assert_eq!(ctx.trace_id, root_trace_id);
    }

    let elapsed = start.elapsed();

    // Final verification
    assert_eq!(ctx.hop_count, CHAIN_LENGTH);
    assert_eq!(ctx.trace_id, root_trace_id);

    println!("Built chain of {} hops in {:?}", CHAIN_LENGTH, elapsed);
}

/// Test nested peer contexts with many levels
#[test]
fn test_nested_peer_contexts() {
    const NESTING_DEPTH: usize = 100;

    let start = Instant::now();

    // Create a stack of guards
    let mut guards = Vec::new();

    for i in 0..NESTING_DEPTH {
        let peer_char = (b'A' + (i % 26) as u8) as char;
        let peer = SimulationIdentity::new(peer_char).unwrap();
        let guard = PeerContextGuard::new(&peer);

        // Verify current context matches the most recent guard
        let ctx = PeerContextGuard::current().unwrap();
        assert_eq!(ctx.peer_id, peer_char.to_string());

        guards.push(guard);
    }

    let elapsed = start.elapsed();
    println!("Created {} nested peer contexts in {:?}", NESTING_DEPTH, elapsed);

    // Drop guards in reverse order and verify context restoration
    for i in (0..NESTING_DEPTH).rev() {
        guards.pop();

        if i > 0 {
            // Context should restore to previous
            let expected_char = (b'A' + ((i - 1) % 26) as u8) as char;
            let ctx = PeerContextGuard::current().unwrap();
            assert_eq!(ctx.peer_id, expected_char.to_string());
        } else {
            // All guards dropped, no context
            assert!(PeerContextGuard::current().is_none());
        }
    }
}

/// Test memory usage by creating many correlation contexts
#[test]
fn test_correlation_memory_stress() {
    const NUM_CONTEXTS: usize = 10_000;

    let start = Instant::now();

    // Create many contexts and store them (to test memory)
    let contexts: Vec<_> = (0..NUM_CONTEXTS)
        .map(|i| {
            CorrelationContext::new_root()
                .with_packet_id(format!("pkt_{:06}", i))
        })
        .collect();

    let elapsed = start.elapsed();

    // Verify all contexts are valid
    assert_eq!(contexts.len(), NUM_CONTEXTS);

    // Check a sample
    assert_eq!(contexts[0].packet_id, Some("pkt_000000".to_string()));
    assert_eq!(contexts[9999].packet_id, Some("pkt_009999".to_string()));

    println!("Created and stored {} contexts in {:?}", NUM_CONTEXTS, elapsed);
}

/// Test serialization roundtrip under stress
#[test]
fn test_serialization_stress() {
    const NUM_ROUNDTRIPS: usize = 1000;

    let start = Instant::now();

    for i in 0..NUM_ROUNDTRIPS {
        let ctx = CorrelationContext::new_root()
            .with_packet_id(format!("pkt_{}", i));

        // Serialize to JSON
        let json = serde_json::to_string(&ctx).unwrap();

        // Deserialize back
        let deserialized: CorrelationContext = serde_json::from_str(&json).unwrap();

        // Verify equality
        assert_eq!(deserialized.trace_id, ctx.trace_id);
        assert_eq!(deserialized.span_id, ctx.span_id);
        assert_eq!(deserialized.packet_id, ctx.packet_id);
        assert_eq!(deserialized.hop_count, ctx.hop_count);
    }

    let elapsed = start.elapsed();
    println!("Completed {} serialization roundtrips in {:?}", NUM_ROUNDTRIPS, elapsed);
}

/// Test mixed operations: concurrent peer contexts and correlation chains
#[test]
fn test_mixed_concurrent_operations() {
    const NUM_THREADS: usize = 20;
    const OPERATIONS_PER_THREAD: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let mut handles = vec![];

    let start = Instant::now();

    for thread_id in 0..NUM_THREADS {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            let peer_char = (b'A' + (thread_id % 26) as u8) as char;
            let peer = SimulationIdentity::new(peer_char).unwrap();

            barrier.wait();

            for i in 0..OPERATIONS_PER_THREAD {
                // Set peer context
                let _guard = PeerContextGuard::new(&peer);

                // Create correlation chain
                let mut ctx = CorrelationContext::new_root()
                    .with_packet_id(format!("t{}_op{}", thread_id, i));

                // Build a small chain
                for hop in 0..10 {
                    ctx = ctx.child();
                    assert_eq!(ctx.hop_count, hop + 1);
                }

                // Verify peer context is still correct
                let peer_ctx = PeerContextGuard::current().unwrap();
                assert_eq!(peer_ctx.peer_id, peer_char.to_string());
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    println!(
        "Completed {} mixed operations across {} threads in {:?}",
        NUM_THREADS * OPERATIONS_PER_THREAD,
        NUM_THREADS,
        elapsed
    );
}

/// Test traceparent format consistency with many contexts
#[test]
fn test_traceparent_format_consistency() {
    const NUM_TESTS: usize = 1000;

    let start = Instant::now();

    for _ in 0..NUM_TESTS {
        let ctx = CorrelationContext::new_root();
        let traceparent = ctx.to_traceparent();

        // Verify format constraints
        assert!(traceparent.starts_with("00-"), "Must start with version 00");
        assert!(traceparent.ends_with("-01"), "Must end with flags 01");

        let parts: Vec<&str> = traceparent.split('-').collect();
        assert_eq!(parts.len(), 4, "Must have 4 parts");
        assert_eq!(parts[0], "00", "Version must be 00");
        assert_eq!(parts[1].len(), 32, "Trace ID must be 32 hex chars");
        assert_eq!(parts[2].len(), 16, "Span ID must be 16 hex chars");
        assert_eq!(parts[3], "01", "Flags must be 01");

        // Verify all chars are valid hex
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
        assert!(parts[2].chars().all(|c| c.is_ascii_hexdigit()));
    }

    let elapsed = start.elapsed();
    println!("Validated {} traceparent formats in {:?}", NUM_TESTS, elapsed);
}
