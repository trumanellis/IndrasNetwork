//! Integration test: braid `PatchManifest` → vault apply → disk materialization.
//!
//! Single-peer scenario (no transport needed): write file v1, capture its
//! manifest, overwrite with v2, then `apply_manifest(v1_manifest)` and assert
//! the file on disk is v1 again. Proves that `Vault::apply_manifest` is the
//! working "checkout" primitive for the braid layer.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{PatchFile, PatchManifest};
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
async fn apply_manifest_checks_out_older_version() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;
    let (vault, _invite) = Vault::create(
        &net,
        "braid-roundtrip-vault",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // v1: write initial content, capture manifest.
    let v1 = b"alpha contents v1";
    vault
        .write_file_content("alpha.md", v1)
        .await
        .expect("write v1");
    let v1_hash = *blake3::hash(v1).as_bytes();
    let v1_manifest = PatchManifest::new(vec![PatchFile {
        path: "alpha.md".into(),
        hash: v1_hash,
        size: v1.len() as u64,
    }]);

    // v2: overwrite with new content.
    let v2 = b"alpha contents v2 -- newer";
    vault
        .write_file_content("alpha.md", v2)
        .await
        .expect("write v2");
    let on_disk_v2 = tokio::fs::read(tmp_vault.path().join("alpha.md"))
        .await
        .unwrap();
    assert_eq!(on_disk_v2, v2, "disk should reflect v2 after overwrite");

    // Checkout v1 via apply_manifest — file must revert.
    vault
        .apply_manifest(&v1_manifest)
        .await
        .expect("apply_manifest v1");

    // Give watcher a moment to settle; then assert disk bytes = v1.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let on_disk = tokio::fs::read(tmp_vault.path().join("alpha.md"))
        .await
        .unwrap();
    assert_eq!(on_disk, v1, "apply_manifest should restore v1 to disk");

    net.stop().await.ok();
}

#[tokio::test]
async fn apply_manifest_missing_blob_errors() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("B", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;
    let (vault, _invite) = Vault::create(
        &net,
        "missing-blob-vault",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Manifest referencing a hash we never stored.
    let ghost_manifest = PatchManifest::new(vec![PatchFile {
        path: "ghost.md".into(),
        hash: [0xAB; 32],
        size: 42,
    }]);

    let err = vault.apply_manifest(&ghost_manifest).await;
    assert!(
        err.is_err(),
        "apply_manifest must fail when blob is unavailable"
    );

    net.stop().await.ok();
}
