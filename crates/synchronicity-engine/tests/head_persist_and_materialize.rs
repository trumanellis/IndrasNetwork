//! Integration test: after a commit, HEAD is published to the DAG's
//! peer_heads and files materialize at the vault root.

use std::path::Path;
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{PatchManifest, RealmBraid};
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::LocalWorkspaceIndex;
use synchronicity_engine::team::publish_and_materialize_head;
use synchronicity_engine::vault_manager::VaultManager;
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
async fn head_persists_and_files_materialize() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_agent = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    // Create vault + register with VaultManager.
    let vault_dir = tmp_data.path().join("vaults").join("head-test");
    tokio::fs::create_dir_all(&vault_dir).await.unwrap();
    let (vault, _invite) = Vault::create(
        &net,
        "head-test",
        vault_dir.clone(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let vm = VaultManager::new(tmp_data.path().to_path_buf())
        .await
        .expect("VaultManager::new");
    vm.ensure_vault(net.as_ref(), vault.realm(), Some("head-test"))
        .await
        .expect("ensure_vault");

    // Agent writes a file.
    let idx = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    let content = b"fn materialized() { 42 }\n";
    tokio::fs::write(tmp_agent.path().join("lib.rs"), content)
        .await
        .unwrap();
    idx.ingest_bytes("lib.rs", content).await.unwrap();

    // Commit.
    let manifest = PatchManifest::new(idx.snapshot_all().await);
    let manifest_for_publish = manifest.clone();
    let pq = net.node().pq_identity();
    let user_id = pq.user_id();
    let change_id = vault
        .realm()
        .try_land(
            "add lib.rs".into(),
            manifest,
            Vec::new(),
            tmp_agent.path().to_path_buf(),
            user_id,
            pq,
        )
        .await
        .expect("try_land");

    // Publish HEAD + materialize.
    publish_and_materialize_head(&vm, vault.realm(), change_id, &manifest_for_publish, user_id).await;

    // Assert 1: DAG peer_heads carries the committed change_id.
    let dag = vault.dag().read().await;
    let peer_head = dag.peer_head(&user_id);
    assert!(
        peer_head.is_some(),
        "DAG peer_heads must carry our HEAD after publish"
    );
    assert_eq!(
        peer_head.unwrap().head, change_id,
        "peer_heads HEAD must match the committed change_id"
    );

    // Assert 2: lib.rs materialized at the vault root.
    let materialized_path = vault_dir.join("lib.rs");
    assert!(
        materialized_path.exists(),
        "lib.rs must be materialized at the vault root"
    );
    let on_disk = tokio::fs::read(&materialized_path).await.unwrap();
    assert_eq!(
        on_disk, content,
        "materialized content must match the committed bytes"
    );

    net.stop().await.ok();
}
