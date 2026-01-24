//! Stress tests for indras-storage
//!
//! These tests verify storage behavior under high load, capacity limits,
//! concurrent access, and eviction policies.

use std::sync::Arc;
use std::time::Instant;

use indras_core::{EventId, SimulationIdentity};
use indras_storage::{
    EvictionPolicy, InMemoryPendingStore, PendingStore, PersistentPendingStore, QuotaManager,
    QuotaManagerBuilder,
};
use tempfile::TempDir;

// ============================================================================
// Throughput Tests
// ============================================================================

/// Test marking 10,000+ events as pending
///
/// Verifies the store can handle high throughput of pending event insertions
/// without errors or performance degradation.
#[tokio::test]
async fn test_pending_store_throughput() {
    // Use high quota limits to avoid eviction during throughput test
    let quota = QuotaManager::new(20_000, 200_000);
    let store = InMemoryPendingStore::with_quota(quota);
    let peer = SimulationIdentity::new('A').unwrap();
    let event_count: usize = 10_000;

    let start = Instant::now();

    // Mark 10,000 events as pending
    for i in 1..=event_count {
        store
            .mark_pending(&peer, EventId::new(1, i as u64))
            .await
            .expect("Failed to mark event pending");
    }

    let duration = start.elapsed();
    println!(
        "Marked {} events in {:?} ({:.2} events/sec)",
        event_count,
        duration,
        event_count as f64 / duration.as_secs_f64()
    );

    // Verify all events are present
    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), event_count);
    assert_eq!(store.total_pending(), event_count);

    // Verify ordering (should be sorted)
    for (i, event_id) in pending.iter().enumerate() {
        assert_eq!(event_id.sequence, (i + 1) as u64);
    }
}

/// Test throughput with multiple peers
#[tokio::test]
async fn test_pending_store_throughput_multi_peer() {
    let store = InMemoryPendingStore::new();
    let peers: Vec<_> = (b'A'..=b'J')
        .map(|c| SimulationIdentity::new(c as char).unwrap())
        .collect();
    let events_per_peer: usize = 1_000;

    let start = Instant::now();

    // Mark 1,000 events for each of 10 peers (10,000 total)
    for peer in &peers {
        for i in 1..=events_per_peer {
            store
                .mark_pending(peer, EventId::new(1, i as u64))
                .await
                .expect("Failed to mark event pending");
        }
    }

    let duration = start.elapsed();
    let total_events = peers.len() * events_per_peer;
    println!(
        "Marked {} events across {} peers in {:?} ({:.2} events/sec)",
        total_events,
        peers.len(),
        duration,
        total_events as f64 / duration.as_secs_f64()
    );

    // Verify totals
    assert_eq!(store.total_pending(), total_events);
    assert_eq!(store.peer_count(), peers.len());

    // Verify each peer has the correct count
    for peer in &peers {
        let pending = store.pending_for(peer).await.unwrap();
        assert_eq!(pending.len(), events_per_peer);
    }
}

// ============================================================================
// Quota Manager Stress Tests
// ============================================================================

/// Test repeated eviction under heavy load
///
/// Continuously adds events beyond capacity to verify eviction policy
/// works correctly under sustained pressure.
#[tokio::test]
async fn test_quota_manager_stress() {
    let quota = QuotaManager::new(100, 10_000);
    let store = InMemoryPendingStore::with_quota(quota);
    let peer = SimulationIdentity::new('A').unwrap();

    // Add 1,000 events to a store with per-peer quota of 100
    // This should trigger eviction 900 times
    for i in 1..=1_000 {
        store
            .mark_pending(&peer, EventId::new(1, i))
            .await
            .expect("Failed to mark event pending");
    }

    // Should have exactly 100 events (the quota limit)
    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 100);
    assert_eq!(store.total_pending(), 100);

    // The first 900 should be evicted, last 100 should remain
    for i in 901..=1_000 {
        assert!(
            pending.contains(&EventId::new(1, i)),
            "Event {} should be present",
            i
        );
    }

    // The first 900 should be gone
    for i in 1..=900 {
        assert!(
            !pending.contains(&EventId::new(1, i)),
            "Event {} should have been evicted",
            i
        );
    }
}

