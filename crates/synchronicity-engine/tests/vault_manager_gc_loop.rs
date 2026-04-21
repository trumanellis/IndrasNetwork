//! Phase-4 smoke test for `brisk-orbiting-lantern`.
//!
//! `VaultManager::start_gc_loop` must spawn a background task that
//! actually calls `Vault::gc_blobs` on every interval tick. We verify
//! this end-to-end with a tiny interval + a real unreferenced blob in
//! the shared store: the loop should delete it within a few ticks.
//!
//! Correctness of `Vault::gc_blobs` itself is already covered by
//! `indras-sync-engine/tests/gc_full_cycle.rs` — this test only
//! confirms the scheduler fires.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::vault::Vault;
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
async fn start_gc_loop_deletes_unreferenced_blobs_periodically() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("gcLoop", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    // Attach a vault so VaultManager has something to iterate.
    let (vault, _invite) = Vault::create(
        &net,
        "gc-loop-test",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let vm = VaultManager::new(tmp_data.path().to_path_buf())
        .await
        .expect("VaultManager::new");
    vm.ensure_vault(net.as_ref(), vault.realm(), Some("gc-loop-test"))
        .await
        .expect("ensure_vault");
    let vm_arc = Arc::new(vm);

    // Seed a blob that nothing in either DAG references.
    let orphan_ref = vm_arc
        .blob_store()
        .store(b"orphaned content")
        .await
        .expect("store orphan blob");
    assert!(
        vm_arc.blob_store().exists(&orphan_ref).await.unwrap(),
        "orphan blob must exist before GC runs"
    );

    // 50 ms interval gives the loop ~3 chances inside the 300 ms budget.
    vm_arc.start_gc_loop(Duration::from_millis(50));

    // Poll until the orphan is deleted or we time out.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if !vm_arc.blob_store().exists(&orphan_ref).await.unwrap() {
            break;
        }
        if Instant::now() >= deadline {
            panic!("gc loop never collected the orphan blob");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // Stopping the loop must not panic and the handle slot clears.
    vm_arc.stop_gc_loop();

    net.stop().await.ok();
}
