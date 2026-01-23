//! Integration tests for indras-transport
//!
//! Tests real peer-to-peer connections using iroh.
//!
//! NOTE: These tests require full network connectivity and may fail in CI
//! environments without proper network access. Run with:
//!   cargo test -p indras-transport --test integration -- --ignored
//! to run the full network tests.

use indras_transport::{
    ConnectionManager, ConnectionConfig, IrohIdentity, SecretKey,
    frame_message, parse_framed_message, WireMessage, PresenceInfo,
};

/// Test connection manager creation and basic properties
#[tokio::test]
async fn test_connection_manager_basic() {
    let secret = SecretKey::generate(&mut rand::rng());
    let config = ConnectionConfig::default();

    let manager = ConnectionManager::new(secret.clone(), config)
        .await
        .expect("Failed to create manager");

    // Verify identity matches secret key
    assert_eq!(
        manager.local_identity(),
        IrohIdentity::new(secret.public())
    );

    // Verify no connections initially
    assert_eq!(manager.connected_peers().len(), 0);

    // Verify stats
    let stats = manager.stats();
    assert_eq!(stats.active_connections, 0);
    assert_eq!(stats.total_connections, 0);
    assert_eq!(stats.max_connections, 100);

    manager.close().await;
}

/// Test that endpoint address contains expected information
#[tokio::test]
async fn test_endpoint_addr() {
    let secret = SecretKey::generate(&mut rand::rng());
    let config = ConnectionConfig::default();

    let manager = ConnectionManager::new(secret.clone(), config)
        .await
        .expect("Failed to create manager");

    let addr = manager.endpoint_addr();

    // Address should have our public key as ID
    assert_eq!(addr.id, secret.public());

    manager.close().await;
}

/// Test wire message framing roundtrip
#[test]
fn test_wire_message_framing() {
    // Test Ping/Pong
    let ping = WireMessage::Ping(42);
    let framed = frame_message(&ping).expect("Frame failed");
    let parsed = parse_framed_message(&framed).expect("Parse failed");
    match parsed {
        WireMessage::Ping(n) => assert_eq!(n, 42),
        _ => panic!("Expected Ping"),
    }

    let pong = WireMessage::Pong(42);
    let framed = frame_message(&pong).expect("Frame failed");
    let parsed = parse_framed_message(&framed).expect("Parse failed");
    match parsed {
        WireMessage::Pong(n) => assert_eq!(n, 42),
        _ => panic!("Expected Pong"),
    }
}

/// Test presence info creation and serialization
#[test]
fn test_presence_info_serialization() {
    let secret = SecretKey::generate(&mut rand::rng());
    let id = IrohIdentity::new(secret.public());

    let presence = PresenceInfo::new(id)
        .with_name("TestPeer");

    let msg = WireMessage::PresenceAnnounce(presence.clone());
    let framed = frame_message(&msg).expect("Frame failed");
    let parsed = parse_framed_message(&framed).expect("Parse failed");

    match parsed {
        WireMessage::PresenceAnnounce(p) => {
            assert_eq!(p.peer_id, id);
            assert_eq!(p.display_name, Some("TestPeer".to_string()));
            assert!(p.accepting_connections);
        }
        _ => panic!("Expected PresenceAnnounce"),
    }
}

/// Test connection config defaults
#[test]
fn test_connection_config_defaults() {
    let config = ConnectionConfig::default();
    assert_eq!(config.max_connections, 100);
    assert_eq!(config.connect_timeout_ms, 10_000);
    assert_eq!(config.idle_timeout_ms, 60_000);
    assert!(config.accept_incoming);
}

// ============================================================================
// Full network tests - require actual network connectivity
// Run with: cargo test --test integration -- --ignored
// ============================================================================

/// Test that two peers can connect to each other
///
/// This test requires full network connectivity and may fail in CI.
#[tokio::test]
#[ignore = "Requires full network connectivity - run with --ignored"]
async fn test_two_peers_connect() {
    let secret_a = SecretKey::generate(&mut rand::rng());
    let secret_b = SecretKey::generate(&mut rand::rng());

    let config = ConnectionConfig {
        connect_timeout_ms: 60_000, // Longer timeout for network tests
        ..Default::default()
    };

    let manager_a = ConnectionManager::new(secret_a, config.clone())
        .await
        .expect("Failed to create manager A");

    let manager_b = ConnectionManager::new(secret_b, config)
        .await
        .expect("Failed to create manager B");

    // Wait for endpoints to be online
    manager_a.endpoint().online().await;
    manager_b.endpoint().online().await;

    // Get B's address
    let addr_b = manager_b.endpoint_addr();
    println!("Peer B address: {:?}", addr_b);

    // Spawn B's accept loop
    let endpoint_b = manager_b.endpoint().clone();
    let accept_handle = tokio::spawn(async move {
        if let Some(incoming) = endpoint_b.accept().await {
            let _ = incoming.await;
        }
    });

    // Give B time to start accepting
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // A connects to B
    let conn = manager_a.connect(addr_b).await
        .expect("Failed to connect A to B");

    // Verify the connection
    assert!(conn.close_reason().is_none(), "Connection should be open");

    // Verify peer IDs match
    let remote_id = conn.remote_id();
    assert_eq!(
        IrohIdentity::new(remote_id),
        manager_b.local_identity(),
        "Remote ID should match B's identity"
    );

    // Clean up
    accept_handle.abort();
    manager_a.close().await;
    manager_b.close().await;
}

/// Test bidirectional stream communication
#[tokio::test]
#[ignore = "Requires full network connectivity - run with --ignored"]
async fn test_bidirectional_stream() {
    let secret_a = SecretKey::generate(&mut rand::rng());
    let secret_b = SecretKey::generate(&mut rand::rng());

    let config = ConnectionConfig {
        connect_timeout_ms: 60_000,
        ..Default::default()
    };

    let manager_a = ConnectionManager::new(secret_a, config.clone())
        .await
        .expect("Failed to create manager A");

    let manager_b = ConnectionManager::new(secret_b, config)
        .await
        .expect("Failed to create manager B");

    manager_a.endpoint().online().await;
    manager_b.endpoint().online().await;

    let addr_b = manager_b.endpoint_addr();

    // Use a channel to signal when A has read the response
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();

    // B accepts and echoes messages
    let endpoint_b = manager_b.endpoint().clone();
    let accept_handle = tokio::spawn(async move {
        let incoming = endpoint_b.accept().await.expect("No incoming");
        let conn = incoming.await.expect("Accept failed");
        let (mut send, mut recv) = conn.accept_bi().await.expect("Accept bi failed");

        let msg = recv.read_to_end(1024).await.expect("Read failed");
        let mut response = b"echo: ".to_vec();
        response.extend_from_slice(&msg);
        send.write_all(&response).await.expect("Write failed");
        send.finish().expect("Finish failed");

        // Wait for A to signal it has read the response before dropping connection
        let _ = done_rx.await;

        msg
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // A connects and sends a message
    let conn = manager_a.connect(addr_b).await.expect("Connect failed");
    let (mut send, mut recv) = conn.open_bi().await.expect("Open bi failed");

    send.write_all(b"Hello from A!").await.expect("Write failed");
    send.finish().expect("Finish failed");

    let response = recv.read_to_end(1024).await.expect("Read failed");
    assert_eq!(response, b"echo: Hello from A!");

    // Signal B that we're done reading
    let _ = done_tx.send(());

    let received = accept_handle.await.expect("Accept task failed");
    assert_eq!(received, b"Hello from A!");

    manager_a.close().await;
    manager_b.close().await;
}
