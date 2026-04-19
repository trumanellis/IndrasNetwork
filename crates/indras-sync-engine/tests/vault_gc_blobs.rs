//! Phase 5: `Vault::gc_blobs` retains blobs referenced by any outer
//! DAG peer HEAD / changeset or any inner-braid entry, and deletes
//! everything else from the shared blob store.

use std::path::Path;
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig, ContentRef};
use indras_sync_engine::vault::Vault;
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
async fn gc_blobs_retains_synced_and_deletes_dangling() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "gc-blobs",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Seed a synced file (becomes a live reference via the outer DAG).
    let live_payload = b"kept alive by a changeset\n";
    vault
        .write_file_content("keep.md", live_payload)
        .await
        .expect("write keep.md");
    if let Some(w) = vault.watcher_ref() {
        w.dirty_paths.insert("keep.md".to_string());
    }
    vault
        .sync("add keep.md".into(), Some("seed".into()))
        .await
        .expect("sync");

    let live_ref = ContentRef::from_data(live_payload);
    assert!(
        blob.load(&live_ref).await.is_ok(),
        "live blob must exist before GC"
    );

    // Store a dangling blob that no changeset or HEAD points at.
    let dangling_payload = b"no one references me\n";
    let dangling_ref = blob
        .store(dangling_payload)
        .await
        .expect("store dangling");
    assert!(
        blob.load(&dangling_ref).await.is_ok(),
        "dangling blob must exist before GC"
    );

    let result = vault.gc_blobs().await.expect("gc_blobs");
    assert!(result.deleted_count >= 1, "dangling blob should be deleted");
    assert!(result.retained_count >= 1, "live blob should be retained");

    assert!(
        blob.load(&live_ref).await.is_ok(),
        "referenced blob must survive GC"
    );
    assert!(
        blob.load(&dangling_ref).await.is_err(),
        "dangling blob must be deleted by GC"
    );
}
