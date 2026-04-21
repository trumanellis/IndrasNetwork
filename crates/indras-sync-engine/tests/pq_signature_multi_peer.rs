//! Multi-instance PQ signature verification over real iroh transport.
//!
//! Two peers (A, B) connect via iroh. A commits a signed changeset.
//! B receives it via CRDT sync and verifies the signature using A's
//! verifying key from the shared peer key directory.
//!
//! Run explicitly (requires iroh transport):
//! ```sh
//! cargo test -p indras-sync-engine --test pq_signature_multi_peer -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{ChangeId, PatchManifest, RealmBraid};
use indras_sync_engine::peer_key_directory::PeerKeyDirectory;
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::LocalWorkspaceIndex;
use tempfile::TempDir;
use tokio::time::sleep;

async fn build_network(name: &str, data_dir: &Path) -> Arc<IndrasNetwork> {
    let network = IndrasNetwork::builder()
        .data_dir(data_dir)
        .display_name(name)
        .build()
        .await
        .unwrap_or_else(|e| panic!("build_network({name}): {e}"));
    network
        .start()
        .await
        .unwrap_or_else(|e| panic!("start({name}): {e}"));
    network
}

async fn build_blob_store(data_dir: &Path) -> Arc<BlobStore> {
    let cfg = BlobStoreConfig {
        base_dir: data_dir.join("shared-blobs"),
        ..Default::default()
    };
    Arc::new(BlobStore::new(cfg).await.expect("BlobStore::new"))
}

/// Poll until a realm's peer key directory contains `target_user_id`.
async fn wait_for_peer_key(
    realm: &indras_network::Realm,
    target_user_id: [u8; 32],
    timeout: Duration,
) -> bool {
    let end = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(doc) = realm.document::<PeerKeyDirectory>("peer-keys").await {
            if doc.read().await.get(&target_user_id).is_some() {
                return true;
            }
        }
        if tokio::time::Instant::now() >= end {
            return false;
        }
        sleep(Duration::from_millis(250)).await;
    }
}

/// Poll until a realm's BraidDag contains `id`.
async fn wait_for_changeset(
    realm: &indras_network::Realm,
    id: ChangeId,
    timeout: Duration,
) -> bool {
    let end = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(dag) = realm.braid_dag().await {
            if dag.read().await.contains(&id) {
                return true;
            }
        }
        if tokio::time::Instant::now() >= end {
            return false;
        }
        sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test]
#[ignore] // requires iroh transport
async fn peer_b_verifies_peer_a_signature() {
    eprintln!("\n=== Multi-Peer PQ Signature Verification ===\n");

    // --- Setup: two peers, one vault ---
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_a_agent = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    let net_a = build_network("A", tmp_a_data.path()).await;
    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "pq-multi-peer",
        tmp_a_vault.path().to_path_buf(),
        Arc::clone(&blob_a),
    )
    .await
    .expect("A: Vault::create");

    let user_a = net_a.node().pq_identity().user_id();
    eprintln!("  Peer A UserId: {}", hex::encode(user_a));

    let net_b = build_network("B", tmp_b_data.path()).await;
    let blob_b = build_blob_store(tmp_b_data.path()).await;
    let invite_str = invite.to_string();
    let vault_b = Vault::join(
        &net_b,
        &invite_str,
        tmp_b_vault.path().to_path_buf(),
        Arc::clone(&blob_b),
    )
    .await
    .expect("B: Vault::join");

    let user_b = net_b.node().pq_identity().user_id();
    eprintln!("  Peer B UserId: {}", hex::encode(user_b));

    // Wait for both peers to see each other.
    vault_b
        .await_members(2, Duration::from_secs(15))
        .await
        .expect("B: await_members");
    eprintln!("  Peers connected.");

    // --- Step 1: Verify peer key directory syncs between peers ---

    // B should see A's verifying key in the directory.
    let a_key_arrived = wait_for_peer_key(vault_b.realm(), user_a, Duration::from_secs(15)).await;
    assert!(
        a_key_arrived,
        "B must see A's verifying key in the peer key directory"
    );
    eprintln!("  B sees A's key in peer-keys directory.");

    // A should see B's verifying key in the directory.
    let b_key_arrived = wait_for_peer_key(vault_a.realm(), user_b, Duration::from_secs(15)).await;
    assert!(
        b_key_arrived,
        "A must see B's verifying key in the peer key directory"
    );
    eprintln!("  A sees B's key in peer-keys directory.");

    // --- Step 2: A commits a signed changeset ---

    let idx = Arc::new(LocalWorkspaceIndex::new(
        tmp_a_agent.path().to_path_buf(),
        Arc::clone(&blob_a),
    ));
    let content = b"pub fn quantum_safe() -> bool { true }\n";
    tokio::fs::write(tmp_a_agent.path().join("lib.rs"), content)
        .await
        .unwrap();
    idx.ingest_bytes("lib.rs", content).await.unwrap();

    let manifest = PatchManifest::new(idx.snapshot_all().await);
    let pq_a = net_a.node().pq_identity();
    let change_id = vault_a
        .realm()
        .try_land(
            "feat: quantum-safe function".into(),
            manifest.into(),
            Vec::new(),
            tmp_a_agent.path().to_path_buf(),
            user_a,
            pq_a,
        )
        .await
        .expect("A: try_land");
    eprintln!("  A committed: {change_id}");

    // --- Step 3: B receives the changeset via CRDT sync ---

    let cs_arrived =
        wait_for_changeset(vault_b.realm(), change_id, Duration::from_secs(15)).await;
    assert!(
        cs_arrived,
        "B must receive A's changeset via CRDT sync"
    );
    eprintln!("  B received changeset via sync.");

    // --- Step 4: B verifies A's signature ---

    let dag_b = vault_b.realm().braid_dag().await.expect("B: braid_dag");
    let dag_read = dag_b.read().await;
    let cs = dag_read.get(&change_id).expect("changeset on B");

    // It must be signed.
    assert!(cs.is_signed(), "changeset must carry a real signature");

    // Look up A's verifying key from B's copy of the peer key directory.
    let key_dir_b = vault_b
        .realm()
        .document::<PeerKeyDirectory>("peer-keys")
        .await
        .expect("B: peer-keys");
    let key_dir_read = key_dir_b.read().await;
    let a_pubkey = key_dir_read
        .get(&user_a)
        .expect("A's key must be in B's peer directory");

    // Verify!
    let verified = cs.verify_signature(&a_pubkey);
    assert!(verified, "B must verify A's changeset signature");
    eprintln!("  B verified A's signature: PASS");

    // Negative check: B's own key should NOT verify A's changeset.
    let b_pubkey = key_dir_read
        .get(&user_b)
        .expect("B's own key in directory");
    assert!(
        !cs.verify_signature(&b_pubkey),
        "A's changeset must NOT verify with B's key"
    );
    eprintln!("  B's key on A's changeset: REJECT (correct)");

    // --- Summary ---
    eprintln!();
    eprintln!("  === Results ===");
    eprintln!("  Peers in directory: {}", key_dir_read.len());
    eprintln!("  Changeset author:   {}", hex::encode(cs.author));
    eprintln!("  Signature size:     {} bytes", cs.signature.to_bytes().len());
    eprintln!("  Cross-peer verify:  PASS");
    eprintln!("  Wrong-key reject:   PASS");
    eprintln!();
    eprintln!("=== Multi-Peer PQ Signature Verification Complete ===\n");

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}
