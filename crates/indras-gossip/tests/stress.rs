//! Stress tests for indras-gossip
//!
//! These tests focus on message signing/encoding performance and can run without network.

use indras_core::{InterfaceEvent, MembershipChange, PresenceStatus, SimulationIdentity};
use indras_gossip::SignedMessage;
use iroh::SecretKey;
use std::thread;
use std::time::Instant;

/// Helper to generate test identities
fn test_identity(c: char) -> SimulationIdentity {
    SimulationIdentity::new(c).unwrap()
}

/// Helper to generate a test message event
fn test_message(peer: SimulationIdentity, seq: u64, content: Vec<u8>) -> InterfaceEvent<SimulationIdentity> {
    InterfaceEvent::message(peer, seq, content)
}

/// Helper to generate a test membership event
fn test_membership(
    actor: SimulationIdentity,
    seq: u64,
    change: MembershipChange<SimulationIdentity>,
) -> InterfaceEvent<SimulationIdentity> {
    InterfaceEvent::membership(&actor, seq, change)
}

/// Helper to generate a test presence event
fn test_presence(peer: SimulationIdentity, status: PresenceStatus) -> InterfaceEvent<SimulationIdentity> {
    InterfaceEvent::presence(peer, status)
}

/// Helper to generate a test custom event
fn test_custom(
    sender: SimulationIdentity,
    seq: u64,
    event_type: String,
    payload: Vec<u8>,
) -> InterfaceEvent<SimulationIdentity> {
    InterfaceEvent::custom(sender, seq, event_type, payload)
}

#[test]
fn test_message_signing_throughput() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('A');
    let content = b"Hello, world! This is a test message.".to_vec();

    let count = 1000;
    let start = Instant::now();

    for i in 0..count {
        let event = test_message(peer, i, content.clone());
        let result = SignedMessage::sign_and_encode(&secret_key, &event);
        assert!(result.is_ok(), "Failed to sign message {}: {:?}", i, result.err());
    }

    let elapsed = start.elapsed();
    let msgs_per_sec = count as f64 / elapsed.as_secs_f64();

    println!("✓ Signed {} messages in {:?}", count, elapsed);
    println!("  Throughput: {:.2} messages/sec", msgs_per_sec);
    println!("  Avg latency: {:.2} μs/message", elapsed.as_micros() as f64 / count as f64);

    // Sanity check: should be able to sign at least 100 messages/sec
    assert!(msgs_per_sec > 100.0, "Signing throughput too low: {:.2} msgs/sec", msgs_per_sec);
}

#[test]
fn test_message_verification_throughput() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('B');
    let content = b"Test verification throughput".to_vec();

    // Pre-generate signed messages
    let count = 1000;
    let mut signed_messages = Vec::with_capacity(count);

    for i in 0..count {
        let event = test_message(peer, i as u64, content.clone());
        let encoded = SignedMessage::sign_and_encode(&secret_key, &event)
            .expect("Failed to sign message");
        signed_messages.push(encoded);
    }

    // Measure verification
    let start = Instant::now();

    for (i, encoded) in signed_messages.iter().enumerate() {
        let result = SignedMessage::verify_and_decode::<SimulationIdentity>(encoded);
        assert!(result.is_ok(), "Failed to verify message {}: {:?}", i, result.err());
    }

    let elapsed = start.elapsed();
    let msgs_per_sec = count as f64 / elapsed.as_secs_f64();

    println!("✓ Verified {} signatures in {:?}", count, elapsed);
    println!("  Throughput: {:.2} messages/sec", msgs_per_sec);
    println!("  Avg latency: {:.2} μs/message", elapsed.as_micros() as f64 / count as f64);

    // Sanity check: verification should be at least as fast as signing
    assert!(msgs_per_sec > 100.0, "Verification throughput too low: {:.2} msgs/sec", msgs_per_sec);
}

#[test]
fn test_wire_message_encoding() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('C');
    let content = b"Encoding test".to_vec();

    let count = 5000;
    let start = Instant::now();

    for i in 0..count {
        let event = test_message(peer, i as u64, content.clone());

        // Sign and encode
        let encoded = SignedMessage::sign_and_encode(&secret_key, &event)
            .expect("Failed to encode");

        // Verify and decode
        let decoded = SignedMessage::verify_and_decode::<SimulationIdentity>(&encoded)
            .expect("Failed to decode");

        // Verify round-trip integrity
        assert_eq!(decoded.from, secret_key.public());
        if let InterfaceEvent::Message { content: decoded_content, .. } = decoded.event {
            assert_eq!(decoded_content, content);
        } else {
            panic!("Decoded event is not a Message");
        }
    }

    let elapsed = start.elapsed();
    let ops_per_sec = (count * 2) as f64 / elapsed.as_secs_f64(); // *2 for encode + decode

    println!("✓ Encoded/decoded {} messages in {:?}", count, elapsed);
    println!("  Throughput: {:.2} operations/sec", ops_per_sec);
    println!("  Avg latency: {:.2} μs/operation", elapsed.as_micros() as f64 / (count * 2) as f64);

    // Should handle at least 200 encode+decode ops/sec
    assert!(ops_per_sec > 200.0, "Encoding throughput too low: {:.2} ops/sec", ops_per_sec);
}

