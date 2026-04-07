//! End-to-end integration tests for IndrasNetwork peering.
//!
//! These tests boot real `IndrasNetwork` instances with transport enabled
//! and verify peer discovery, event broadcasting, and lifecycle management.
//!
//! Marked `#[ignore]` by default because they require network transport
//! (iroh relay). Run explicitly with:
//!
//! ```sh
//! cargo test -p indras-network --test peering_e2e -- --ignored
//! ```

use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use indras_network::{IndrasNetwork, PeerEvent};

/// Create an IndrasNetwork in a temp dir with a fresh identity.
async fn create_test_network(name: &str) -> (Arc<IndrasNetwork>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::builder()
        .data_dir(tmp.path())
        .display_name(name)
        .build()
        .await
        .expect("failed to build IndrasNetwork");
    network.start().await.expect("failed to start network");
    (network, tmp)
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_runtime_boot_and_identity() {
    let (network, _tmp) = create_test_network("Alice").await;

    // Identity should be set
    assert!(!network.identity_code().is_empty());

    // Peer list starts empty
    assert!(network.peers().is_empty());
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_connect_by_code_emits_conversation_opened() {
    let (alice, _tmp_a) = create_test_network("Alice").await;
    let (bob, _tmp_b) = create_test_network("Bob").await;

    // Subscribe to Bob's events BEFORE connecting (with snapshot to avoid race)
    let (mut bob_rx, _initial_peers) = bob.peer_events_with_snapshot();

    // Get Alice's identity URI
    let alice_uri = alice.identity_uri();

    // Bob connects to Alice by code
    let result = bob.connect_by_code(&alice_uri).await;
    assert!(result.is_ok(), "connect_by_code failed: {:?}", result.err());

    let (realm, peer_info) = result.unwrap();
    assert!(!realm.id().as_bytes().iter().all(|&b| b == 0));
    assert!(!peer_info.display_name.is_empty(), "peer display_name should be non-empty");

    // New fields should have defaults for a fresh connection
    assert_eq!(peer_info.sentiment, 0);

    // Bob should have received a ConversationOpened event
    let event = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match bob_rx.recv().await {
                Ok(PeerEvent::ConversationOpened { realm_id, peer }) => {
                    return (realm_id, peer);
                }
                Ok(_) => continue,
                Err(e) => panic!("event recv error: {e}"),
            }
        }
    })
    .await
    .expect("timed out waiting for ConversationOpened");

    assert_eq!(event.0, realm.id());
    assert!(!event.1.display_name.is_empty());
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_contact_poller_detects_peer() {
    let (alice, _tmp_a) = create_test_network("Alice").await;
    let (bob, _tmp_b) = create_test_network("Bob").await;

    // Subscribe to Alice's events BEFORE any connections
    let mut alice_rx = alice.peer_events();

    // Both sides connect to each other.
    let alice_uri = alice.identity_uri();
    let bob_uri = bob.identity_uri();

    bob.connect_by_code(&alice_uri).await
        .expect("Bob→Alice connect_by_code failed");
    alice.connect_by_code(&bob_uri).await
        .expect("Alice→Bob connect_by_code failed");

    // Wait for Alice's contact poller to detect Bob (polls every 2s).
    let peers_changed = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match alice_rx.recv().await {
                Ok(PeerEvent::PeersChanged { peers }) => {
                    if !peers.is_empty() {
                        return peers;
                    }
                }
                Ok(_) => continue,
                Err(e) => panic!("event recv error: {e}"),
            }
        }
    })
    .await
    .expect("timed out waiting for PeersChanged on Alice");

    assert!(!peers_changed.is_empty(), "Alice should see at least one peer");
    let peer = &peers_changed[0];
    assert_eq!(peer.sentiment, 0, "default sentiment should be 0");

    // The watch channel should also reflect the peer
    let alice_peers = alice.peers();
    assert!(!alice_peers.is_empty(), "watch channel should reflect detected peer");
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_shutdown_is_graceful() {
    let tmp = TempDir::new().unwrap();
    let network = IndrasNetwork::builder()
        .data_dir(tmp.path())
        .display_name("Shutdown Test")
        .build()
        .await
        .expect("build failed");
    network.start().await.expect("start failed");

    // Shutdown should complete without panic or hang
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        network.stop(),
    )
    .await
    .expect("shutdown timed out");

    assert!(result.is_ok(), "shutdown failed: {:?}", result.err());

    // Double-shutdown should return AlreadyShutDown
    let result2 = network.stop().await;
    assert!(result2.is_err(), "double stop should error");
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_is_first_run_detection() {
    let tmp = TempDir::new().unwrap();

    // Fresh dir should be first run
    assert!(IndrasNetwork::is_first_run(tmp.path()));

    // After creating identity, no longer first run
    let network = IndrasNetwork::builder()
        .data_dir(tmp.path())
        .display_name("First Run")
        .build()
        .await
        .expect("build failed");
    network.stop().await.unwrap();

    assert!(!IndrasNetwork::is_first_run(tmp.path()));
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_create_persists_identity_to_disk() {
    let tmp = TempDir::new().unwrap();

    // Fresh dir → first run
    assert!(IndrasNetwork::is_first_run(tmp.path()));

    // Create identity and capture the code
    let network = IndrasNetwork::builder()
        .data_dir(tmp.path())
        .display_name("Persistent")
        .build()
        .await
        .unwrap();

    let identity_code = network.identity_code();
    assert!(!identity_code.is_empty(), "identity code should be non-empty");

    // Stop persists state to disk
    network.stop().await.unwrap();

    // After create+stop, keystore files exist on disk → no longer first run
    assert!(
        !IndrasNetwork::is_first_run(tmp.path()),
        "identity should persist on disk after create + stop"
    );
}

// ── Contact management E2E tests ─────────────────────────────────

#[tokio::test]
#[ignore] // requires network transport
async fn test_block_contact_leaves_realms() {
    let (alice, _tmp_a) = create_test_network("Alice").await;
    let (bob, _tmp_b) = create_test_network("Bob").await;

    // Connect both sides
    let alice_uri = alice.identity_uri();
    let bob_uri = bob.identity_uri();
    bob.connect_by_code(&alice_uri).await.expect("Bob→Alice failed");
    let (realm, _) = alice.connect_by_code(&bob_uri).await.expect("Alice→Bob failed");

    // Subscribe to Alice's events before blocking
    let mut alice_rx = alice.peer_events();

    // Alice blocks Bob
    let bob_id = bob.identity().id();
    let left_realms = alice.block_contact(&bob_id).await
        .expect("block_contact failed");

    // The DM realm should be among the left realms
    assert!(
        left_realms.contains(&realm.id()),
        "block should have left the DM realm; left: {:?}, expected: {:?}",
        left_realms,
        realm.id()
    );

    // Should receive a PeerBlocked event
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match alice_rx.recv().await {
                Ok(PeerEvent::PeerBlocked { member_id, left_realms }) => {
                    return (member_id, left_realms);
                }
                Ok(_) => continue,
                Err(e) => panic!("event recv error: {e}"),
            }
        }
    })
    .await
    .expect("timed out waiting for PeerBlocked event");

    assert_eq!(event.0, bob_id);
    assert!(!event.1.is_empty());
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_sentiment_roundtrip() {
    let (alice, _tmp_a) = create_test_network("Alice").await;
    let (bob, _tmp_b) = create_test_network("Bob").await;

    // Connect both sides so Bob is in Alice's contacts
    let alice_uri = alice.identity_uri();
    let bob_uri = bob.identity_uri();
    bob.connect_by_code(&alice_uri).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_uri).await.expect("Alice→Bob failed");

    let bob_id = bob.identity().id();

    // Subscribe to Alice's events
    let mut alice_rx = alice.peer_events();

    // Default sentiment should be 0
    let sentiment = alice.get_sentiment(&bob_id).await.expect("get_sentiment failed");
    assert_eq!(sentiment, Some(0), "default sentiment should be 0");

    // Update sentiment to +1
    alice.update_sentiment(&bob_id, 1).await.expect("update_sentiment failed");

    // Read back
    let sentiment = alice.get_sentiment(&bob_id).await.expect("get_sentiment failed");
    assert_eq!(sentiment, Some(1), "sentiment should be 1 after update");

    // Should receive a SentimentChanged event
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match alice_rx.recv().await {
                Ok(PeerEvent::SentimentChanged { member_id, sentiment }) => {
                    return (member_id, sentiment);
                }
                Ok(_) => continue,
                Err(e) => panic!("event recv error: {e}"),
            }
        }
    })
    .await
    .expect("timed out waiting for SentimentChanged event");

    assert_eq!(event.0, bob_id);
    assert_eq!(event.1, 1);

    // Update sentiment to -1
    alice.update_sentiment(&bob_id, -1).await.expect("update_sentiment to -1 failed");
    let sentiment = alice.get_sentiment(&bob_id).await.expect("get_sentiment failed");
    assert_eq!(sentiment, Some(-1), "sentiment should be -1 after update");

    // Verify clamping: out-of-range values should be clamped
    alice.update_sentiment(&bob_id, 100).await.expect("update_sentiment clamp failed");
    let sentiment = alice.get_sentiment(&bob_id).await.expect("get_sentiment failed");
    assert_eq!(sentiment, Some(1), "sentiment should be clamped to 1");
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_peer_info_includes_sentiment() {
    let (alice, _tmp_a) = create_test_network("Alice").await;
    let (bob, _tmp_b) = create_test_network("Bob").await;

    // Connect both sides
    let alice_uri = alice.identity_uri();
    let bob_uri = bob.identity_uri();
    bob.connect_by_code(&alice_uri).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_uri).await.expect("Alice→Bob failed");

    let bob_id = bob.identity().id();

    // Set sentiment to +1
    alice.update_sentiment(&bob_id, 1).await.expect("update_sentiment failed");

    // Subscribe to Alice's events and wait for a PeersChanged that includes Bob
    let mut alice_rx = alice.peer_events();
    let peers = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match alice_rx.recv().await {
                Ok(PeerEvent::PeersChanged { peers }) => {
                    if peers.iter().any(|p| p.member_id == bob_id) {
                        return peers;
                    }
                }
                Ok(_) => continue,
                Err(e) => panic!("event recv error: {e}"),
            }
        }
    })
    .await
    .expect("timed out waiting for PeersChanged with Bob");

    let bob_peer = peers.iter().find(|p| p.member_id == bob_id).unwrap();
    assert_eq!(bob_peer.sentiment, 1, "PeerInfo should reflect updated sentiment");
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_contact_entry_and_relayable() {
    let (alice, _tmp_a) = create_test_network("Alice").await;
    let (bob, _tmp_b) = create_test_network("Bob").await;

    // Connect both sides
    let alice_uri = alice.identity_uri();
    let bob_uri = bob.identity_uri();
    bob.connect_by_code(&alice_uri).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_uri).await.expect("Alice→Bob failed");

    let bob_id = bob.identity().id();

    // Get full contact entry
    let entry = alice.get_contact_entry(&bob_id).await
        .expect("get_contact_entry failed")
        .expect("Bob should be in Alice's contacts");

    assert_eq!(entry.sentiment, 0);
    assert!(entry.relayable, "default relayable should be true");

    // Set relayable to false
    alice.set_relayable(&bob_id, false).await.expect("set_relayable failed");

    let entry = alice.get_contact_entry(&bob_id).await
        .expect("get_contact_entry failed")
        .expect("Bob should still be in contacts");
    assert!(!entry.relayable, "relayable should be false after update");
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_remove_contact() {
    let (alice, _tmp_a) = create_test_network("Alice").await;
    let (bob, _tmp_b) = create_test_network("Bob").await;

    // Connect both sides
    let alice_uri = alice.identity_uri();
    let bob_uri = bob.identity_uri();
    bob.connect_by_code(&alice_uri).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_uri).await.expect("Alice→Bob failed");

    let bob_id = bob.identity().id();

    // Remove Bob (soft — no realm cascade)
    let removed = alice.remove_contact(&bob_id).await.expect("remove_contact failed");
    assert!(removed, "Bob should have been removed");

    // Bob should no longer be in contacts
    let entry = alice.get_contact_entry(&bob_id).await.expect("get_contact_entry failed");
    assert!(entry.is_none(), "Bob should not be in contacts after removal");

    // Removing again should return false
    let removed_again = alice.remove_contact(&bob_id).await.expect("remove_contact failed");
    assert!(!removed_again, "second removal should return false");
}