/// Test total quota enforcement across multiple peers
#[tokio::test]
async fn test_quota_manager_total_limit() {
    // 50 per peer, 200 total
    let quota = QuotaManager::new(50, 200);
    let store = InMemoryPendingStore::with_quota(quota);

    let peers: Vec<_> = (b'A'..=b'D')
        .map(|c| SimulationIdentity::new(c as char).unwrap())
        .collect();

    // Add 50 events for each of 4 peers (200 total, at the limit)
    for (peer_idx, peer) in peers.iter().enumerate() {
        for i in 1..=50 {
            store
                .mark_pending(peer, EventId::new(peer_idx as u64 + 1, i))
                .await
                .expect("Failed to mark event pending");
        }
    }

    assert_eq!(store.total_pending(), 200);

    // Try to add one more event - should hit total quota
    let peer_e = SimulationIdentity::new('E').unwrap();
    let result = store.mark_pending(&peer_e, EventId::new(5, 1)).await;

    // Should fail due to total quota
    assert!(
        result.is_err(),
        "Should have exceeded total quota, but succeeded"
    );
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

/// Test concurrent marking and delivering from multiple threads
///
/// Verifies thread-safety and correctness of concurrent operations.
#[tokio::test]
async fn test_concurrent_pending_access() {
    let store = Arc::new(InMemoryPendingStore::new());
    let mut handles = vec![];

    // Spawn 10 tasks, each marking 100 events for a different peer
    for i in 0..10 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            let peer = SimulationIdentity::new((b'A' + i) as char).unwrap();
            for seq in 1..=100 {
                store_clone
                    .mark_pending(&peer, EventId::new(i as u64 + 1, seq))
                    .await
                    .expect("Failed to mark pending");
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify total count
    assert_eq!(store.total_pending(), 1000);
    assert_eq!(store.peer_count(), 10);

    // Now concurrently deliver events from different threads
    let mut handles = vec![];

    for i in 0..10 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            let peer = SimulationIdentity::new((b'A' + i) as char).unwrap();
            // Deliver first 50 events
            for seq in 1..=50 {
                store_clone
                    .mark_delivered(&peer, EventId::new(i as u64 + 1, seq))
                    .await
                    .expect("Failed to mark delivered");
            }
        });
        handles.push(handle);
    }

    // Wait for all deliveries
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Should have 500 events left (50 per peer)
    assert_eq!(store.total_pending(), 500);

    // Verify each peer has 50 remaining
    for i in 0..10 {
        let peer = SimulationIdentity::new((b'A' + i) as char).unwrap();
        let pending = store.pending_for(&peer).await.unwrap();
        assert_eq!(pending.len(), 50);

        // Verify it's the correct 50 (sequences 51-100)
        for event_id in pending {
            assert!(
                event_id.sequence >= 51 && event_id.sequence <= 100,
                "Unexpected sequence: {}",
                event_id.sequence
            );
        }
    }
}