#[test]
fn test_large_payload_handling() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('D');

    // Test with 64KB payload
    let large_content = vec![0x42u8; 64 * 1024];

    let count = 100;
    let start = Instant::now();

    for i in 0..count {
        let event = test_message(peer, i as u64, large_content.clone());

        // Sign and encode
        let encoded = SignedMessage::sign_and_encode(&secret_key, &event)
            .expect("Failed to encode large message");

        // Verify size is reasonable (should be slightly larger than payload)
        assert!(encoded.len() > large_content.len());
        assert!(encoded.len() < large_content.len() + 1024, "Encoded size too large");

        // Verify and decode
        let decoded = SignedMessage::verify_and_decode::<SimulationIdentity>(&encoded)
            .expect("Failed to decode large message");

        if let InterfaceEvent::Message { content: decoded_content, .. } = decoded.event {
            assert_eq!(decoded_content.len(), large_content.len());
            assert_eq!(decoded_content, large_content);
        } else {
            panic!("Decoded event is not a Message");
        }
    }

    let elapsed = start.elapsed();
    let msgs_per_sec = count as f64 / elapsed.as_secs_f64();
    let mb_per_sec = (count as f64 * large_content.len() as f64) / (1024.0 * 1024.0) / elapsed.as_secs_f64();

    println!("✓ Processed {} 64KB messages in {:?}", count, elapsed);
    println!("  Throughput: {:.2} messages/sec", msgs_per_sec);
    println!("  Data rate: {:.2} MB/sec", mb_per_sec);
    println!("  Avg latency: {:.2} ms/message", elapsed.as_millis() as f64 / count as f64);

    // Should handle at least 10 large messages per second
    assert!(msgs_per_sec > 10.0, "Large payload throughput too low: {:.2} msgs/sec", msgs_per_sec);
}

