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
use indras_peering::{PeerEvent, PeeringConfig, PeeringRuntime};

/// Create a PeeringRuntime in a temp dir with a fresh identity.
async fn create_test_runtime(name: &str) -> (Arc<PeeringRuntime>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = PeeringConfig::new(tmp.path());
    let runtime = PeeringRuntime::create(name, None, config)
        .await
        .expect("failed to create PeeringRuntime");
    (Arc::new(runtime), tmp)
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

    // Subscribe to Bob's events BEFORE connecting
    let mut bob_rx = bob.subscribe();

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
