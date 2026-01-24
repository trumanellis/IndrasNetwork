//! Stress tests for indras-messaging
//!
//! These tests validate the messaging system under high load and stress conditions,
//! including large message volumes, concurrent access, and various edge cases.

use std::collections::HashSet;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use chrono::Utc;
use indras_core::{InterfaceId, SimulationIdentity};
use indras_messaging::{
    ContentValidator, Message, MessageContent, MessageFilter, MessageHistory, MessageId,
    SchemaVersion, TypedContent, ValidationConfig, content_types,
};

/// Helper to create a test interface ID
fn test_interface(byte: u8) -> InterfaceId {
    InterfaceId::new([byte; 32])
}

/// Helper to create a test identity
fn test_identity(c: char) -> SimulationIdentity {
    SimulationIdentity::new(c).unwrap()
}

/// Helper to create a test message
fn create_message(
    interface_id: InterfaceId,
    sender: char,
    seq: u64,
    text: &str,
) -> Message<SimulationIdentity> {
    Message::text(interface_id, test_identity(sender), seq, text)
}

#[test]
fn test_message_history_capacity() {
    println!("\n=== Testing Message History Capacity ===");
    let start = Instant::now();

    let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
    let interface_id = test_interface(0x01);
    let sender = test_identity('A');

    // Store 10,000 messages
    const MESSAGE_COUNT: u64 = 10_000;
    println!("Storing {} messages...", MESSAGE_COUNT);

    for i in 0..MESSAGE_COUNT {
        let msg = Message::text(
            interface_id,
            sender,
            i,
            format!("Message number {}", i),
        );
        history.store(msg).expect("Failed to store message");

        // Print progress every 1000 messages
        if i > 0 && i % 1000 == 0 {
            println!("  Stored {} messages", i);
        }
    }

    let store_duration = start.elapsed();
    println!("Stored {} messages in {:?}", MESSAGE_COUNT, store_duration);

    // Verify count
    let total = history.total_count();
    assert_eq!(total, MESSAGE_COUNT as usize, "Message count mismatch");

    // Verify we can retrieve messages at various points
    let test_sequences = [0, MESSAGE_COUNT / 4, MESSAGE_COUNT / 2, MESSAGE_COUNT * 3 / 4, MESSAGE_COUNT - 1];
    for seq in test_sequences {
        // We can't use the exact message ID since it has a random nonce,
        // but we can query by sequence number
        let messages = history.since(interface_id, seq).expect("Failed to query");
        assert!(!messages.is_empty(), "Should find messages from sequence {}", seq);
        assert_eq!(messages[0].id.sequence, seq, "First message should have correct sequence");
    }

    println!("✓ Successfully stored and verified {} messages", MESSAGE_COUNT);
    println!("  Average time per message: {:?}", store_duration / MESSAGE_COUNT as u32);
}

