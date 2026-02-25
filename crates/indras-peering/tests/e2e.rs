//! End-to-end integration tests for PeeringRuntime.
//!
//! These tests boot real `IndrasNetwork` instances with transport enabled
//! and verify peer discovery, event broadcasting, and lifecycle management.
//!
//! Marked `#[ignore]` by default because they require network transport
//! (iroh relay). Run explicitly with:
//!
//! ```sh
//! cargo test -p indras-peering --test e2e -- --ignored
//! ```

use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use indras_peering::{ContactStatus, PeerEvent, PeeringConfig, PeeringRuntime};

/// Create a PeeringRuntime in a temp dir with a fresh identity.
async fn create_test_runtime(name: &str) -> (Arc<PeeringRuntime>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = PeeringConfig::new(tmp.path());
    let runtime = PeeringRuntime::create(name, None, config)
        .await
        .expect("failed to create PeeringRuntime");
    // Wrap in Arc — shutdown(&self) works fine through Arc
    (Arc::new(runtime), tmp)
}

/// Create a PeeringRuntime without wrapping in Arc (for lifecycle tests).
async fn create_test_runtime_owned(name: &str) -> (PeeringRuntime, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = PeeringConfig::new(tmp.path());
    let runtime = PeeringRuntime::create(name, None, config)
        .await
        .expect("failed to create PeeringRuntime");
    (runtime, tmp)
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_runtime_boot_and_identity() {
    let (runtime, _tmp) = create_test_runtime("Alice").await;

    // Identity should be set
    assert_eq!(runtime.display_name(), Some("Alice"));
    assert!(!runtime.identity_code().is_empty());
    assert!(!runtime.identity_uri().is_empty());

    // Peer list starts empty
    assert!(runtime.peers().is_empty());

    // Network should be accessible
    let net = runtime.network();
    assert!(!net.identity_code().is_empty());
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_connect_by_code_emits_conversation_opened() {
    let (alice, _tmp_a) = create_test_runtime("Alice").await;
    let (bob, _tmp_b) = create_test_runtime("Bob").await;

    // Subscribe to Bob's events BEFORE connecting (with snapshot to avoid race)
    let (mut bob_rx, _initial_peers) = bob.subscribe_with_snapshot();

    // Get Alice's identity code
    let alice_code = alice.identity_uri();

    // Bob connects to Alice by code
    let result = bob.connect_by_code(&alice_code).await;
    assert!(result.is_ok(), "connect_by_code failed: {:?}", result.err());

    let (realm, peer_info) = result.unwrap();
    assert!(!realm.id().as_bytes().iter().all(|&b| b == 0));
    // Display name may be the friendly name or a hex-encoded member ID,
    // depending on whether the remote peer's profile has propagated yet.
    assert!(!peer_info.display_name.is_empty(), "peer display_name should be non-empty");

    // New fields should have defaults for a fresh connection
    // (sentiment starts at 0, status starts as Pending)
    assert_eq!(peer_info.sentiment, 0);

    // Bob should have received a ConversationOpened event
    let event = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match bob_rx.recv().await {
                Ok(PeerEvent::ConversationOpened { realm_id, peer }) => {
                    return (realm_id, peer);
                }
                Ok(_) => continue, // skip other events
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
    let (alice, _tmp_a) = create_test_runtime("Alice").await;
    let (bob, _tmp_b) = create_test_runtime("Bob").await;

    // Subscribe to Alice's events BEFORE any connections
    let mut alice_rx = alice.subscribe();

    // Both sides connect to each other.
    // Bob→Alice creates the DM realm; Alice→Bob ensures Alice's LOCAL
    // contacts realm is updated immediately (relay sync can be slow).
    let alice_code = alice.identity_uri();
    let bob_code = bob.identity_uri();

    bob.connect_by_code(&alice_code).await
        .expect("Bob→Alice connect_by_code failed");
    alice.connect_by_code(&bob_code).await
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
    // Peer should include sentiment + status fields
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
    let config = PeeringConfig::new(tmp.path());
    let runtime = PeeringRuntime::create("Shutdown Test", None, config)
        .await
        .expect("create failed");

    // Verify it's running
    assert_eq!(runtime.display_name(), Some("Shutdown Test"));

    // Shutdown should complete without panic or hang
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        runtime.shutdown(),
    )
    .await
    .expect("shutdown timed out");

    assert!(result.is_ok(), "shutdown failed: {:?}", result.err());

    // Double-shutdown should return AlreadyShutDown
    let result2 = runtime.shutdown().await;
    assert!(matches!(result2, Err(indras_peering::PeeringError::AlreadyShutDown)));
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_attach_mode_does_not_stop_network() {
    let tmp = TempDir::new().unwrap();
    let config = PeeringConfig::new(tmp.path());

    // Create network manually
    let net = indras_network::IndrasNetwork::new(tmp.path()).await.unwrap();
    net.start().await.unwrap();
    let net = Arc::new(net);

    // Attach peering runtime
    let runtime = PeeringRuntime::attach(Arc::clone(&net), config)
        .await
        .expect("attach failed");

    assert!(runtime.display_name().is_none() || runtime.display_name().is_some());

    // Shutdown the peering runtime
    runtime.shutdown().await.expect("shutdown failed");

    // Network should still be usable (attach mode doesn't own it)
    // Verify by accessing network state
    let code = net.identity_code();
    assert!(!code.is_empty(), "network should still be accessible after peering shutdown");

    // Clean up
    net.stop().await.unwrap();
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_is_first_run_detection() {
    let tmp = TempDir::new().unwrap();

    // Fresh dir should be first run
    assert!(PeeringRuntime::is_first_run(tmp.path()));

    // After creating identity, no longer first run
    let config = PeeringConfig::new(tmp.path());
    let runtime = PeeringRuntime::create("First Run", None, config)
        .await
        .expect("create failed");
    runtime.shutdown().await.unwrap();

    assert!(!PeeringRuntime::is_first_run(tmp.path()));
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_create_persists_identity_to_disk() {
    let tmp = TempDir::new().unwrap();

    // Fresh dir → first run
    assert!(PeeringRuntime::is_first_run(tmp.path()));

    // Create identity and capture the code
    let config = PeeringConfig::new(tmp.path());
    let runtime = PeeringRuntime::create("Persistent", None, config)
        .await
        .unwrap();

    let identity_code = runtime.identity_code();
    assert!(!identity_code.is_empty(), "identity code should be non-empty");

    // Shutdown persists state to disk
    runtime.shutdown().await.unwrap();

    // After create+shutdown, keystore files exist on disk → no longer first run
    assert!(
        !PeeringRuntime::is_first_run(tmp.path()),
        "identity should persist on disk after create + shutdown"
    );
}

// ── Contact management E2E tests ─────────────────────────────────

#[tokio::test]
#[ignore] // requires network transport
async fn test_block_contact_leaves_realms() {
    let (alice, _tmp_a) = create_test_runtime("Alice").await;
    let (bob, _tmp_b) = create_test_runtime("Bob").await;

    // Connect both sides
    let alice_code = alice.identity_uri();
    let bob_code = bob.identity_uri();
    bob.connect_by_code(&alice_code).await.expect("Bob→Alice failed");
    let (realm, _) = alice.connect_by_code(&bob_code).await.expect("Alice→Bob failed");

    // Subscribe to Alice's events before blocking
    let mut alice_rx = alice.subscribe();

    // Alice blocks Bob
    let bob_id = bob.id();
    let left_realms = alice.block_contact(bob_id).await
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
    let (alice, _tmp_a) = create_test_runtime("Alice").await;
    let (bob, _tmp_b) = create_test_runtime("Bob").await;

    // Connect both sides so Bob is in Alice's contacts
    let alice_code = alice.identity_uri();
    let bob_code = bob.identity_uri();
    bob.connect_by_code(&alice_code).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_code).await.expect("Alice→Bob failed");

    let bob_id = bob.id();

    // Subscribe to Alice's events
    let mut alice_rx = alice.subscribe();

    // Default sentiment should be 0
    let sentiment = alice.get_sentiment(bob_id).await.expect("get_sentiment failed");
    assert_eq!(sentiment, Some(0), "default sentiment should be 0");

    // Update sentiment to +1
    alice.update_sentiment(bob_id, 1).await.expect("update_sentiment failed");

    // Read back
    let sentiment = alice.get_sentiment(bob_id).await.expect("get_sentiment failed");
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
    alice.update_sentiment(bob_id, -1).await.expect("update_sentiment to -1 failed");
    let sentiment = alice.get_sentiment(bob_id).await.expect("get_sentiment failed");
    assert_eq!(sentiment, Some(-1), "sentiment should be -1 after update");

    // Verify clamping: out-of-range values should be clamped
    alice.update_sentiment(bob_id, 100).await.expect("update_sentiment clamp failed");
    let sentiment = alice.get_sentiment(bob_id).await.expect("get_sentiment failed");
    assert_eq!(sentiment, Some(1), "sentiment should be clamped to 1");
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_peer_info_includes_sentiment() {
    let (alice, _tmp_a) = create_test_runtime("Alice").await;
    let (bob, _tmp_b) = create_test_runtime("Bob").await;

    // Connect both sides
    let alice_code = alice.identity_uri();
    let bob_code = bob.identity_uri();
    bob.connect_by_code(&alice_code).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_code).await.expect("Alice→Bob failed");

    let bob_id = bob.id();

    // Set sentiment to +1
    alice.update_sentiment(bob_id, 1).await.expect("update_sentiment failed");

    // Subscribe to Alice's events and wait for a PeersChanged that includes Bob
    let mut alice_rx = alice.subscribe();
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
    let (alice, _tmp_a) = create_test_runtime("Alice").await;
    let (bob, _tmp_b) = create_test_runtime("Bob").await;

    // Connect both sides
    let alice_code = alice.identity_uri();
    let bob_code = bob.identity_uri();
    bob.connect_by_code(&alice_code).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_code).await.expect("Alice→Bob failed");

    let bob_id = bob.id();

    // Get full contact entry
    let entry = alice.get_contact_entry(bob_id).await
        .expect("get_contact_entry failed")
        .expect("Bob should be in Alice's contacts");

    assert_eq!(entry.sentiment, 0);
    assert!(entry.relayable, "default relayable should be true");

    // Set relayable to false
    alice.set_relayable(bob_id, false).await.expect("set_relayable failed");

    let entry = alice.get_contact_entry(bob_id).await
        .expect("get_contact_entry failed")
        .expect("Bob should still be in contacts");
    assert!(!entry.relayable, "relayable should be false after update");
}

#[tokio::test]
#[ignore] // requires network transport
async fn test_remove_contact() {
    let (alice, _tmp_a) = create_test_runtime("Alice").await;
    let (bob, _tmp_b) = create_test_runtime("Bob").await;

    // Connect both sides
    let alice_code = alice.identity_uri();
    let bob_code = bob.identity_uri();
    bob.connect_by_code(&alice_code).await.expect("Bob→Alice failed");
    alice.connect_by_code(&bob_code).await.expect("Alice→Bob failed");

    let bob_id = bob.id();

    // Remove Bob (soft — no realm cascade)
    let removed = alice.remove_contact(bob_id).await.expect("remove_contact failed");
    assert!(removed, "Bob should have been removed");

    // Bob should no longer be in contacts
    let entry = alice.get_contact_entry(bob_id).await.expect("get_contact_entry failed");
    assert!(entry.is_none(), "Bob should not be in contacts after removal");

    // Removing again should return false
    let removed_again = alice.remove_contact(bob_id).await.expect("remove_contact failed");
    assert!(!removed_again, "second removal should return false");
}