#[test]
fn test_concurrent_signing() {
    let count_per_thread = 200;
    let num_threads = 8;
    let total_count = count_per_thread * num_threads;

    let start = Instant::now();
    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            thread::spawn(move || {
                let secret_key = SecretKey::generate(&mut rand::rng());
                let peer = test_identity((b'A' + (thread_id % 26) as u8) as char);
                let content = format!("Thread {} message", thread_id).into_bytes();

                for i in 0..count_per_thread {
                    let event = test_message(peer, i as u64, content.clone());
                    let encoded = SignedMessage::sign_and_encode(&secret_key, &event)
                        .expect("Failed to sign in thread");

                    // Also verify to ensure correctness under concurrency
                    let decoded = SignedMessage::verify_and_decode::<SimulationIdentity>(&encoded)
                        .expect("Failed to verify in thread");

                    assert_eq!(decoded.from, secret_key.public());
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let elapsed = start.elapsed();
    let msgs_per_sec = total_count as f64 / elapsed.as_secs_f64();

    println!("✓ {} threads signed {} messages each ({} total) in {:?}",
             num_threads, count_per_thread, total_count, elapsed);
    println!("  Throughput: {:.2} messages/sec", msgs_per_sec);
    println!("  Avg latency: {:.2} μs/message", elapsed.as_micros() as f64 / total_count as f64);

    // Should handle concurrent signing efficiently
    assert!(msgs_per_sec > 100.0, "Concurrent signing throughput too low: {:.2} msgs/sec", msgs_per_sec);
}

#[test]
fn test_interface_event_serialization() {
    let peer = test_identity('E');
    let content = b"Serialization test".to_vec();

    let count = 1000;
    let start = Instant::now();

    for i in 0..count {
        let event = test_message(peer, i as u64, content.clone());

        // Serialize
        let serialized = postcard::to_allocvec(&event)
            .expect("Failed to serialize");

        // Deserialize
        let deserialized: InterfaceEvent<SimulationIdentity> = postcard::from_bytes(&serialized)
            .expect("Failed to deserialize");

        // Verify round-trip
        if let InterfaceEvent::Message { content: deserialized_content, .. } = deserialized {
            assert_eq!(deserialized_content, content);
        } else {
            panic!("Deserialized event is not a Message");
        }
    }

    let elapsed = start.elapsed();
    let ops_per_sec = (count * 2) as f64 / elapsed.as_secs_f64(); // *2 for ser + deser

    println!("✓ Serialized/deserialized {} events in {:?}", count, elapsed);
    println!("  Throughput: {:.2} operations/sec", ops_per_sec);
    println!("  Avg latency: {:.2} μs/operation", elapsed.as_micros() as f64 / (count * 2) as f64);

    // Should handle at least 1000 ser+deser ops/sec
    assert!(ops_per_sec > 1000.0, "Serialization throughput too low: {:.2} ops/sec", ops_per_sec);
}

#[test]
fn test_mixed_event_types() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer_a = test_identity('A');
    let peer_b = test_identity('B');
    let peer_c = test_identity('C');

    // Create a mix of all event types
    let events: Vec<InterfaceEvent<SimulationIdentity>> = vec![
        // Messages
        test_message(peer_a, 0, b"Hello".to_vec()),
        test_message(peer_b, 0, b"World".to_vec()),
        test_message(peer_c, 0, vec![0x00; 1024]), // Binary data

        // Membership changes
        test_membership(peer_a, 1, MembershipChange::Created { creator: peer_a }),
        test_membership(peer_a, 2, MembershipChange::Invited { by: peer_a, peer: peer_b }),
        test_membership(peer_b, 1, MembershipChange::Joined { peer: peer_b }),
        test_membership(peer_c, 0, MembershipChange::Left { peer: peer_c }),
        test_membership(peer_a, 3, MembershipChange::Removed { by: peer_a, peer: peer_c }),

        // Presence updates
        test_presence(peer_a, PresenceStatus::Online),
        test_presence(peer_b, PresenceStatus::Away),
        test_presence(peer_c, PresenceStatus::Busy),
        test_presence(peer_a, PresenceStatus::Offline),

        // Custom events
        test_custom(peer_a, 4, "custom.event.type".to_string(), b"Custom payload".to_vec()),
        test_custom(peer_b, 2, "app.specific".to_string(), vec![1, 2, 3, 4, 5]),
        test_custom(peer_c, 1, "test.event".to_string(), vec![]),
    ];

    let count = 200; // Process each event type multiple times
    let start = Instant::now();

    for i in 0..count {
        for event in &events {
            // Sign and encode
            let encoded = SignedMessage::sign_and_encode(&secret_key, event)
                .expect("Failed to encode mixed event");

            // Verify and decode
            let decoded = SignedMessage::verify_and_decode::<SimulationIdentity>(&encoded)
                .expect("Failed to decode mixed event");

            // Verify signature
            assert_eq!(decoded.from, secret_key.public());

            // Verify event type is preserved
            match (&event, &decoded.event) {
                (InterfaceEvent::Message { .. }, InterfaceEvent::Message { .. }) => {},
                (InterfaceEvent::MembershipChange { .. }, InterfaceEvent::MembershipChange { .. }) => {},
                (InterfaceEvent::Presence { .. }, InterfaceEvent::Presence { .. }) => {},
                (InterfaceEvent::Custom { .. }, InterfaceEvent::Custom { .. }) => {},
                (InterfaceEvent::SyncMarker { .. }, InterfaceEvent::SyncMarker { .. }) => {},
                _ => panic!("Event type changed during round-trip at iteration {}", i),
            }
        }
    }

    let total_events = count * events.len();
    let elapsed = start.elapsed();
    let events_per_sec = total_events as f64 / elapsed.as_secs_f64();

    println!("✓ Processed {} mixed events ({} types × {} iterations) in {:?}",
             total_events, events.len(), count, elapsed);
    println!("  Throughput: {:.2} events/sec", events_per_sec);
    println!("  Avg latency: {:.2} μs/event", elapsed.as_micros() as f64 / total_events as f64);

    // Should handle mixed event types efficiently
    assert!(events_per_sec > 100.0, "Mixed event throughput too low: {:.2} events/sec", events_per_sec);
}

#[test]
fn test_roundtrip_integrity() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('F');

    let test_cases = vec![
        // Empty message
        test_message(peer, 0, vec![]),
        // Small message
        test_message(peer, 1, b"Hello".to_vec()),
        // Medium message
        test_message(peer, 2, vec![0xAB; 1024]),
        // Large message
        test_message(peer, 3, vec![0xCD; 32 * 1024]),
        // Binary data with all byte values
        test_message(peer, 4, (0..=255).collect()),
        // UTF-8 text
        test_message(peer, 5, "Hello 世界 🌍".as_bytes().to_vec()),
    ];

    let iterations = 200;
    let start = Instant::now();

    for iteration in 0..iterations {
        for (test_idx, event) in test_cases.iter().enumerate() {
            // Step 1: Sign
            let encoded = SignedMessage::sign_and_encode(&secret_key, event)
                .unwrap_or_else(|e| panic!("Failed to sign test case {} iteration {}: {:?}", test_idx, iteration, e));

            // Step 2: Encode (already done in sign_and_encode)

            // Step 3: Decode
            let decoded = SignedMessage::verify_and_decode::<SimulationIdentity>(&encoded)
                .unwrap_or_else(|e| panic!("Failed to decode test case {} iteration {}: {:?}", test_idx, iteration, e));

            // Step 4: Verify signature
            assert_eq!(decoded.from, secret_key.public(),
                      "Signature mismatch in test case {} iteration {}", test_idx, iteration);

            // Step 5: Verify content integrity
            if let (InterfaceEvent::Message { content: original_content, .. },
                    InterfaceEvent::Message { content: decoded_content, .. }) = (event, &decoded.event) {
                assert_eq!(original_content.len(), decoded_content.len(),
                          "Content length mismatch in test case {} iteration {}", test_idx, iteration);
                assert_eq!(original_content, decoded_content,
                          "Content mismatch in test case {} iteration {}", test_idx, iteration);
            } else {
                panic!("Event type changed in test case {} iteration {}", test_idx, iteration);
            }
        }
    }

    let total_operations = iterations * test_cases.len();
    let elapsed = start.elapsed();
    let ops_per_sec = total_operations as f64 / elapsed.as_secs_f64();

    println!("✓ Verified integrity for {} operations ({} test cases × {} iterations) in {:?}",
             total_operations, test_cases.len(), iterations, elapsed);
    println!("  Throughput: {:.2} operations/sec", ops_per_sec);
    println!("  Avg latency: {:.2} μs/operation", elapsed.as_micros() as f64 / total_operations as f64);

    // Should maintain integrity under repeated operations
    assert!(ops_per_sec > 50.0, "Roundtrip integrity throughput too low: {:.2} ops/sec", ops_per_sec);
}

#[test]
fn test_signature_verification_failure() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('G');
    let content = b"Test tamper detection".to_vec();

    let count = 100;
    let mut failures = 0;

    for i in 0..count {
        let event = test_message(peer, i as u64, content.clone());
        let mut encoded = SignedMessage::sign_and_encode(&secret_key, &event)
            .expect("Failed to encode");

        // Tamper with the message by flipping a bit in the middle
        if encoded.len() > 10 {
            let tamper_idx = encoded.len() / 2;
            encoded[tamper_idx] ^= 0xFF;
        }

        // Verification should fail
        let result = SignedMessage::verify_and_decode::<SimulationIdentity>(&encoded);
        if result.is_err() {
            failures += 1;
        }
    }

    println!("✓ Detected {} tampered messages out of {} ({:.1}%)",
             failures, count, (failures as f64 / count as f64) * 100.0);

    // Should detect ALL tampered messages
    assert_eq!(failures, count, "Failed to detect some tampered messages");
}

