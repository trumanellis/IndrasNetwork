//! End-to-end test: PQ signature round-trip through the vault commit path.
//!
//! Proves that:
//! 1. Vault::create publishes the peer's PQ verifying key to the key directory.
//! 2. A changeset committed via try_land is signed with ML-DSA-65.
//! 3. The signature verifies against the key from the peer key directory.
//! 4. The signature fails against a different peer's key.

use std::path::Path;
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{PatchManifest, RealmBraid};
use indras_sync_engine::peer_key_directory::PeerKeyDirectory;
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::LocalWorkspaceIndex;
use tempfile::TempDir;

async fn build_blob_store(data_dir: &Path) -> Arc<BlobStore> {
    let cfg = BlobStoreConfig {
        base_dir: data_dir.join("shared-blobs"),
        ..Default::default()
    };
    Arc::new(BlobStore::new(cfg).await.expect("BlobStore::new"))
}

async fn build_network(name: &str, data_dir: &Path) -> Arc<IndrasNetwork> {
    IndrasNetwork::builder()
        .data_dir(data_dir)
        .display_name(name)
        .build()
        .await
        .unwrap_or_else(|e| panic!("build_network({name}): {e}"))
}

#[tokio::test]
async fn pq_signature_round_trip() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();
    let tmp_agent = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    // 1. Create vault — this publishes our PQ verifying key.
    let (vault, _invite) = Vault::create(
        &net,
        "pq-sig-test",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // 2. Write a file and commit via try_land (which signs the changeset).
    let idx = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    let content = b"fn quantum_safe() -> bool { true }\n";
    tokio::fs::write(tmp_agent.path().join("lib.rs"), content)
        .await
        .unwrap();
    idx.ingest_bytes("lib.rs", content).await.unwrap();

    let manifest = PatchManifest::new(idx.snapshot_all().await);
    let pq = net.node().pq_identity();
    let user_id = pq.user_id();

    let change_id = vault
        .realm()
        .try_land(
            "feat: quantum-safe function".into(),
            manifest,
            Vec::new(),
            tmp_agent.path().to_path_buf(),
            user_id,
            pq,
        )
        .await
        .expect("try_land");

    // 3. Read the changeset from the DAG.
    let dag = vault.realm().braid_dag().await.expect("braid_dag");
    let dag_read = dag.read().await;
    let cs = dag_read.get(&change_id).expect("changeset must exist");

    // 4. Verify it's signed (not a dummy).
    assert!(cs.is_signed(), "changeset must carry a real PQ signature");

    // 5. Load the peer key directory and look up our verifying key.
    let key_dir = vault
        .realm()
        .document::<PeerKeyDirectory>("peer-keys")
        .await
        .expect("peer-keys document");
    let key_dir_read = key_dir.read().await;
    let our_pubkey = key_dir_read
        .get(&user_id)
        .expect("our verifying key must be in the directory");

    // 6. Verify the signature.
    assert!(
        cs.verify_signature(&our_pubkey),
        "changeset signature must verify against the author's published key"
    );

    // 7. Verify that a different key fails.
    let imposter = indras_crypto::PQIdentity::generate();
    assert!(
        !cs.verify_signature(&imposter.verifying_key()),
        "signature must NOT verify against a different peer's key"
    );

    // 8. Verify UserId binding: blake3(vk_bytes) == user_id.
    let vk_bytes = key_dir_read.keys.get(&user_id).unwrap();
    let derived = *blake3::hash(vk_bytes).as_bytes();
    assert_eq!(
        derived, user_id,
        "UserId must equal blake3(verifying_key_bytes)"
    );

    eprintln!();
    eprintln!("=== PQ Signature E2E ===");
    eprintln!("  ChangeId:       {change_id}");
    eprintln!("  Author (UserId): {}", hex::encode(user_id));
    eprintln!("  Signature size:  {} bytes (ML-DSA-65)", cs.signature.to_bytes().len());
    eprintln!("  Signed:          {}", cs.is_signed());
    eprintln!("  Verified:        {}", cs.verify_signature(&our_pubkey));
    eprintln!("  Peers in dir:    {}", key_dir_read.len());
    eprintln!("========================");

    net.stop().await.ok();
}
