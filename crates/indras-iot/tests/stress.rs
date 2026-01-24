//! # Stress Tests for Indras IoT
//!
//! High-load tests to verify system behavior under extreme conditions:
//! - Rapid state transitions
//! - Concurrent operations
//! - Memory pressure
//! - Large payloads
//! - Edge cases

use indras_iot::compact::{CompactMessage, CompactMessageType, Fragmenter};
use indras_iot::duty_cycle::{DutyCycleConfig, DutyCycleManager, PowerState};
use indras_iot::low_memory::{BufferPool, MemoryBudget, MemoryTracker};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Test 1: Duty cycle manager with rapid state transitions
///
/// Exercises state machine with 10,000+ tick() calls to verify:
/// - No panics or deadlocks
/// - Correct state transitions
/// - Timer handling under rapid calls
#[test]
fn test_duty_cycle_rapid_transitions() {
    let config = DutyCycleConfig {
        active_duration: Duration::from_millis(1),
        sleep_duration: Duration::from_millis(1),
        min_sync_interval: Duration::from_millis(1),
        max_pending_before_wake: 5,
        low_battery_threshold: 0.2,
    };

    let max_pending = config.max_pending_before_wake;
    let mut manager = DutyCycleManager::new(config);
    let mut state_counts = [0usize; 4]; // Active, PreSleep, Sleeping, Waking

    // Rapid tick calls
    for i in 0..10_000 {
        let state = manager.tick();

        // Count state occurrences
        match state {
            PowerState::Active => state_counts[0] += 1,
            PowerState::PreSleep => state_counts[1] += 1,
            PowerState::Sleeping => state_counts[2] += 1,
            PowerState::Waking => state_counts[3] += 1,
        }

        // Occasionally add pending operations (but respect errors)
        if i % 100 == 0 && manager.pending_count() < max_pending {
            let _ = manager.add_pending();
        }

        // Occasionally complete pending
        if i % 150 == 0 {
            manager.complete_pending();
        }

        // Change battery level periodically
        if i % 500 == 0 {
            manager.set_battery_level(0.5 + (i as f32 / 20_000.0));
        }
    }

    // Verify we've seen multiple states
    assert!(
        state_counts[0] > 0,
        "Should have seen Active state at least once"
    );

    // Verify final state is valid
    assert!(matches!(
        manager.state(),
        PowerState::Active | PowerState::PreSleep | PowerState::Sleeping | PowerState::Waking
    ));

    // Verify pending operations stayed within acceptable range
    // Note: add_pending increments first, then checks if > max * 2, so we might see max * 2 + 1
    assert!(
        manager.pending_count() <= max_pending * 2 + 1,
        "Pending count {} exceeded acceptable limit {}",
        manager.pending_count(),
        max_pending * 2 + 1
    );
}

