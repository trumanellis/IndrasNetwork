//! Phase 5 full-cycle integration: ref counting → rollup → staged
//! deletion → blob GC as one narrative.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig, ContentRef};
use indras_sync_engine::braid::StagedDeletionSet;
use indras_sync_engine::vault::Vault;
use indras_sync_engine::ContentAddr;
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
async fn ref_counting_rollup_staging_and_gc_full_cycle() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "gc-full-cycle",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Two successive versions of the same file, each synced.
    let v1 = b"version one\n";
    let v2 = b"version two\n";
    let v1_ref = ContentRef::from_data(v1);
    let v2_ref = ContentRef::from_data(v2);
    let v1_addr = ContentAddr::from(v1_ref);
    let v2_addr = ContentAddr::from(v2_ref);

    vault.write_file_content("note.md", v1).await.expect("write v1");
    if let Some(w) = vault.watcher_ref() {
        w.dirty_paths.insert("note.md".to_string());
    }
    let c1 = vault.sync("v1".into(), None).await.expect("sync v1");

    vault.write_file_content("note.md", v2).await.expect("write v2");
    if let Some(w) = vault.watcher_ref() {
        w.dirty_paths.insert("note.md".to_string());
    }
    let c2 = vault.sync("v2".into(), None).await.expect("sync v2");

    // Reference counting: both addrs currently reachable.
    let before_rollup = vault.dag().read().await.all_referenced_addrs();
    assert!(before_rollup.contains(&v1_addr), "v1 kept alive by c1");
    assert!(before_rollup.contains(&v2_addr), "v2 kept alive by c2 + HEAD");

    // Rollup outer DAG to c2 — c1 goes away, so does v1 from the ref set.
    let freed: HashSet<ContentAddr> = vault
        .dag()
        .try_update(|d| Ok::<_, indras_network::error::IndraError>(d.rollup(c2)))
        .await
        .expect("rollup c2");

    assert!(freed.contains(&v1_addr), "v1 freed after c1 pruned");
    assert!(!freed.contains(&v2_addr), "v2 still referenced by HEAD + c2");
    assert!(!vault.dag().read().await.contains(&c1), "c1 pruned");
    assert!(vault.dag().read().await.contains(&c2), "c2 retained");

    // Stage the freed addrs with a short grace period.
    let mut staging = StagedDeletionSet::new();
    let staged_at = 1_000_i64;
    let grace_ms = 500_i64;
    for a in &freed {
        staging.stage(*a, staged_at, grace_ms);
    }
    assert_eq!(staging.len(), freed.len());

    // Within the grace window: nothing ready.
    assert!(staging
        .take_ready_for_deletion(staged_at + grace_ms - 1)
        .is_empty());

    // Past the grace window: all entries ready.
    let ready = staging.take_ready_for_deletion(staged_at + grace_ms + 1);
    assert_eq!(ready.len(), freed.len());
    assert!(staging.is_empty(), "ready entries drained from staging");

    // gc_blobs now deletes v1 (unreferenced post-rollup) and keeps v2.
    let result = vault.gc_blobs().await.expect("gc_blobs");
    assert!(result.deleted_count >= 1);
    assert!(
        blob.load(&v1_ref).await.is_err(),
        "v1 blob must be deleted after rollup + gc"
    );
    assert!(
        blob.load(&v2_ref).await.is_ok(),
        "v2 blob must survive rollup + gc (still in HEAD index)"
    );
}