#[test]
fn test_encoding_determinism() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('H');
    let content = b"Determinism test".to_vec();

    // Create the same event
    let event = test_message(peer, 42, content.clone());

    // Encode it multiple times
    // Note: Encoding includes timestamp, so it won't be deterministic.
    // But the structure should be consistent.
    let count = 100;
    let mut sizes = Vec::new();

    for _ in 0..count {
        let encoded = SignedMessage::sign_and_encode(&secret_key, &event)
            .expect("Failed to encode");
        sizes.push(encoded.len());
    }

    // All encoded messages should have the same size (even if content differs due to timestamp)
    let first_size = sizes[0];
    let all_same_size = sizes.iter().all(|&size| size == first_size);

    println!("✓ Encoded {} identical events", count);
    println!("  All sizes equal: {}", all_same_size);
    println!("  Size: {} bytes", first_size);

    assert!(all_same_size, "Encoded message sizes are inconsistent");
}

#[test]
fn test_memory_efficiency() {
    let secret_key = SecretKey::generate(&mut rand::rng());
    let peer = test_identity('I');

    // Test with various payload sizes
    let payload_sizes = vec![0, 10, 100, 1000, 10000];

    for payload_size in payload_sizes {
        let content = vec![0x42u8; payload_size];
        let event = test_message(peer, 0, content.clone());

        let encoded = SignedMessage::sign_and_encode(&secret_key, &event)
            .expect("Failed to encode");

        let overhead = encoded.len() as i64 - payload_size as i64;
        let overhead_pct = (overhead as f64 / payload_size.max(1) as f64) * 100.0;

        println!("  Payload: {} bytes → Encoded: {} bytes (overhead: {} bytes, {:.1}%)",
                 payload_size, encoded.len(), overhead, overhead_pct);

        // Overhead should be reasonable (< 1KB for fixed signature/metadata)
        assert!(overhead < 1024, "Encoding overhead too large: {} bytes", overhead);
    }

    println!("✓ Memory efficiency verified across payload sizes");
}