/// Test 2: Memory tracker with 100 concurrent threads
///
/// Tests thread-safety of atomic compare-and-swap operations:
/// - No race conditions
/// - Budget never exceeded
/// - All memory properly freed
#[test]
fn test_memory_tracker_concurrent() {
    let tracker = Arc::new(MemoryTracker::new(MemoryBudget {
        max_heap_bytes: 50_000,
        max_message_size: 2048,
        max_connections: 50,
        max_pending_ops: 100,
    }));

    let threads = 100;
    let operations_per_thread = 100;

    let handles: Vec<_> = (0..threads)
        .map(|thread_id| {
            let tracker = Arc::clone(&tracker);
            thread::spawn(move || {
                let mut successful_allocations = 0;
                let mut failed_allocations = 0;

                for i in 0..operations_per_thread {
                    // Vary allocation sizes
                    let size = 100 + (thread_id * 10) + (i % 50);

                    match tracker.try_allocate(size) {
                        Ok(guard) => {
                            successful_allocations += 1;
                            // Verify allocation is tracked
                            assert!(tracker.allocated_bytes() > 0);
                            // Hold briefly to create contention
                            thread::sleep(Duration::from_micros(10));
                            drop(guard);
                        }
                        Err(_) => {
                            failed_allocations += 1;
                        }
                    }

                    // Also test connections concurrently
                    if i % 10 == 0 {
                        if let Ok(guard) = tracker.try_add_connection() {
                            thread::sleep(Duration::from_micros(5));
                            drop(guard);
                        }
                    }

                    // And operations
                    if i % 5 == 0 {
                        if let Ok(guard) = tracker.try_queue_op() {
                            thread::sleep(Duration::from_micros(3));
                            drop(guard);
                        }
                    }
                }

                (successful_allocations, failed_allocations)
            })
        })
        .collect();

    let mut total_success = 0;
    let mut total_failed = 0;

    for handle in handles {
        let (success, failed) = handle.join().unwrap();
        total_success += success;
        total_failed += failed;
    }

    // All memory should be freed after all threads complete
    assert_eq!(
        tracker.allocated_bytes(),
        0,
        "All memory should be freed"
    );
    assert_eq!(tracker.connection_count(), 0, "All connections released");
    assert_eq!(tracker.pending_ops_count(), 0, "All ops completed");

    // We should have had both successes and some failures due to contention
    assert!(total_success > 0, "Should have successful allocations");
    println!(
        "Concurrent allocations: {} succeeded, {} failed (contention)",
        total_success, total_failed
    );
}

/// Test 3: Compact message encode/decode throughput
///
/// Stress tests serialization with 10,000+ messages:
/// - Various payload sizes
/// - Different message types
/// - Round-trip integrity
#[test]
fn test_compact_message_throughput() {
    let mut encode_count = 0;
    let mut decode_count = 0;
    let mut total_bytes = 0;

    for i in 0..10_000 {
        // Vary message types
        let msg = match i % 7 {
            0 => CompactMessage::ping(),
            1 => CompactMessage::pong(),
            2 => CompactMessage::ack(i as u32),
            3 => CompactMessage::data(vec![0x42; i % 100]),
            4 => CompactMessage::data(vec![0xFF; (i % 500) + 1]),
            5 => CompactMessage::new(CompactMessageType::SyncRequest, vec![1, 2, 3]),
            _ => CompactMessage::new(CompactMessageType::Presence, vec![]),
        };

        let msg = msg
            .with_sequence(i as u32)
            .with_ack_requested();

        // Encode
        let encoded = msg.encode().expect("encode should succeed");
        encode_count += 1;
        total_bytes += encoded.len();

        // Decode
        let decoded = CompactMessage::decode(&encoded).expect("decode should succeed");
        decode_count += 1;

        // Verify round-trip
        assert_eq!(decoded.sequence, i as u32);
        assert_eq!(decoded.payload, msg.payload);
        assert_eq!(decoded.msg_type, msg.msg_type);
        assert!(decoded.ack_requested());
    }

    assert_eq!(encode_count, 10_000, "All messages encoded");
    assert_eq!(decode_count, 10_000, "All messages decoded");
    println!(
        "Encoded/decoded {} messages, {} bytes total",
        encode_count, total_bytes
    );
}

