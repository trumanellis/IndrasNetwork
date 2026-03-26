//! Integration tests for the embedded relay service in IndrasNode.
//!
//! Verifies that a RelayClient can connect to an IndrasNode's endpoint
//! and perform relay protocol operations through the bi-stream path.
//!
//! This exercises the full wiring:
//! ```text
//! RelayClient.connect() -> QUIC -> IndrasProtocolHandler -> accept_bi loop
//!   -> bi_stream_tx channel -> bi_rx router in IndrasNode
//!   -> RelayService.handle_bi_stream() -> response back through bi-stream
//! ```

use ed25519_dalek::SigningKey;
use indras_core::InterfaceId;
use indras_node::{IndrasNode, NodeConfig};
use indras_transport::protocol::StorageTier;
use indras_transport::relay_client::RelayClient;
use iroh::SecretKey;
use tempfile::TempDir;

/// Generate a random Ed25519 signing key (avoids rand_core version conflict)
fn random_signing_key() -> SigningKey {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    SigningKey::from_bytes(&bytes)
}

/// Create a started test node with a temp directory
async fn create_started_node() -> (IndrasNode, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig::with_data_dir(temp_dir.path());
    let node = IndrasNode::new(config).await.unwrap();
    node.start().await.unwrap();
    (node, temp_dir)
}

#[tokio::test]
async fn test_relay_bi_stream_routed_end_to_end() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    // 1. Create and start an IndrasNode (starts the embedded relay)
    let (node, _temp) = create_started_node().await;

    // 2. Get the node's endpoint address
    let endpoint_addr = node
        .endpoint_addr()
        .await
        .expect("started node must have an endpoint address");

    // 3. Create a RelayClient with a fresh stranger identity
    let signing_key = random_signing_key();
    let transport_secret = SecretKey::generate(&mut rand::rng());
    let client = RelayClient::new(signing_key, transport_secret);

    // 4. Connect to the node's endpoint through the bi-stream path
    let mut session = client
        .connect(endpoint_addr)
        .await
        .expect("should connect to node's embedded relay");

    // 5. Authenticate — as a stranger, should get Public tier
    let auth_ack = session
        .authenticate()
        .await
        .expect("should authenticate with relay");
    assert!(auth_ack.authenticated, "stranger should be authenticated");
    assert_eq!(
        auth_ack.granted_tiers,
        vec![StorageTier::Public],
        "stranger should only get Public tier"
    );

    // 6. Register an interface
    let iface = InterfaceId::new([0x42; 32]);
    let reg_ack = session
        .register(vec![iface])
        .await
        .expect("should register interface");
    assert_eq!(reg_ack.accepted.len(), 1);
    assert!(reg_ack.rejected.is_empty());

    // 7. Store an event in Public tier
    let data = b"test-event-via-bi-stream".to_vec();
    let store_ack = session
        .store_event(StorageTier::Public, iface, data.clone())
        .await
        .expect("should store event");
    assert!(store_ack.accepted, "public-tier store should be accepted");

    // 8. Retrieve the event
    let delivery = session
        .retrieve(iface, None, Some(StorageTier::Public))
        .await
        .expect("should retrieve events");
    assert_eq!(delivery.events.len(), 1, "should have exactly one event");
    assert_eq!(
        delivery.events[0].encrypted_event, data,
        "retrieved data should match stored data"
    );

    // 9. Ping — verify the stream is still alive
    let latency = session.ping().await.expect("should ping");
    assert!(
        latency.as_millis() < 5000,
        "local ping should be fast, got {latency:?}"
    );

    // 10. Clean up
    node.stop().await.unwrap();
}
