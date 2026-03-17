//! Integration tests for the relay protocol
//!
//! Spins up a RelayNode in-process and uses RelayClient to exercise
//! the full protocol: auth, register, store, retrieve, contacts sync.

use ed25519_dalek::SigningKey;
use indras_core::InterfaceId;
use indras_relay::{RelayConfig, RelayNode};
use indras_transport::protocol::StorageTier;
use indras_transport::relay_client::RelayClient;
use iroh::SecretKey;
use tempfile::TempDir;

/// Generate a random Ed25519 signing key
fn random_signing_key() -> SigningKey {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    SigningKey::from_bytes(&bytes)
}

/// Create a relay config for testing with a specific owner
fn test_config(data_dir: &std::path::Path, owner_player_id: Option<[u8; 32]>) -> RelayConfig {
    let mut config = RelayConfig::default();
    config.data_dir = data_dir.to_path_buf();
    // Use port 0 to get a random available port for the admin API
    config.admin_bind = "127.0.0.1:0".parse().unwrap();
    if let Some(pid) = owner_player_id {
        config.owner_player_id = Some(pid.iter().map(|b| format!("{b:02x}")).collect());
    }
    config
}

#[tokio::test]
async fn test_owner_auth_register_store_retrieve() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let dir = TempDir::new().unwrap();

    // Create owner identity
    let owner_signing = random_signing_key();
    let owner_player_id = owner_signing.verifying_key().to_bytes();
    let owner_transport = SecretKey::generate(&mut rand::rng());

    // Start relay with owner configured
    let config = test_config(dir.path(), Some(owner_player_id));
    let relay = RelayNode::new(config).await.unwrap();
    let shutdown = relay.shutdown_token();
    let (relay_addr, _handle) = relay.start().await.unwrap();

    // Connect as owner
    let client = RelayClient::new(owner_signing, owner_transport);
    let mut session = client.connect(relay_addr).await.unwrap();

    // Authenticate — should get Self_ tier
    let auth_ack = session.authenticate().await.unwrap();
    assert!(auth_ack.authenticated);
    assert!(auth_ack.granted_tiers.contains(&StorageTier::Self_));
    assert!(auth_ack.granted_tiers.contains(&StorageTier::Connections));
    assert!(auth_ack.granted_tiers.contains(&StorageTier::Public));

    // Register an interface
    let iface = InterfaceId::new([0x42; 32]);
    let reg_ack = session.register(vec![iface]).await.unwrap();
    assert_eq!(reg_ack.accepted.len(), 1);
    assert!(reg_ack.rejected.is_empty());

    // Store an event in Self_ tier
    let test_data = b"encrypted-event-data".to_vec();
    let store_ack = session
        .store_event(StorageTier::Self_, iface, test_data.clone())
        .await
        .unwrap();
    assert!(store_ack.accepted);

    // Retrieve from Self_ tier
    let delivery = session
        .retrieve(iface, None, Some(StorageTier::Self_))
        .await
        .unwrap();
    assert_eq!(delivery.events.len(), 1);
    assert_eq!(delivery.events[0].encrypted_event, test_data);

    // Ping
    let rtt = session.ping().await.unwrap();
    assert!(rtt.as_millis() < 5000); // should be very fast locally

    shutdown.cancel();
}

#[tokio::test]
async fn test_contact_gets_connections_tier() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let dir = TempDir::new().unwrap();

    // Owner and contact identities
    let owner_signing = random_signing_key();
    let owner_player_id = owner_signing.verifying_key().to_bytes();
    let owner_transport = SecretKey::generate(&mut rand::rng());

    let contact_signing = random_signing_key();
    let contact_player_id = contact_signing.verifying_key().to_bytes();
    let contact_transport = SecretKey::generate(&mut rand::rng());

    // Start relay with owner configured
    let config = test_config(dir.path(), Some(owner_player_id));
    let relay = RelayNode::new(config).await.unwrap();
    let shutdown = relay.shutdown_token();
    let (relay_addr, _handle) = relay.start().await.unwrap();

    // Owner connects and syncs contacts
    {
        let client = RelayClient::new(owner_signing, owner_transport);
        let mut session = client.connect(relay_addr.clone()).await.unwrap();
        let auth_ack = session.authenticate().await.unwrap();
        assert!(auth_ack.authenticated);

        let sync_ack = session
            .sync_contacts(vec![contact_player_id])
            .await
            .unwrap();
        assert!(sync_ack.accepted);
        assert_eq!(sync_ack.contact_count, 1);
    }

    // Contact connects — should get Connections tier
    {
        let client = RelayClient::new(contact_signing, contact_transport);
        let mut session = client.connect(relay_addr).await.unwrap();
        let auth_ack = session.authenticate().await.unwrap();
        assert!(auth_ack.authenticated);
        assert!(auth_ack.granted_tiers.contains(&StorageTier::Connections));
        assert!(auth_ack.granted_tiers.contains(&StorageTier::Public));
        assert!(!auth_ack.granted_tiers.contains(&StorageTier::Self_));
    }

    shutdown.cancel();
}

#[tokio::test]
async fn test_stranger_gets_public_tier_only() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let dir = TempDir::new().unwrap();

    let owner_signing = random_signing_key();
    let owner_player_id = owner_signing.verifying_key().to_bytes();

    let stranger_signing = random_signing_key();
    let stranger_transport = SecretKey::generate(&mut rand::rng());

    // Start relay with owner configured
    let config = test_config(dir.path(), Some(owner_player_id));
    let relay = RelayNode::new(config).await.unwrap();
    let shutdown = relay.shutdown_token();
    let (relay_addr, _handle) = relay.start().await.unwrap();

    // Stranger connects — should only get Public tier
    let client = RelayClient::new(stranger_signing, stranger_transport);
    let mut session = client.connect(relay_addr).await.unwrap();
    let auth_ack = session.authenticate().await.unwrap();
    assert!(auth_ack.authenticated);
    assert_eq!(auth_ack.granted_tiers, vec![StorageTier::Public]);

    // Stranger can store in Public tier
    let iface = InterfaceId::new([0x99; 32]);
    let store_ack = session
        .store_event(StorageTier::Public, iface, b"public-data".to_vec())
        .await
        .unwrap();
    assert!(store_ack.accepted);

    // Stranger cannot store in Self_ tier
    let store_ack = session
        .store_event(StorageTier::Self_, iface, b"self-data".to_vec())
        .await
        .unwrap();
    assert!(!store_ack.accepted);

    shutdown.cancel();
}