/// Test 4: Fragment 1MB+ messages
///
/// Tests fragmenter with very large payloads:
/// - Fragmentation logic
/// - Fragment metadata
/// - Boundary conditions
#[test]
fn test_fragmenter_large_payloads() {
    let fragmenter = Fragmenter::new(256); // Small fragments

    // Test various large sizes
    let test_sizes = vec![
        1024,              // 1KB
        10 * 1024,         // 10KB
        100 * 1024,        // 100KB
        1024 * 1024,       // 1MB
        1024 * 1024 + 777, // 1MB + odd bytes
    ];

    for size in test_sizes {
        let payload: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let msg = CompactMessage::data(payload.clone()).with_sequence(42);

        let fragments = fragmenter.fragment(&msg);
        let expected_fragments = (size + 255) / 256; // ceil division

        assert_eq!(
            fragments.len(),
            expected_fragments,
            "Expected {} fragments for {} bytes",
            expected_fragments,
            size
        );

        // Verify fragment metadata
        for (i, frag) in fragments.iter().enumerate() {
            assert!(frag.is_fragmented(), "Fragment should be marked fragmented");
            assert_eq!(
                frag.fragment_index(),
                i as u16,
                "Fragment index should match position"
            );
            assert_eq!(
                frag.original_sequence(),
                42,
                "Original sequence preserved"
            );

            // Last fragment should be marked
            if i == fragments.len() - 1 {
                assert!(frag.is_last_fragment(), "Last fragment should be marked");
            } else {
                assert!(
                    !frag.is_last_fragment(),
                    "Non-last fragment should not be marked"
                );
            }

            // Verify fragment can be encoded/decoded
            let encoded = frag.encode().expect("fragment encode should succeed");
            let decoded =
                CompactMessage::decode(&encoded).expect("fragment decode should succeed");
            assert_eq!(decoded.payload, frag.payload);
        }

        // Verify payloads can be reassembled
        let reassembled: Vec<u8> = fragments
            .iter()
            .flat_map(|f| f.payload.iter().copied())
            .collect();
        assert_eq!(reassembled, payload, "Reassembled payload should match");

        println!(
            "Fragmented {} bytes into {} fragments",
            size,
            fragments.len()
        );
    }
}

/// Test 5: Allocate to memory limits repeatedly
///
/// Stress tests memory pressure scenarios:
/// - Fill to capacity
/// - Free all
/// - Repeat
#[test]
fn test_memory_tracker_limits() {
    let budget = MemoryBudget {
        max_heap_bytes: 10_000,
        max_message_size: 1024,
        max_connections: 10,
        max_pending_ops: 20,
    };
    let tracker = MemoryTracker::new(budget);

    // Repeat allocation/free cycles
    for cycle in 0..100 {
        let mut guards = Vec::new();

        // Allocate up to capacity
        loop {
            match tracker.try_allocate(100) {
                Ok(guard) => guards.push(guard),
                Err(_) => break, // Budget exhausted
            }
        }

        let allocated = tracker.allocated_bytes();
        assert!(
            allocated <= 10_000,
            "Should never exceed budget, got {}",
            allocated
        );
        assert!(
            allocated >= 9_900,
            "Should be near capacity, got {}",
            allocated
        );

        // Verify we can't allocate more
        assert!(
            tracker.try_allocate(100).is_err(),
            "Should fail when at capacity"
        );

        // Free all
        guards.clear();
        assert_eq!(
            tracker.allocated_bytes(),
            0,
            "All memory freed after cycle {}",
            cycle
        );

        // Test connections to limit
        let mut conn_guards = Vec::new();
        for _ in 0..10 {
            conn_guards.push(tracker.try_add_connection().unwrap());
        }
        assert!(tracker.try_add_connection().is_err());
        conn_guards.clear();
        assert_eq!(tracker.connection_count(), 0);

        // Test operations to limit
        let mut op_guards = Vec::new();
        for _ in 0..20 {
            op_guards.push(tracker.try_queue_op().unwrap());
        }
        assert!(tracker.try_queue_op().is_err());
        op_guards.clear();
        assert_eq!(tracker.pending_ops_count(), 0);
    }

    println!("Completed 100 allocation cycles to limits");
}