#[test]
fn test_history_query_performance() {
    println!("\n=== Testing History Query Performance ===");

    let history: MessageHistory<SimulationIdentity> = MessageHistory::new();

    // Create multiple interfaces with various messages
    let interfaces = [
        test_interface(0x01),
        test_interface(0x02),
        test_interface(0x03),
    ];

    let senders = ['A', 'B', 'C', 'D', 'E'];

    // Store 5000 messages across interfaces and senders
    const MESSAGES_PER_COMBO: u64 = 100;
    let mut total_messages = 0u64;

    println!("Setting up test data with {} messages...",
        interfaces.len() * senders.len() * MESSAGES_PER_COMBO as usize);

    for interface in &interfaces {
        for sender in &senders {
            for i in 0..MESSAGES_PER_COMBO {
                let seq = total_messages;
                let msg = create_message(*interface, *sender, seq, &format!("Msg {}", i));
                history.store(msg).expect("Failed to store message");
                total_messages += 1;
            }
        }
    }

    println!("Stored {} total messages", total_messages);

    // Test 1: Query by interface
    println!("\nTest 1: Query by interface");
    let start = Instant::now();
    let filter = MessageFilter::new().interface(interfaces[0]);
    let results = history.query(&filter).expect("Query failed");
    let duration = start.elapsed();
    println!("  Found {} messages in {:?}", results.len(), duration);
    assert_eq!(results.len(), (senders.len() * MESSAGES_PER_COMBO as usize));

    // Test 2: Query by sender
    println!("\nTest 2: Query by sender");
    let start = Instant::now();
    let filter = MessageFilter::new().sender(test_identity('A'));
    let results = history.query(&filter).expect("Query failed");
    let duration = start.elapsed();
    println!("  Found {} messages in {:?}", results.len(), duration);
    assert_eq!(results.len(), (interfaces.len() * MESSAGES_PER_COMBO as usize));

    // Test 3: Query by interface AND sender
    println!("\nTest 3: Query by interface AND sender");
    let start = Instant::now();
    let filter = MessageFilter::new()
        .interface(interfaces[0])
        .sender(test_identity('A'));
    let results = history.query(&filter).expect("Query failed");
    let duration = start.elapsed();
    println!("  Found {} messages in {:?}", results.len(), duration);
    assert_eq!(results.len(), MESSAGES_PER_COMBO as usize);

    // Test 4: Query with time range
    println!("\nTest 4: Query with time range");
    let now = Utc::now();
    let past = now - chrono::Duration::hours(1);
    let start = Instant::now();
    let filter = MessageFilter::new().since(past).until(now);
    let results = history.query(&filter).expect("Query failed");
    let duration = start.elapsed();
    println!("  Found {} messages in {:?}", results.len(), duration);
    // All messages should be within this range
    assert_eq!(results.len(), total_messages as usize);

    // Test 5: Query with limit and offset (pagination)
    println!("\nTest 5: Query with pagination");
    let page_size = 50;
    let mut all_pages = Vec::new();
    let mut offset = 0;

    let start = Instant::now();
    loop {
        let filter = MessageFilter::new()
            .interface(interfaces[0])
            .limit(page_size)
            .offset(offset);
        let page = history.query(&filter).expect("Query failed");

        if page.is_empty() {
            break;
        }

        all_pages.push(page.len());
        offset += page_size;
    }
    let duration = start.elapsed();

    let total_paginated: usize = all_pages.iter().sum();
    println!("  Retrieved {} messages in {} pages in {:?}",
        total_paginated, all_pages.len(), duration);
    assert_eq!(total_paginated, (senders.len() * MESSAGES_PER_COMBO as usize));

    // Test 6: Query text only
    println!("\nTest 6: Query text only");
    let start = Instant::now();
    let filter = MessageFilter::new()
        .interface(interfaces[0])
        .text_only();
    let results = history.query(&filter).expect("Query failed");
    let duration = start.elapsed();
    println!("  Found {} text messages in {:?}", results.len(), duration);
    assert!(results.iter().all(|m| m.content.is_text()));

    println!("\n✓ All query performance tests passed");
}

#[test]
fn test_schema_validation_throughput() {
    println!("\n=== Testing Schema Validation Throughput ===");

    let validator = ContentValidator::default();

    // Test different content types
    let test_cases = vec![
        ("Text", TypedContent::text("Hello, world!")),
        ("Binary", TypedContent::binary("image/png", vec![0u8; 1024])),
        ("System", TypedContent::system("User joined")),
        ("Large Text", TypedContent::text("A".repeat(10_000))),
    ];

    const VALIDATIONS_PER_TYPE: usize = 5_000;

    for (name, content) in test_cases {
        println!("\nValidating {} content {} times...", name, VALIDATIONS_PER_TYPE);
        let start = Instant::now();

        for _ in 0..VALIDATIONS_PER_TYPE {
            validator.validate(&content).expect("Validation failed");
        }

        let duration = start.elapsed();
        let per_validation = duration / VALIDATIONS_PER_TYPE as u32;
        println!("  Total: {:?}", duration);
        println!("  Per validation: {:?}", per_validation);
        println!("  Throughput: {:.0} validations/sec",
            VALIDATIONS_PER_TYPE as f64 / duration.as_secs_f64());
    }

    // Test with custom validator
    println!("\nTesting with custom validator...");
    let mut custom_validator = ContentValidator::new();
    custom_validator.register_validator(content_types::TEXT, |content| {
        // Simple word count check
        if let Some(text) = content.as_text() {
            if text.split_whitespace().count() > 1000 {
                return Err(indras_messaging::SchemaError::ValidationFailed(
                    "Too many words".into()
                ));
            }
        }
        Ok(())
    });

    let content = TypedContent::text("Hello, world!");
    let start = Instant::now();

    for _ in 0..VALIDATIONS_PER_TYPE {
        custom_validator.validate(&content).expect("Custom validation failed");
    }

    let duration = start.elapsed();
    println!("  Custom validator: {:?} ({:.0} validations/sec)",
        duration, VALIDATIONS_PER_TYPE as f64 / duration.as_secs_f64());

    println!("\n✓ Schema validation throughput tests completed");
}