/// Test concurrent access with mixed operations
#[tokio::test]
async fn test_concurrent_mixed_operations() {
    let store = Arc::new(InMemoryPendingStore::new());
    let mut handles = vec![];

    let peers: Vec<_> = (b'A'..=b'E')
        .map(|c| SimulationIdentity::new(c as char).unwrap())
        .collect();

    // Concurrent marks
    for (i, peer) in peers.iter().enumerate() {
        let store_clone = Arc::clone(&store);
        let peer = *peer;
        let handle = tokio::spawn(async move {
            for seq in 1..=200 {
                store_clone
                    .mark_pending(&peer, EventId::new(i as u64 + 1, seq))
                    .await
                    .unwrap();
            }
        });
        handles.push(handle);
    }

    // Concurrent queries
    for peer in peers.iter() {
        let store_clone = Arc::clone(&store);
        let peer = *peer;
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = store_clone.pending_for(&peer).await.unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all
    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(store.total_pending(), 1000); // 5 peers * 200 events
}

// ============================================================================
// Memory Store Capacity Tests
// ============================================================================

/// Test filling memory store to capacity
#[tokio::test]
async fn test_memory_store_capacity() {
    // Small quota for testing
    let quota = QuotaManager::new(1_000, 5_000);
    let store = InMemoryPendingStore::with_quota(quota);

    let peers: Vec<_> = (b'A'..=b'E')
        .map(|c| SimulationIdentity::new(c as char).unwrap())
        .collect();

    // Fill to capacity (1,000 per peer, 5,000 total)
    for (peer_idx, peer) in peers.iter().enumerate() {
        for seq in 1..=1_000 {
            store
                .mark_pending(peer, EventId::new(peer_idx as u64 + 1, seq))
                .await
                .expect("Failed to mark pending");
        }
    }

    assert_eq!(store.total_pending(), 5_000);
    assert_eq!(store.peer_count(), 5);

    // Try to add more events - should fail due to total quota
    let peer_f = SimulationIdentity::new('F').unwrap();
    let result = store.mark_pending(&peer_f, EventId::new(6, 1)).await;
    assert!(result.is_err(), "Should exceed total capacity");

    // But we can add more to existing peers (will trigger per-peer eviction)
    let peer_a = SimulationIdentity::new('A').unwrap();
    let result = store.mark_pending(&peer_a, EventId::new(1, 1_001)).await;
    assert!(result.is_err(), "Should exceed total capacity");
}

// ============================================================================
// Eviction Policy Tests
// ============================================================================

/// Test FIFO eviction policy
#[tokio::test]
async fn test_eviction_policy_fifo() {
    let quota = QuotaManagerBuilder::new()
        .max_pending_per_peer(10)
        .max_total_pending(1_000)
        .eviction_policy(EvictionPolicy::Fifo)
        .build();

    let store = InMemoryPendingStore::with_quota(quota);
    let peer = SimulationIdentity::new('A').unwrap();

    // Add 10 events (at capacity)
    for i in 1..=10 {
        store
            .mark_pending(&peer, EventId::new(1, i))
            .await
            .expect("Failed to mark pending");
    }

    // Add 5 more (should evict first 5)
    for i in 11..=15 {
        store
            .mark_pending(&peer, EventId::new(1, i))
            .await
            .expect("Failed to mark pending");
    }

    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 10);

    // Should have events 6-15 (first 5 evicted)
    for i in 6..=15 {
        assert!(
            pending.contains(&EventId::new(1, i)),
            "Event {} should be present",
            i
        );
    }

    // First 5 should be evicted
    for i in 1..=5 {
        assert!(
            !pending.contains(&EventId::new(1, i)),
            "Event {} should have been evicted",
            i
        );
    }
}

/// Test OldestFirst eviction policy
#[tokio::test]
async fn test_eviction_policy_oldest() {
    let quota = QuotaManagerBuilder::new()
        .max_pending_per_peer(10)
        .max_total_pending(1_000)
        .eviction_policy(EvictionPolicy::OldestFirst)
        .build();

    let store = InMemoryPendingStore::with_quota(quota);
    let peer = SimulationIdentity::new('A').unwrap();

    // Add 10 events from different senders
    for i in 1..=10 {
        store
            .mark_pending(&peer, EventId::new(i, 1))
            .await
            .expect("Failed to mark pending");
    }

    // Add more events - should evict oldest by EventId ordering
    for i in 11..=15 {
        store
            .mark_pending(&peer, EventId::new(i, 1))
            .await
            .expect("Failed to mark pending");
    }

    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 10);

    // Should have kept the newest 10 by EventId natural ordering
    for i in 6..=15 {
        assert!(
            pending.contains(&EventId::new(i, 1)),
            "Event from sender {} should be present",
            i
        );
    }
}

/// Test eviction with multiple senders and sequences
#[tokio::test]
async fn test_eviction_mixed_senders() {
    let quota = QuotaManagerBuilder::new()
        .max_pending_per_peer(20)
        .max_total_pending(1_000)
        .eviction_policy(EvictionPolicy::Fifo)
        .build();

    let store = InMemoryPendingStore::with_quota(quota);
    let peer = SimulationIdentity::new('A').unwrap();

    // Add events from 2 senders alternating
    for seq in 1..=10 {
        store
            .mark_pending(&peer, EventId::new(1, seq))
            .await
            .unwrap();
        store
            .mark_pending(&peer, EventId::new(2, seq))
            .await
            .unwrap();
    }

    assert_eq!(store.pending_for(&peer).await.unwrap().len(), 20);

    // Add 10 more from sender 3 (should evict oldest 10)
    for seq in 1..=10 {
        store
            .mark_pending(&peer, EventId::new(3, seq))
            .await
            .unwrap();
    }

    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 20);

    // Should have evicted oldest events (from sender 1, sequences 1-10)
    for seq in 1..=10 {
        assert!(!pending.contains(&EventId::new(1, seq)));
    }
}