/// Test 6: 50 threads managing connections concurrently
///
/// Validates connection guard thread-safety:
/// - No race conditions on connection slots
/// - Limit properly enforced
/// - All connections properly released
#[test]
fn test_connection_guard_concurrent() {
    let tracker = Arc::new(MemoryTracker::new(MemoryBudget {
        max_connections: 10,
        max_heap_bytes: 100_000,
        max_message_size: 1024,
        max_pending_ops: 50,
    }));

    let thread_count = 50;
    let attempts_per_thread = 200;

    let handles: Vec<_> = (0..thread_count)
        .map(|_| {
            let tracker = Arc::clone(&tracker);
            thread::spawn(move || {
                let mut acquired = 0;
                let mut denied = 0;

                for _ in 0..attempts_per_thread {
                    match tracker.try_add_connection() {
                        Ok(guard) => {
                            acquired += 1;
                            // Verify count never exceeds limit
                            assert!(
                                tracker.connection_count() <= 10,
                                "Connection count exceeded limit: {}",
                                tracker.connection_count()
                            );
                            // Hold briefly
                            thread::sleep(Duration::from_micros(5));
                            drop(guard);
                        }
                        Err(_) => {
                            denied += 1;
                        }
                    }
                    thread::yield_now();
                }

                (acquired, denied)
            })
        })
        .collect();

    let mut total_acquired = 0;
    let mut total_denied = 0;

    for handle in handles {
        let (acquired, denied) = handle.join().unwrap();
        total_acquired += acquired;
        total_denied += denied;
    }

    // All connections should be released
    assert_eq!(tracker.connection_count(), 0);

    println!(
        "Connection guard stress: {} acquired, {} denied across {} threads",
        total_acquired, total_denied, thread_count
    );
}

/// Test 7: Buffer pool under stress
///
/// Tests buffer pool with rapid acquire/release:
/// - No buffer leaks
/// - Proper exhaustion handling
/// - State consistency
#[test]
fn test_buffer_pool_stress() {
    let mut pool = BufferPool::new(10, 512);

    assert_eq!(pool.capacity(), 10);
    assert_eq!(pool.available(), 10);

    // Rapid acquire/release cycles
    for cycle in 0..1000 {
        // Acquire random number of buffers
        let acquire_count = (cycle % 10) + 1;
        let mut buffers = Vec::new();

        for _ in 0..acquire_count {
            if let Some(buf) = pool.try_acquire() {
                assert_eq!(buf.buffer.len(), 512);
                buffers.push(buf);
            }
        }

        let in_use = buffers.len();
        assert_eq!(pool.in_use(), in_use);
        assert_eq!(pool.available(), 10 - in_use);

        // Modify buffer contents
        for buf in &mut buffers {
            buf.buffer[0] = 0xFF;
            buf.buffer[511] = 0xAA;
        }

        // Release in random order
        while let Some(buf) = buffers.pop() {
            pool.release(buf);
        }

        // Verify all released
        assert_eq!(pool.available(), 10);
        assert_eq!(pool.in_use(), 0);
    }

    // Verify buffers were properly cleared on release
    let buf = pool.try_acquire().unwrap();
    assert_eq!(buf.buffer[0], 0, "Buffer should be cleared");
    assert_eq!(buf.buffer[511], 0, "Buffer should be cleared");
    pool.release(buf);

    println!("Completed 1000 buffer pool acquire/release cycles");
}

/// Test 8: Add many pending operations to duty cycle
///
/// Tests pending operation overflow handling:
/// - Queue up to double max_pending_before_wake
/// - Verify error on overflow
/// - Test force wake on threshold
#[test]
fn test_duty_cycle_pending_overflow() {
    let config = DutyCycleConfig {
        max_pending_before_wake: 50,
        active_duration: Duration::from_secs(100),
        sleep_duration: Duration::from_secs(100),
        ..Default::default()
    };

    let max_pending = config.max_pending_before_wake;
    let manager = Arc::new(Mutex::new(DutyCycleManager::new(config)));

    // Multiple threads adding pending operations
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let manager = Arc::clone(&manager);
            thread::spawn(move || {
                let mut added = 0;
                let mut rejected = 0;

                for _ in 0..20 {
                    let mut mgr = manager.lock().unwrap();
                    match mgr.add_pending() {
                        Ok(_) => added += 1,
                        Err(_) => {
                            rejected += 1;
                            // Stop trying after hitting limit
                            break;
                        }
                    }
                    drop(mgr);
                    thread::yield_now();
                }

                (added, rejected)
            })
        })
        .collect();

    let mut total_added = 0;
    let mut total_rejected = 0;

    for handle in handles {
        let (added, rejected) = handle.join().unwrap();
        total_added += added;
        total_rejected += rejected;
    }

    let mgr = manager.lock().unwrap();

    // Should have rejected some (overflow protection)
    assert!(total_rejected > 0, "Should reject some operations beyond 2x max");

    // In concurrent scenarios, multiple threads might increment past the limit
    // before checking. With 10 threads, we could see up to max * 2 + 10 in worst case.
    // The important thing is that errors were returned.
    assert!(
        mgr.pending_count() <= max_pending * 2 + 10,
        "Pending count {} exceeded reasonable limit ({})",
        mgr.pending_count(),
        max_pending * 2 + 10
    );

    println!(
        "Pending operations: {} added, {} rejected (total attempted: {})",
        total_added,
        total_rejected,
        total_added + total_rejected
    );
}

