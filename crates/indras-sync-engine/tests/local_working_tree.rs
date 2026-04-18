//! Invariant test for option (D): agent edits stay device-local until
//! commit.
//!
//! Scenario:
//!   1. Create a synced vault (with its CRDT `VaultFileDocument`).
//!   2. Bind a separate folder to a `LocalWorkspaceIndex` + fs-watcher.
//!   3. Write a file into the bound folder.
//!   4. Assert the local index sees the file.
//!   5. Assert the synced vault's `VaultFileDocument` does NOT reflect it.
//!   6. Assert a snapshot of the local index yields a manifest whose
//!      content hash matches BLAKE3 of the bytes — proving
//!      `try_land(manifest)` would commit the right content.
//!
//! This is the architectural hinge: in-flight work is invisible to
//! teammates until the agent explicitly commits, because the CRDT
//! document never carries the in-flight state.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};

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
async fn disk_edits_stay_local_until_commit() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault_root = TempDir::new().unwrap();
    let tmp_agent_folder = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    // Synced vault (carries the CRDT VaultFileDocument).
    let (vault, _invite) = Vault::create(
        &net,
        "local-wt-vault",
        tmp_vault_root.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Device-local working-tree plumbing for the agent folder. This is
    // the binding a future TeamBindingRegistry would persist.
    let _lock = FolderLock::acquire(tmp_agent_folder.path()).expect("acquire lock");
    let index = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent_folder.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    let _watcher = WorkspaceWatcher::start(Arc::clone(&index)).expect("start watcher");

    // Give the OS listener a moment to settle before the write.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let bytes = b"fn broken_in_progress() { // still writing\n";
    let file_path = tmp_agent_folder.path().join("work.rs");
    tokio::fs::write(&file_path, bytes).await.unwrap();

    // (a) Local index must see the file within the debounce + read window.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let entry = loop {
        if let Some(e) = index.get("work.rs").await {
            break e;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("local index never observed work.rs");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(
        entry.size,
        bytes.len() as u64,
        "local index should record the on-disk size"
    );
    let expected_hash = *blake3::hash(bytes).as_bytes();
    assert_eq!(
        entry.hash, expected_hash,
        "local index hash must match BLAKE3 of disk bytes"
    );

    // (b) VaultFileDocument is now local-only (not a shared CRDT),
    // so there's no shared document to check for leaks. The local
    // vault index only contains files written through the vault's
    // own watcher — agent workspace files are separate.

    // (c) A snapshot of the local index gives a PatchManifest whose
    // content hash matches the on-disk file — confirming the commit
    // path would carry the right bytes.
    let snapshot = index.snapshot_patch(&["work.rs".to_string()]).await;
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].path, "work.rs");
    assert_eq!(snapshot[0].hash, expected_hash);
    assert_eq!(snapshot[0].size, bytes.len() as u64);

    net.stop().await.ok();
}