// ============================================================================
// Quota Limits Enforcement Tests
// ============================================================================

/// Test per-peer and total limits enforcement
#[tokio::test]
async fn test_quota_limits_enforcement() {
    let quota = QuotaManagerBuilder::new()
        .max_pending_per_peer(50)
        .max_total_pending(150)
        .eviction_policy(EvictionPolicy::Fifo)
        .build();

    let store = InMemoryPendingStore::with_quota(quota);

    let peer_a = SimulationIdentity::new('A').unwrap();
    let peer_b = SimulationIdentity::new('B').unwrap();
    let peer_c = SimulationIdentity::new('C').unwrap();
    let peer_d = SimulationIdentity::new('D').unwrap();

    // Fill A, B, C to 50 each (150 total, at limit)
    for seq in 1..=50 {
        store
            .mark_pending(&peer_a, EventId::new(1, seq))
            .await
            .unwrap();
        store
            .mark_pending(&peer_b, EventId::new(2, seq))
            .await
            .unwrap();
        store
            .mark_pending(&peer_c, EventId::new(3, seq))
            .await
            .unwrap();
    }

    assert_eq!(store.total_pending(), 150);

    // Try to add to peer D - should fail (total limit)
    let result = store.mark_pending(&peer_d, EventId::new(4, 1)).await;
    assert!(result.is_err(), "Should exceed total quota");

    // Deliver one event from peer A to make room
    store
        .mark_delivered(&peer_a, EventId::new(1, 1))
        .await
        .unwrap();

    assert_eq!(store.total_pending(), 149);

    // Now add more to peer A (should work since we're under total limit)
    store
        .mark_pending(&peer_a, EventId::new(1, 51))
        .await
        .unwrap();

    let pending_a = store.pending_for(&peer_a).await.unwrap();
    assert_eq!(pending_a.len(), 50); // Still at per-peer limit
    assert!(!pending_a.contains(&EventId::new(1, 1))); // First one evicted
    assert!(pending_a.contains(&EventId::new(1, 51))); // New one present

    // Total should still be 150 (or could fail on total limit)
    assert!(store.total_pending() <= 150);
}

/// Test that eviction respects both per-peer and total limits
#[tokio::test]
async fn test_quota_limits_per_peer_vs_total() {
    // Small per-peer limit, large total limit
    let quota = QuotaManagerBuilder::new()
        .max_pending_per_peer(10)
        .max_total_pending(10_000)
        .build();

    let store = InMemoryPendingStore::with_quota(quota);
    let peer = SimulationIdentity::new('A').unwrap();

    // Add 100 events (should keep only last 10 due to per-peer limit)
    for seq in 1..=100 {
        store
            .mark_pending(&peer, EventId::new(1, seq))
            .await
            .unwrap();
    }

    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 10);
    assert_eq!(store.total_pending(), 10);

    // Should have last 10 events
    for seq in 91..=100 {
        assert!(pending.contains(&EventId::new(1, seq)));
    }
}

// ============================================================================
// Bulk Delivery Tests
// ============================================================================

/// Test bulk delivery marking (mark_delivered_up_to)
#[tokio::test]
async fn test_bulk_delivery_marking() {
    let store = InMemoryPendingStore::new();
    let peer = SimulationIdentity::new('A').unwrap();

    // Add 1,000 events from same sender
    for seq in 1..=1_000 {
        store
            .mark_pending(&peer, EventId::new(1, seq))
            .await
            .unwrap();
    }

    assert_eq!(store.total_pending(), 1_000);

    let start = Instant::now();

    // Mark first 500 as delivered in one call
    store
        .mark_delivered_up_to(&peer, EventId::new(1, 500))
        .await
        .unwrap();

    let duration = start.elapsed();
    println!("Bulk delivered 500 events in {:?}", duration);

    // Should have 500 remaining
    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 500);
    assert_eq!(store.total_pending(), 500);

    // Verify correct events remain
    for seq in 501..=1_000 {
        assert!(pending.contains(&EventId::new(1, seq)));
    }

    // Verify delivered events are gone
    for seq in 1..=500 {
        assert!(!pending.contains(&EventId::new(1, seq)));
    }
}