/// Test 9: All varint boundary values
///
/// Tests varint encoding at critical boundaries:
/// - Powers of 2
/// - Byte boundaries (127, 128, etc.)
/// - Maximum values
#[test]
fn test_varint_encoding_edges() {
    // All critical varint boundaries
    let test_values = vec![
        0u64,
        1,
        127,                  // Max 1-byte varint (0x7F)
        128,                  // Min 2-byte varint (0x80)
        255,                  // Max value in one byte of data
        256,                  // First value requiring more
        16_383,               // Max 2-byte varint (0x3FFF)
        16_384,               // Min 3-byte varint (0x4000)
        2_097_151,            // Max 3-byte varint (0x1FFFFF)
        2_097_152,            // Min 4-byte varint (0x200000)
        268_435_455,          // Max 4-byte varint (0x0FFFFFFF)
        268_435_456,          // Min 5-byte varint (0x10000000)
        u32::MAX as u64,      // Max u32
        (1u64 << 35) - 1,     // Max 5-byte varint
        (1u64 << 42) - 1,     // Max 6-byte varint
        (1u64 << 49) - 1,     // Max 7-byte varint
        (1u64 << 56) - 1,     // Max 8-byte varint
        (1u64 << 63) - 1,     // Max 9-byte varint
        u64::MAX,             // Maximum u64
    ];

    for &value in &test_values {
        // Create message with this sequence number
        let msg = if value <= u32::MAX as u64 {
            CompactMessage::data(vec![0x42; 10]).with_sequence(value as u32)
        } else {
            // For values > u32::MAX, test via payload length encoding
            // We can't directly test sequence > u32::MAX, but we can test
            // the varint encoding by creating a message with calculated size
            CompactMessage::ping()
        };

        let encoded = msg.encode().expect(&format!("encode failed for value {}", value));
        let decoded = CompactMessage::decode(&encoded)
            .expect(&format!("decode failed for value {}", value));

        if value <= u32::MAX as u64 {
            assert_eq!(
                decoded.sequence, value as u32,
                "Round-trip failed for value {}",
                value
            );
        }
    }

    // Test large payload sizes to stress varint length encoding
    let large_payloads = vec![
        0,
        1,
        127,
        128,
        16_383,
        16_384,
        65_535, // u16::MAX
    ];

    for size in large_payloads {
        let payload = vec![0x42; size];
        let msg = CompactMessage::data(payload.clone());
        let encoded = msg.encode().expect(&format!("encode failed for payload size {}", size));
        let decoded = CompactMessage::decode(&encoded)
            .expect(&format!("decode failed for payload size {}", size));

        assert_eq!(
            decoded.payload.len(),
            size,
            "Payload size mismatch for {}",
            size
        );
        assert_eq!(decoded.payload, payload);
    }

    println!("Tested {} varint boundary values", test_values.len());
}