#[test]
fn test_message_serialization() {
    println!("\n=== Testing Message Serialization Performance ===");

    let interface_id = test_interface(0x42);
    let sender = test_identity('A');

    // Create various message types
    let messages = vec![
        Message::text(interface_id, sender, 1, "Short text"),
        Message::text(interface_id, sender, 2, "A".repeat(10_000)),
        Message::new(
            interface_id,
            sender,
            3,
            MessageContent::binary("application/octet-stream", vec![0u8; 1024 * 100]), // 100KB
        ),
        Message::new(
            interface_id,
            sender,
            4,
            MessageContent::file("large_file.bin", 1024 * 1024 * 100, [0x42; 32]),
        ),
    ];

    const ITERATIONS: usize = 1_000;

    for (i, msg) in messages.iter().enumerate() {
        println!("\nMessage type {}: Serializing {} times...", i + 1, ITERATIONS);

        // Serialize
        let start = Instant::now();
        let mut serialized_sizes = Vec::new();

        for _ in 0..ITERATIONS {
            let bytes = postcard::to_allocvec(&msg).expect("Serialization failed");
            serialized_sizes.push(bytes.len());
        }

        let serialize_duration = start.elapsed();
        let avg_size = serialized_sizes.iter().sum::<usize>() / serialized_sizes.len();

        println!("  Serialize: {:?} ({:.2} µs/msg, avg size: {} bytes)",
            serialize_duration,
            serialize_duration.as_micros() as f64 / ITERATIONS as f64,
            avg_size);

        // Deserialize
        let serialized = postcard::to_allocvec(&msg).expect("Serialization failed");
        let start = Instant::now();

        for _ in 0..ITERATIONS {
            let _: Message<SimulationIdentity> = postcard::from_bytes(&serialized)
                .expect("Deserialization failed");
        }

        let deserialize_duration = start.elapsed();
        println!("  Deserialize: {:?} ({:.2} µs/msg)",
            deserialize_duration,
            deserialize_duration.as_micros() as f64 / ITERATIONS as f64);

        // Round-trip
        let start = Instant::now();

        for _ in 0..ITERATIONS {
            let bytes = postcard::to_allocvec(&msg).expect("Serialization failed");
            let _: Message<SimulationIdentity> = postcard::from_bytes(&bytes)
                .expect("Deserialization failed");
        }

        let roundtrip_duration = start.elapsed();
        println!("  Round-trip: {:?} ({:.2} µs/msg)",
            roundtrip_duration,
            roundtrip_duration.as_micros() as f64 / ITERATIONS as f64);
    }

    println!("\n✓ Message serialization performance tests completed");
}