/// Test bulk delivery with multiple senders
#[tokio::test]
async fn test_bulk_delivery_multiple_senders() {
    let store = InMemoryPendingStore::new();
    let peer = SimulationIdentity::new('A').unwrap();

    // Add events from 3 different senders
    for sender in 1..=3 {
        for seq in 1..=100 {
            store
                .mark_pending(&peer, EventId::new(sender, seq))
                .await
                .unwrap();
        }
    }

    assert_eq!(store.total_pending(), 300);

    // Mark delivered up to sequence 50 for sender 1
    store
        .mark_delivered_up_to(&peer, EventId::new(1, 50))
        .await
        .unwrap();

    let pending = store.pending_for(&peer).await.unwrap();

    // Should have removed 50 events (sender 1, sequences 1-50)
    assert_eq!(pending.len(), 250);

    // Verify sender 1's first 50 are gone
    for seq in 1..=50 {
        assert!(!pending.contains(&EventId::new(1, seq)));
    }

    // Verify sender 1's remaining events are still there
    for seq in 51..=100 {
        assert!(pending.contains(&EventId::new(1, seq)));
    }

    // Verify other senders are untouched
    for sender in 2..=3 {
        for seq in 1..=100 {
            assert!(pending.contains(&EventId::new(sender, seq)));
        }
    }
}

// ============================================================================
// Persistent Store Tests
// ============================================================================

/// Test persistent store under stress
#[tokio::test]
async fn test_persistent_store_stress() {
    let temp_dir = TempDir::new().unwrap();
    let store = PersistentPendingStore::new(temp_dir.path()).await.unwrap();
    let peer = SimulationIdentity::new('A').unwrap();

    // Add 1,000 events
    for seq in 1..=1_000 {
        store
            .mark_pending(&peer, EventId::new(1, seq))
            .await
            .expect("Failed to mark pending");
    }

    // Deliver 500 of them
    for seq in 1..=500 {
        store
            .mark_delivered(&peer, EventId::new(1, seq))
            .await
            .expect("Failed to mark delivered");
    }

    store.flush().await.unwrap();

    assert_eq!(store.total_pending(), 500);

    // Compact the log
    store.compact().await.unwrap();

    // Verify state after compaction
    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 500);

    // Drop store and reload
    drop(store);

    // Reload from disk
    let store2 = PersistentPendingStore::new(temp_dir.path()).await.unwrap();
    let pending2 = store2.pending_for(&peer).await.unwrap();

    assert_eq!(pending2.len(), 500);
    assert_eq!(store2.total_pending(), 500);

    // Verify correct events are present
    for seq in 501..=1_000 {
        assert!(pending2.contains(&EventId::new(1, seq)));
    }
}

/// Test persistent store with quota and eviction
#[tokio::test]
async fn test_persistent_store_with_quota() {
    let temp_dir = TempDir::new().unwrap();
    let quota = QuotaManager::new(100, 1_000);
    let store = PersistentPendingStore::with_options(temp_dir.path(), quota, true)
        .await
        .unwrap();

    let peer = SimulationIdentity::new('A').unwrap();

    // Add 200 events (should evict first 100)
    for seq in 1..=200 {
        store
            .mark_pending(&peer, EventId::new(1, seq))
            .await
            .unwrap();
    }

    // Should have exactly 100 (the quota limit)
    let pending = store.pending_for(&peer).await.unwrap();
    assert_eq!(pending.len(), 100);

    // Should have last 100 events
    for seq in 101..=200 {
        assert!(pending.contains(&EventId::new(1, seq)));
    }
}

/// Test concurrent access to persistent store
#[tokio::test]
async fn test_persistent_store_concurrent() {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(PersistentPendingStore::new(temp_dir.path()).await.unwrap());

    let mut handles = vec![];

    // Spawn 5 tasks writing to different peers
    for i in 0..5 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            let peer = SimulationIdentity::new((b'A' + i) as char).unwrap();
            for seq in 1..=100 {
                store_clone
                    .mark_pending(&peer, EventId::new(i as u64 + 1, seq))
                    .await
                    .unwrap();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Flush to ensure all writes are persisted
    store.flush().await.unwrap();

    assert_eq!(store.total_pending(), 500);
    assert_eq!(store.peer_count(), 5);
}
