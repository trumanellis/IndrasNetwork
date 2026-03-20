//! Integration tests for the NodeLog feature
//!
//! Verifies that node lifecycle events (start, interface creation, stop) are
//! correctly written to the append-only node log with a valid hash chain.

use tempfile::TempDir;

use indras_node::{IndrasNode, NodeConfig};
use indras_storage::NodeEvent;

async fn create_test_node() -> (IndrasNode, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig::with_data_dir(temp_dir.path());
    let node = IndrasNode::new(config).await.unwrap();
    (node, temp_dir)
}

#[tokio::test]
async fn test_node_log_lifecycle_events() {
    let (node, _temp) = create_test_node().await;

    // Capture identity fingerprint before starting
    let expected_fingerprint = *node.identity().public_key().as_bytes();

    // Start → logs NodeStarted
    node.start().await.unwrap();

    // Create interface → logs InterfaceCreated
    let (interface_id, _invite_key) = node.create_interface(Some("Test")).await.unwrap();

    // Stop → logs NodeStopped
    node.stop().await.unwrap();

    // Read the log from storage
    let node_log = node.storage().node_log();

    // Should have exactly 3 entries (sequences 0, 1, 2)
    assert_eq!(node_log.current_sequence(), 3);

    // Entry 0: NodeStarted with correct identity fingerprint
    let entry0 = node_log.read_entry(0).await.unwrap().expect("entry 0 missing");
    assert_eq!(entry0.sequence, 0);
    if let NodeEvent::NodeStarted { identity_fingerprint } = entry0.event {
        assert_eq!(identity_fingerprint, expected_fingerprint);
    } else {
        panic!("entry 0: expected NodeStarted, got {:?}", entry0.event);
    }

    // Entry 1: InterfaceCreated with correct interface_id and name "Test"
    let entry1 = node_log.read_entry(1).await.unwrap().expect("entry 1 missing");
    assert_eq!(entry1.sequence, 1);
    if let NodeEvent::InterfaceCreated { interface_id: logged_id, name } = entry1.event {
        assert_eq!(logged_id, interface_id);
        assert_eq!(name.as_deref(), Some("Test"));
    } else {
        panic!("entry 1: expected InterfaceCreated, got {:?}", entry1.event);
    }

    // Entry 2: NodeStopped
    let entry2 = node_log.read_entry(2).await.unwrap().expect("entry 2 missing");
    assert_eq!(entry2.sequence, 2);
    assert!(
        matches!(entry2.event, NodeEvent::NodeStopped),
        "entry 2: expected NodeStopped, got {:?}",
        entry2.event
    );

    // Verify hash chain is intact across all three entries
    assert!(
        node_log.verify_chain(0, 2).await.unwrap(),
        "hash chain verification failed"
    );

    // Genesis entry must have zero prev_hash
    assert_eq!(entry0.prev_hash, [0u8; 32]);

    // All entries have positive timestamps
    assert!(entry0.timestamp_millis > 0);
    assert!(entry1.timestamp_millis > 0);
    assert!(entry2.timestamp_millis > 0);
}