#[test]
fn test_large_message_content() {
    println!("\n=== Testing Large Message Content ===");

    let interface_id = test_interface(0x01);
    let sender = test_identity('A');
    let history: MessageHistory<SimulationIdentity> = MessageHistory::new();

    // Test various large content sizes
    let sizes = vec![
        1024,           // 1 KB
        10 * 1024,      // 10 KB
        100 * 1024,     // 100 KB
        1024 * 1024,    // 1 MB
        5 * 1024 * 1024, // 5 MB
    ];

    for (i, size) in sizes.iter().enumerate() {
        println!("\nTesting message with {} bytes of content...", size);

        // Create large text message
        let large_text = "X".repeat(*size);
        let start = Instant::now();
        let msg = Message::text(interface_id, sender, i as u64, large_text);
        let create_duration = start.elapsed();

        println!("  Created in {:?}", create_duration);

        // Store message
        let start = Instant::now();
        history.store(msg.clone()).expect("Failed to store large message");
        let store_duration = start.elapsed();

        println!("  Stored in {:?}", store_duration);

        // Retrieve message
        let start = Instant::now();
        let retrieved = history.get(&msg.id).expect("Failed to retrieve").expect("Message not found");
        let retrieve_duration = start.elapsed();

        println!("  Retrieved in {:?}", retrieve_duration);

        // Verify content
        assert_eq!(retrieved.content.as_text().unwrap().len(), *size);

        // Serialize
        let start = Instant::now();
        let serialized = postcard::to_allocvec(&msg).expect("Serialization failed");
        let serialize_duration = start.elapsed();

        println!("  Serialized to {} bytes in {:?}", serialized.len(), serialize_duration);
    }

    // Test binary content
    println!("\nTesting large binary message (10 MB)...");
    let large_binary = vec![0xAB; 10 * 1024 * 1024];
    let start = Instant::now();
    let msg = Message::new(
        interface_id,
        sender,
        100,
        MessageContent::binary("application/octet-stream", large_binary.clone()),
    );
    let create_duration = start.elapsed();

    println!("  Created in {:?}", create_duration);

    history.store(msg.clone()).expect("Failed to store large binary message");
    let retrieved = history.get(&msg.id).expect("Failed to retrieve").expect("Message not found");

    if let MessageContent::Binary { data, .. } = &retrieved.content {
        assert_eq!(data.len(), large_binary.len());
        println!("  ✓ Large binary message stored and retrieved successfully");
    } else {
        panic!("Expected binary content");
    }

    println!("\n✓ Large message content tests completed");
}