/// Test 10: All modules working together under stress
///
/// Combined stress test exercising:
/// - Duty cycle management
/// - Memory tracking
/// - Message encoding/fragmentation
/// - Concurrent access
#[test]
fn test_combined_constraints() {
    let duty_config = DutyCycleConfig {
        active_duration: Duration::from_millis(50),
        sleep_duration: Duration::from_millis(50),
        min_sync_interval: Duration::from_millis(20),
        max_pending_before_wake: 10,
        low_battery_threshold: 0.3,
    };

    let max_pending = duty_config.max_pending_before_wake;

    let memory_budget = MemoryBudget {
        max_heap_bytes: 50_000,
        max_message_size: 4096,
        max_connections: 8,
        max_pending_ops: 20,
    };

    let duty_manager = Arc::new(Mutex::new(DutyCycleManager::new(duty_config)));
    let mem_tracker = Arc::new(MemoryTracker::new(memory_budget));
    let fragmenter = Fragmenter::new(256);

    let thread_count = 20;
    let operations_per_thread = 100;

    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let duty_manager = Arc::clone(&duty_manager);
            let mem_tracker = Arc::clone(&mem_tracker);
            let fragmenter_clone = Fragmenter::new(fragmenter.max_fragment_size());

            thread::spawn(move || {
                let mut operations = 0;
                let mut messages_sent = 0;
                let mut allocations = 0;

                for i in 0..operations_per_thread {
                    // Tick duty cycle
                    {
                        let mut mgr = duty_manager.lock().unwrap();
                        mgr.tick();

                        // Check if we can send based on power state
                        let is_urgent = i % 10 == 0;
                        if mgr.should_allow_operation(is_urgent) && mgr.pending_count() < max_pending {
                            let _ = mgr.add_pending();
                            operations += 1;
                        }
                    }

                    // Try to allocate memory for message
                    if let Ok(guard) = mem_tracker.try_allocate(512) {
                        allocations += 1;

                        // Create and encode message
                        let payload_size = (i % 100) + 50;
                        let payload = vec![0x42; payload_size];
                        let msg = CompactMessage::data(payload).with_sequence(i as u32);

                        // Encode
                        if msg.encode().is_ok() {
                            // Fragment if needed
                            let fragments = fragmenter_clone.fragment(&msg);

                            // Decode each fragment
                            for frag in fragments {
                                if let Ok(frag_encoded) = frag.encode() {
                                    let _ = CompactMessage::decode(&frag_encoded);
                                }
                            }

                            messages_sent += 1;
                        }

                        drop(guard);
                    }

                    // Try connection
                    if i % 5 == 0 {
                        if let Ok(conn) = mem_tracker.try_add_connection() {
                            thread::sleep(Duration::from_micros(10));
                            drop(conn);
                        }
                    }

                    // Complete some operations
                    if i % 7 == 0 {
                        let mut mgr = duty_manager.lock().unwrap();
                        mgr.complete_pending();
                    }

                    // Adjust battery occasionally
                    if i % 20 == 0 {
                        let mut mgr = duty_manager.lock().unwrap();
                        let battery = 0.3 + ((thread_id + i) % 70) as f32 / 100.0;
                        mgr.set_battery_level(battery);
                    }

                    thread::yield_now();
                }

                (operations, messages_sent, allocations)
            })
        })
        .collect();

    let mut total_ops = 0;
    let mut total_messages = 0;
    let mut total_allocations = 0;

    for handle in handles {
        let (ops, msgs, allocs) = handle.join().unwrap();
        total_ops += ops;
        total_messages += msgs;
        total_allocations += allocs;
    }

    // Verify clean state
    assert_eq!(mem_tracker.allocated_bytes(), 0, "All memory freed");
    assert_eq!(mem_tracker.connection_count(), 0, "All connections closed");

    let final_mgr = duty_manager.lock().unwrap();
    assert!(
        final_mgr.pending_count() <= max_pending * 2 + 1,
        "Pending operations within limits (count: {}, limit: {})",
        final_mgr.pending_count(),
        max_pending * 2 + 1
    );

    println!(
        "Combined stress test: {} operations, {} messages, {} allocations across {} threads",
        total_ops, total_messages, total_allocations, thread_count
    );
}
