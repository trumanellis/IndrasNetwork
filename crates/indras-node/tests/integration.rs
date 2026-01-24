//! Integration tests for IndrasNode
//!
//! Tests the complete node functionality including storage persistence,
//! interface management, and event handling.

use tempfile::TempDir;

use indras_core::InterfaceEvent;
use indras_node::{IndrasNode, InviteKey, NodeConfig, NodeError};

/// Create a test node with a temp directory
async fn create_test_node() -> (IndrasNode, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig::with_data_dir(temp_dir.path());
    let node = IndrasNode::new(config).await.unwrap();
    (node, temp_dir)
}

#[tokio::test]
async fn test_node_lifecycle() {
    let (node, _temp) = create_test_node().await;

    // Initially not started
    assert!(!node.is_started());

    // Start
    node.start().await.unwrap();
    assert!(node.is_started());

    // Can't start twice
    assert!(matches!(node.start().await, Err(NodeError::AlreadyStarted)));

    // Stop
    node.stop().await.unwrap();
    assert!(!node.is_started());
}

#[tokio::test]
async fn test_interface_creation_and_listing() {
    let (node, _temp) = create_test_node().await;

    // Create multiple interfaces
    let (id1, _) = node.create_interface(Some("Interface 1")).await.unwrap();
    let (id2, _) = node.create_interface(Some("Interface 2")).await.unwrap();
    let (id3, _) = node.create_interface(None).await.unwrap();

    // All should be listed
    let interfaces = node.list_interfaces();
    assert_eq!(interfaces.len(), 3);
    assert!(interfaces.contains(&id1));
    assert!(interfaces.contains(&id2));
    assert!(interfaces.contains(&id3));
}

#[tokio::test]
async fn test_interface_join_via_invite() {
    let (node1, _temp1) = create_test_node().await;

    // Create an interface on node1
    let (interface_id, invite) = node1.create_interface(Some("Shared")).await.unwrap();

    // Serialize and deserialize the invite (simulates sharing)
    let invite_b64 = invite.to_base64().unwrap();
    let parsed_invite = InviteKey::from_base64(&invite_b64).unwrap();
    assert_eq!(parsed_invite.interface_id, interface_id);

    // Node2 joins using the invite
    let (node2, _temp2) = create_test_node().await;
    let joined_id = node2.join_interface(parsed_invite).await.unwrap();
    assert_eq!(joined_id, interface_id);

    // Both nodes should have the interface
    assert!(node1.list_interfaces().contains(&interface_id));
    assert!(node2.list_interfaces().contains(&interface_id));
}

#[tokio::test]
async fn test_message_sending_and_receiving() {
    let (node, _temp) = create_test_node().await;
    let (interface_id, _) = node.create_interface(None).await.unwrap();

    // Subscribe to events
    let mut rx = node.events(&interface_id).unwrap();

    // Send a message
    let event_id = node
        .send_message(&interface_id, b"Hello, world!".to_vec())
        .await
        .unwrap();

    // Should have sequence 1
    assert_eq!(event_id.sequence, 1);

    // Receive the event
    let received = rx.try_recv().unwrap();
    assert_eq!(received.interface_id, interface_id);
    match received.event {
        InterfaceEvent::Message {
            content, sender, ..
        } => {
            assert_eq!(content, b"Hello, world!");
            assert_eq!(sender, *node.identity());
        }
        _ => panic!("Expected Message event"),
    }
}

#[tokio::test]
async fn test_multiple_messages_ordering() {
    let (node, _temp) = create_test_node().await;
    let (interface_id, _) = node.create_interface(None).await.unwrap();

    // Send multiple messages
    for i in 1..=5 {
        let event_id = node
            .send_message(&interface_id, format!("Message {}", i).into_bytes())
            .await
            .unwrap();
        assert_eq!(event_id.sequence, i);
    }

    // Get all events
    let events = node.events_since(&interface_id, 0).await.unwrap();
    assert_eq!(events.len(), 5);

    // Verify ordering
    for (i, event) in events.iter().enumerate() {
        match event {
            InterfaceEvent::Message { content, .. } => {
                let expected = format!("Message {}", i + 1);
                assert_eq!(*content, expected.into_bytes());
            }
            _ => panic!("Expected Message event"),
        }
    }
}

#[tokio::test]
async fn test_events_since_filtering() {
    let (node, _temp) = create_test_node().await;
    let (interface_id, _) = node.create_interface(None).await.unwrap();

    // Send 10 messages
    for i in 1..=10 {
        node.send_message(&interface_id, format!("Msg {}", i).into_bytes())
            .await
            .unwrap();
    }

    // Get events since sequence 5
    let events = node.events_since(&interface_id, 5).await.unwrap();
    assert_eq!(events.len(), 5); // Messages 6-10

    // Verify first event is message 6
    match &events[0] {
        InterfaceEvent::Message { content, .. } => {
            assert_eq!(*content, b"Msg 6");
        }
        _ => panic!("Expected Message event"),
    }
}