#[test]
fn test_concurrent_history_access() {
    println!("\n=== Testing Concurrent History Access ===");

    let history = Arc::new(MessageHistory::<SimulationIdentity>::new());
    let interface_id = test_interface(0x01);

    const THREADS: usize = 10;
    const MESSAGES_PER_THREAD: u64 = 500;

    println!("Spawning {} threads, each storing {} messages...",
        THREADS, MESSAGES_PER_THREAD);

    let start = Instant::now();
    let mut handles = Vec::new();

    // Writer threads
    for thread_id in 0..THREADS {
        let history_clone = Arc::clone(&history);
        let sender_char = char::from_u32('A' as u32 + (thread_id as u32 % 26)).unwrap();

        let handle = thread::spawn(move || {
            for i in 0..MESSAGES_PER_THREAD {
                let seq = (thread_id as u64 * MESSAGES_PER_THREAD) + i;
                let msg = create_message(
                    interface_id,
                    sender_char,
                    seq,
                    &format!("Thread {} message {}", thread_id, i),
                );

                history_clone.store(msg).expect("Failed to store message");
            }
        });

        handles.push(handle);
    }

    // Wait for all writers to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let write_duration = start.elapsed();
    println!("All writes completed in {:?}", write_duration);

    // Verify total count
    let total = history.total_count();
    let expected = THREADS * MESSAGES_PER_THREAD as usize;
    assert_eq!(total, expected, "Expected {} messages, got {}", expected, total);

    // Concurrent readers
    println!("\nSpawning {} reader threads...", THREADS);
    let start = Instant::now();
    let mut handles = Vec::new();

    for _ in 0..THREADS {
        let history_clone = Arc::clone(&history);

        let handle = thread::spawn(move || {
            let mut read_count = 0;

            // Perform various queries
            for _ in 0..100 {
                let filter = MessageFilter::new().interface(interface_id).limit(50);
                let results = history_clone.query(&filter).expect("Query failed");
                read_count += results.len();
            }

            read_count
        });

        handles.push(handle);
    }

    // Collect results
    let mut total_reads = 0;
    for handle in handles {
        let read_count = handle.join().expect("Thread panicked");
        total_reads += read_count;
    }

    let read_duration = start.elapsed();
    println!("All reads completed in {:?}", read_duration);
    println!("Total items read: {}", total_reads);

    // Mixed concurrent access (readers and writers)
    println!("\nTesting mixed concurrent access...");
    let start = Instant::now();
    let mut handles = Vec::new();

    for thread_id in 0..THREADS {
        let history_clone = Arc::clone(&history);
        let is_writer = thread_id % 3 == 0; // 1/3 writers, 2/3 readers

        let handle = thread::spawn(move || {
            if is_writer {
                // Writer
                for i in 0..100 {
                    let seq = 1000000 + (thread_id as u64 * 100) + i;
                    let msg = create_message(
                        interface_id,
                        'Z',
                        seq,
                        &format!("Concurrent write {}", i),
                    );
                    history_clone.store(msg).expect("Failed to store");
                }
            } else {
                // Reader
                for _ in 0..200 {
                    let filter = MessageFilter::new().limit(10);
                    let _ = history_clone.query(&filter).expect("Query failed");
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let mixed_duration = start.elapsed();
    println!("Mixed concurrent access completed in {:?}", mixed_duration);

    println!("\n✓ Concurrent access tests completed successfully");
}

#[test]
fn test_filter_combinations() {
    println!("\n=== Testing Filter Combinations ===");

    let history: MessageHistory<SimulationIdentity> = MessageHistory::new();

    // Set up test data
    let interfaces = [test_interface(0x01), test_interface(0x02)];
    let senders = ['A', 'B', 'C'];
    let now = Utc::now();

    println!("Setting up test data...");
    let mut seq = 0u64;

    for interface in &interfaces {
        for sender in &senders {
            for i in 0..20 {
                let msg = create_message(*interface, *sender, seq, &format!("Msg {}", i));
                history.store(msg).expect("Failed to store");
                seq += 1;
            }
        }
    }

    println!("Testing all filter combinations...");

    // Test 1: No filters (return all)
    let filter = MessageFilter::new();
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 120); // 2 interfaces * 3 senders * 20 messages
    println!("  ✓ No filters: {} messages", results.len());

    // Test 2: Interface only
    let filter = MessageFilter::new().interface(interfaces[0]);
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 60); // 3 senders * 20 messages
    println!("  ✓ Interface filter: {} messages", results.len());

    // Test 3: Sender only
    let filter = MessageFilter::new().sender(test_identity('A'));
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 40); // 2 interfaces * 20 messages
    println!("  ✓ Sender filter: {} messages", results.len());

    // Test 4: Interface + Sender
    let filter = MessageFilter::new()
        .interface(interfaces[0])
        .sender(test_identity('A'));
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 20);
    println!("  ✓ Interface + Sender: {} messages", results.len());

    // Test 5: Time range
    let past = now - chrono::Duration::hours(1);
    let future = now + chrono::Duration::hours(1);
    let filter = MessageFilter::new().since(past).until(future);
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 120); // All messages in range
    println!("  ✓ Time range: {} messages", results.len());

    // Test 6: Exclude by time (past only)
    let way_future = now + chrono::Duration::hours(10);
    let filter = MessageFilter::new().since(way_future);
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 0); // No messages in future
    println!("  ✓ Future time filter: {} messages", results.len());

    // Test 7: Text only (all our test messages are text)
    let filter = MessageFilter::new().text_only();
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 120);
    println!("  ✓ Text only: {} messages", results.len());

    // Test 8: Text only + Interface
    let filter = MessageFilter::new()
        .interface(interfaces[0])
        .text_only();
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 60);
    println!("  ✓ Text only + Interface: {} messages", results.len());

    // Test 9: Limit
    let filter = MessageFilter::new().limit(10);
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 10);
    println!("  ✓ Limit: {} messages", results.len());

    // Test 10: Offset
    let filter = MessageFilter::new().offset(100);
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 20);
    println!("  ✓ Offset: {} messages", results.len());

    // Test 11: Limit + Offset
    let filter = MessageFilter::new().limit(15).offset(10);
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 15);
    println!("  ✓ Limit + Offset: {} messages", results.len());

    // Test 12: All filters combined
    let filter = MessageFilter::new()
        .interface(interfaces[0])
        .sender(test_identity('B'))
        .since(past)
        .until(future)
        .text_only()
        .limit(5)
        .offset(2);
    let results = history.query(&filter).expect("Query failed");
    assert_eq!(results.len(), 5);
    println!("  ✓ All filters combined: {} messages", results.len());

    // Verify results are sorted by timestamp
    for i in 1..results.len() {
        assert!(
            results[i].timestamp >= results[i - 1].timestamp,
            "Results should be sorted by timestamp"
        );
    }

    println!("\n✓ All filter combination tests passed");
}

