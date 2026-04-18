//! Headless drive of the Sync-panel commit pipeline.
//!
//! Mirrors what `synchronicity_engine::components::sync_panel::commit_for_agent`
//! does, minus Dioxus: bind a folder with a `LocalWorkspaceIndex` +
//! `WorkspaceWatcher`, write a file, wait for the index to observe it,
//! snapshot into a `PatchManifest`, call `try_land`, and assert the
//! resulting `Changeset` is present in the team realm's `BraidDag`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{PatchManifest, RealmBraid};
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::{FolderLock, LocalWorkspaceIndex, WorkspaceWatcher};
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
async fn commit_lands_changeset_in_team_realm_dag() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault_root = TempDir::new().unwrap();
    let tmp_agent_folder = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "commit-flow-vault",
        tmp_vault_root.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Bind the agent folder to a local index + fs watcher.
    let _lock = FolderLock::acquire(tmp_agent_folder.path()).expect("lock");
    let index = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent_folder.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    let _watcher = WorkspaceWatcher::start(Arc::clone(&index)).expect("watcher");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let bytes = b"fn commit_this() { 42 }\n";
    let file_path = tmp_agent_folder.path().join("work.rs");
    tokio::fs::write(&file_path, bytes).await.unwrap();

    // Wait for the index to observe the write.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if index.get("work.rs").await.is_some() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "index never observed work.rs"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Snapshot → PatchManifest → try_land. crates=[] ⇒ verification
    // no-op returning green Evidence (matches MVP behavior).
    let files = index.snapshot_all().await;
    assert!(!files.is_empty(), "snapshot should see at least work.rs");
    let manifest = PatchManifest::new(files);
    let pq = net.node().pq_identity();
    let user_id = pq.user_id();
    let change_id = vault
        .realm()
        .try_land(
            "add work.rs".into(),
            manifest,
            Vec::new(),
            tmp_agent_folder.path().to_path_buf(),
            user_id,
            pq,
        )
        .await
        .expect("try_land");

    // The resulting Changeset must live in the vault's braid DAG.
    let dag = vault.dag().read().await;
    assert!(
        dag.contains(&change_id),
        "DAG must contain the changeset we just landed"
    );

    // The Changeset's manifest must reference the content hash we wrote.
    let cs = dag.get(&change_id).cloned().expect("changeset");
    assert_eq!(cs.patch.files.len(), 1, "manifest carries one file");
    assert_eq!(cs.patch.files[0].path, "work.rs");
    let expected_hash = *blake3::hash(bytes).as_bytes();
    assert_eq!(cs.patch.files[0].hash, expected_hash);

    net.stop().await.ok();
}
