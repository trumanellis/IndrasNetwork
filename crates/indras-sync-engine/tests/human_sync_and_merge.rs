//! Integration tests for the human sync path and merge consent flow.
//!
//! Tests `Vault::sync()`, `pending_forks()`, `merge_from_peer()`, and
//! `diff_fork()` using in-process vaults (no iroh transport required).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::RealmBraid;
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
async fn sync_creates_changeset_in_dag() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "sync-test",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Write a file directly (bypassing watcher for determinism).
    let content = b"hello from human sync\n";
    vault
        .write_file_content("hello.md", content)
        .await
        .expect("write");

    // Manually mark the path as dirty (watcher would do this in real use).
    if let Some(ref w) = vault.watcher_ref() {
        w.dirty_paths.insert("hello.md".to_string());
    }

    // Sync should create a changeset.
    let change_id = vault
        .sync("add hello.md".into(), Some("first sync".into()))
        .await
        .expect("sync");

    // The DAG should contain the changeset.
    let dag = vault.dag().read().await;
    assert!(dag.contains(&change_id), "DAG must contain the synced changeset");

    // Our peer head should be updated.
    let user_id = vault.user_id();
    let ps = dag.peer_head(&user_id).expect("peer head must be set");
    assert_eq!(ps.head, change_id);
    assert_eq!(ps.head_index.len(), 1);
    assert!(ps.head_index.get(&indras_sync_engine::LogicalPath::new("hello.md")).is_some());

    // The evidence should be Human.
    let cs = dag.get(&change_id).expect("changeset");
    match &cs.evidence {
        indras_sync_engine::Evidence::Human { approved_by, message, .. } => {
            assert_eq!(approved_by, &user_id);
            assert_eq!(message.as_deref(), Some("first sync"));
        }
        _ => panic!("expected Evidence::Human"),
    }

    net.stop().await.ok();
}

#[tokio::test]
async fn sequential_syncs_chain_parents() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _) = Vault::create(
        &net,
        "seq-sync",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("create");

    // First sync.
    vault.write_file_content("a.md", b"aaa").await.unwrap();
    if let Some(ref w) = vault.watcher_ref() {
        w.dirty_paths.insert("a.md".to_string());
    }
    let id1 = vault.sync("add a.md".into(), None).await.expect("sync 1");

    // Second sync.
    vault.write_file_content("b.md", b"bbb").await.unwrap();
    if let Some(ref w) = vault.watcher_ref() {
        w.dirty_paths.insert("b.md".to_string());
    }
    let id2 = vault.sync("add b.md".into(), None).await.expect("sync 2");

    // id2 should parent on id1.
    let dag = vault.dag().read().await;
    let cs2 = dag.get(&id2).expect("changeset 2");
    assert!(
        cs2.parents.contains(&id1),
        "second sync must parent on first; parents = {:?}",
        cs2.parents
    );

    net.stop().await.ok();
}

#[tokio::test]
async fn pending_forks_and_merge() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _) = Vault::create(
        &net,
        "fork-test",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("create");

    let user_a = vault.user_id();

    // Simulate peer B's changeset directly in the DAG.
    // Store B's blob so merge can materialize it.
    let peer_b: [u8; 32] = [2u8; 32];
    let b_content = b"peer B content";
    vault.blob_store().store(b_content).await.expect("store B blob");
    let fake_hash = *blake3::hash(b_content).as_bytes();
    let index_b = indras_sync_engine::SymlinkIndex::from_iter([(
        indras_sync_engine::LogicalPath::new("from_b.md"),
        indras_sync_engine::ContentAddr::new(fake_hash, b_content.len() as u64),
    )]);
    let evidence_b = indras_sync_engine::Evidence::human(peer_b, Some("B sync".into()));
    let cs_b = indras_sync_engine::braid::Changeset::new_unsigned(
        peer_b,
        vec![],
        "B: add from_b.md".into(),
        index_b.clone(),
        None,
        evidence_b,
        chrono::Utc::now().timestamp_millis(),
    );
    let id_b = cs_b.id;

    vault
        .dag()
        .update(|d| {
            d.insert(cs_b);
            d.update_peer_head(peer_b, id_b, index_b);
        })
        .await
        .expect("insert B's changeset");

    // A should also have a HEAD — sync a file first.
    vault.write_file_content("from_a.md", b"A content").await.unwrap();
    if let Some(ref w) = vault.watcher_ref() {
        w.dirty_paths.insert("from_a.md".to_string());
    }
    let _id_a = vault.sync("A: add from_a.md".into(), None).await.expect("A sync");

    // Now pending_forks should show B's fork.
    let forks = vault.pending_forks().await;
    assert_eq!(forks.len(), 1, "should see exactly one fork from B");
    assert_eq!(forks[0].0, peer_b);
    assert_eq!(forks[0].1.head, id_b);

    // diff_fork shows all differences between B's index and A's index.
    // B has from_b.md (Add from A's perspective), A has from_a.md (not in B's).
    let diff = vault.diff_fork(peer_b).await;
    assert!(diff.len() >= 1, "diff should show at least from_b.md");
    assert!(diff.ops.contains_key(&indras_sync_engine::LogicalPath::new("from_b.md")));

    // Merge from B.
    let merge_id = vault.merge_from_peer(peer_b).await.expect("merge");

    // After merge, no more pending forks from B.
    let dag = vault.dag().read().await;
    let my_head = dag.peer_head(&user_a).expect("my head");
    assert_eq!(my_head.head, merge_id);

    // The merge changeset should have both A's and B's heads as parents.
    let merge_cs = dag.get(&merge_id).expect("merge changeset");
    assert!(merge_cs.parents.contains(&id_b), "merge must parent on B's head");

    net.stop().await.ok();
}