#[test]
fn test_interface_isolation() {
    println!("\n=== Testing Interface Isolation ===");

    let history: MessageHistory<SimulationIdentity> = MessageHistory::new();

    const NUM_INTERFACES: u8 = 50;
    const MESSAGES_PER_INTERFACE: u64 = 100;

    println!("Creating {} interfaces with {} messages each...",
        NUM_INTERFACES, MESSAGES_PER_INTERFACE);

    let start = Instant::now();

    // Create many interfaces with messages
    for interface_num in 0..NUM_INTERFACES {
        let interface_id = test_interface(interface_num);

        for seq in 0..MESSAGES_PER_INTERFACE {
            let sender_char = char::from_u32('A' as u32 + (seq % 26) as u32).unwrap();
            let msg = create_message(
                interface_id,
                sender_char,
                seq,
                &format!("Interface {} message {}", interface_num, seq),
            );
            history.store(msg).expect("Failed to store message");
        }
    }

    let setup_duration = start.elapsed();
    println!("Setup completed in {:?}", setup_duration);

    // Verify total count
    let total = history.total_count();
    let expected = (NUM_INTERFACES as usize) * (MESSAGES_PER_INTERFACE as usize);
    assert_eq!(total, expected);
    println!("Total messages stored: {}", total);

    // Test isolation: queries should only return messages from the specified interface
    println!("\nTesting interface isolation...");

    for interface_num in (0..NUM_INTERFACES).step_by(10) {
        let interface_id = test_interface(interface_num);
        let filter = MessageFilter::new().interface(interface_id);
        let results = history.query(&filter).expect("Query failed");

        assert_eq!(
            results.len(),
            MESSAGES_PER_INTERFACE as usize,
            "Interface {} should have exactly {} messages",
            interface_num,
            MESSAGES_PER_INTERFACE
        );

        // Verify all messages belong to this interface
        for msg in &results {
            assert_eq!(
                msg.interface_id,
                interface_id,
                "Message should belong to interface {}",
                interface_num
            );
        }
    }

    println!("  ✓ All interfaces properly isolated");

    // Test that we can query across all interfaces
    println!("\nTesting cross-interface queries...");

    let filter = MessageFilter::new().sender(test_identity('A'));
    let results = history.query(&filter).expect("Query failed");

    // 'A' appears at position 0, 26, 52, 78 in each interface (every 26 messages)
    // So in 100 messages per interface, 'A' appears 4 times
    let expected_a_messages = (NUM_INTERFACES as usize) * 4;
    assert_eq!(results.len(), expected_a_messages);
    println!("  ✓ Found {} messages from sender 'A' across all interfaces", results.len());

    // Verify messages come from different interfaces
    let mut interfaces_seen = HashSet::new();
    for msg in &results {
        interfaces_seen.insert(msg.interface_id);
    }
    assert_eq!(
        interfaces_seen.len(),
        NUM_INTERFACES as usize,
        "Should see messages from all interfaces"
    );

    // Test per-interface counts
    println!("\nVerifying per-interface message counts...");
    for interface_num in 0..NUM_INTERFACES {
        let interface_id = test_interface(interface_num);
        let count = history.count(interface_id);
        assert_eq!(
            count,
            MESSAGES_PER_INTERFACE as usize,
            "Interface {} count mismatch",
            interface_num
        );
    }
    println!("  ✓ All interface counts correct");

    // Test clearing a single interface
    println!("\nTesting interface clearing...");
    let clear_interface = test_interface(0);
    history.clear(clear_interface).expect("Failed to clear interface");

    assert_eq!(history.count(clear_interface), 0, "Cleared interface should have 0 messages");

    let new_total = history.total_count();
    let expected_after_clear = expected - (MESSAGES_PER_INTERFACE as usize);
    assert_eq!(
        new_total,
        expected_after_clear,
        "Total count should decrease after clearing one interface"
    );

    println!("  ✓ Interface clearing works correctly");

    // Verify other interfaces unaffected
    for interface_num in 1..NUM_INTERFACES {
        let interface_id = test_interface(interface_num);
        let count = history.count(interface_id);
        assert_eq!(
            count,
            MESSAGES_PER_INTERFACE as usize,
            "Other interfaces should be unaffected"
        );
    }

    println!("\n✓ Interface isolation tests completed successfully");
}