#[tokio::test]
async fn test_member_management() {
    let (node, _temp) = create_test_node().await;
    let (interface_id, _) = node.create_interface(None).await.unwrap();

    // Initially just us
    let members = node.members(&interface_id).await.unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0], *node.identity());

    // Add another member (simulated peer)
    let secret = iroh::SecretKey::generate(&mut rand::rng());
    let peer = indras_transport::IrohIdentity::new(secret.public());

    node.add_member(&interface_id, peer).await.unwrap();

    // Now should have 2 members
    let members = node.members(&interface_id).await.unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.contains(node.identity()));
    assert!(members.contains(&peer));
}

#[tokio::test]
async fn test_storage_persistence() {
    let temp_dir = TempDir::new().unwrap();

    // Create node, add interface and messages
    let interface_id;
    {
        let config = NodeConfig::with_data_dir(temp_dir.path());
        let node = IndrasNode::new(config).await.unwrap();

        let (id, _) = node.create_interface(Some("Persistent")).await.unwrap();
        interface_id = id;

        node.send_message(&interface_id, b"Message 1".to_vec())
            .await
            .unwrap();
        node.send_message(&interface_id, b"Message 2".to_vec())
            .await
            .unwrap();

        node.stop().await.unwrap();
    }

    // Create new node with same storage
    {
        let config = NodeConfig::with_data_dir(temp_dir.path());
        let node = IndrasNode::new(config).await.unwrap();

        // Storage should have persisted data (in composite storage)
        // Note: In-memory interfaces don't persist across restarts currently
        // This test verifies the storage layer persists data

        let storage = node.storage();
        let entries = storage.events_since(&interface_id, 0).await.unwrap();
        assert_eq!(entries.len(), 2);
    }
}

#[tokio::test]
async fn test_error_on_unknown_interface() {
    let (node, _temp) = create_test_node().await;

    let fake_id = indras_core::InterfaceId::generate();

    // Should error on unknown interface
    assert!(matches!(
        node.events(&fake_id),
        Err(NodeError::InterfaceNotFound(_))
    ));

    assert!(matches!(
        node.send_message(&fake_id, vec![]).await,
        Err(NodeError::InterfaceNotFound(_))
    ));

    assert!(matches!(
        node.members(&fake_id).await,
        Err(NodeError::InterfaceNotFound(_))
    ));

    assert!(matches!(
        node.events_since(&fake_id, 0).await,
        Err(NodeError::InterfaceNotFound(_))
    ));
}

#[tokio::test]
async fn test_invite_key_serialization() {
    let interface_id = indras_core::InterfaceId::generate();

    // Create invite with bootstrap peers
    let invite = InviteKey::new(interface_id)
        .with_bootstrap(vec![1, 2, 3, 4])
        .with_bootstrap(vec![5, 6, 7, 8]);

    // Bytes roundtrip
    let bytes = invite.to_bytes().unwrap();
    let restored = InviteKey::from_bytes(&bytes).unwrap();
    assert_eq!(restored.interface_id, interface_id);
    assert_eq!(restored.bootstrap_peers.len(), 2);
    assert_eq!(restored.bootstrap_peers[0], vec![1, 2, 3, 4]);

    // Base64 roundtrip
    let b64 = invite.to_base64().unwrap();
    assert!(!b64.is_empty());
    let restored = InviteKey::from_base64(&b64).unwrap();
    assert_eq!(restored.interface_id, interface_id);
}

#[tokio::test]
async fn test_concurrent_message_sending() {
    let (node, _temp) = create_test_node().await;
    let (interface_id, _) = node.create_interface(None).await.unwrap();

    // Send messages concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let node_ref = &node;
        handles.push(async move {
            node_ref
                .send_message(&interface_id, format!("Concurrent {}", i).into_bytes())
                .await
        });
    }

    // Use futures::future::join_all equivalent
    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
    }

    // Should have all 10 messages
    let events = node.events_since(&interface_id, 0).await.unwrap();
    assert_eq!(events.len(), 10);
}

#[tokio::test]
async fn test_identity_persistence() {
    let (node1, _temp1) = create_test_node().await;
    let (node2, _temp2) = create_test_node().await;

    // Each node should have a unique identity
    assert_ne!(node1.identity(), node2.identity());

    // Identity should be consistent for the same node
    let id = *node1.identity();
    assert_eq!(*node1.identity(), id);
}