#[test]
fn test_message_id_uniqueness() {
    println!("\n=== Testing Message ID Uniqueness ===");

    let interface_id = test_interface(0x01);
    let mut seen_ids = HashSet::new();

    // Generate many message IDs with the same sequence number
    // The nonce should make them unique
    const ID_COUNT: usize = 10_000;
    println!("Generating {} message IDs with same sequence number...", ID_COUNT);

    for _ in 0..ID_COUNT {
        let id = MessageId::new(interface_id, 42);
        assert!(
            seen_ids.insert(id),
            "Duplicate message ID detected!"
        );
    }

    println!("  ✓ All {} IDs are unique", ID_COUNT);

    // Test IDs with different sequences
    seen_ids.clear();
    println!("Generating {} message IDs with different sequences...", ID_COUNT);

    for seq in 0..ID_COUNT as u64 {
        let id = MessageId::new(interface_id, seq);
        assert!(
            seen_ids.insert(id),
            "Duplicate message ID detected at sequence {}!",
            seq
        );
    }

    println!("  ✓ All {} IDs are unique", ID_COUNT);

    println!("\n✓ Message ID uniqueness tests passed");
}

#[test]
fn test_schema_version_compatibility() {
    println!("\n=== Testing Schema Version Compatibility ===");

    let v1_0 = SchemaVersion::new(1, 0);
    let v1_1 = SchemaVersion::new(1, 1);
    let v1_5 = SchemaVersion::new(1, 5);
    let v2_0 = SchemaVersion::new(2, 0);

    println!("Testing version compatibility rules...");

    // Same major, newer minor can read older minor
    assert!(v1_1.can_read(&v1_0), "v1.1 should read v1.0");
    assert!(v1_5.can_read(&v1_0), "v1.5 should read v1.0");
    assert!(v1_5.can_read(&v1_1), "v1.5 should read v1.1");

    // Same major, older minor cannot read newer minor
    assert!(!v1_0.can_read(&v1_1), "v1.0 should not read v1.1");
    assert!(!v1_1.can_read(&v1_5), "v1.1 should not read v1.5");

    // Different major versions are incompatible
    assert!(!v2_0.can_read(&v1_0), "v2.0 should not read v1.0");
    assert!(!v1_0.can_read(&v2_0), "v1.0 should not read v2.0");

    println!("  ✓ Version compatibility rules work correctly");

    // Test version ordering
    assert!(v1_1.is_newer_than(&v1_0));
    assert!(v2_0.is_newer_than(&v1_5));
    assert!(!v1_0.is_newer_than(&v1_1));

    println!("  ✓ Version ordering works correctly");

    // Test validation with strict version checking
    println!("\nTesting strict version validation...");

    let strict_config = ValidationConfig {
        strict_versions: true,
        ..Default::default()
    };
    let strict_validator = ContentValidator::with_config(strict_config);

    let current_content = TypedContent::new(
        content_types::TEXT,
        SchemaVersion::CURRENT,
        b"Hello".to_vec(),
    );

    assert!(
        strict_validator.validate(&current_content).is_ok(),
        "Current version should validate"
    );

    // Future version should fail with strict validation
    let future_content = TypedContent::new(
        content_types::TEXT,
        SchemaVersion::new(99, 0),
        b"Hello".to_vec(),
    );

    assert!(
        strict_validator.validate(&future_content).is_err(),
        "Future version should fail strict validation"
    );

    println!("  ✓ Strict version validation works correctly");

    println!("\n✓ Schema version compatibility tests passed");
}
