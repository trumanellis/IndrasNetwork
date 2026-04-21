//! Multi-instance braid sync integration tests (iroh transport).
//!
//! Exercises the end-to-end path:
//!   peer A: write file -> insert Changeset -> BraidDag propagates over iroh
//!   peer B: receive Changeset in DAG -> checkout -> file materializes on disk
//!
//! These tests skip the cargo verification gate (`RealmBraid::try_land`) and
//! construct `Changeset` directly with synthetic `Evidence`, because the
//! interesting thing here is the *sync path*, not the gate.
//!
//! Marked `#[ignore]` since they require the real iroh transport and take
//! ~10-60s each. Run explicitly with:
//!
//! ```sh
//! cargo test -p indras-sync-engine --test braid_sync_wiring -- --ignored
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{ChangeId, Changeset, Evidence, RealmBraid};
use indras_sync_engine::{ContentAddr, LogicalPath, SymlinkIndex};
use indras_sync_engine::vault::Vault;
use tempfile::TempDir;
use tokio::time::sleep;

// ── helpers ──────────────────────────────────────────────────────────────────

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

/// Poll a vault path until its bytes match `expected`, or `deadline` elapses.
async fn poll_file_contains(path: &Path, expected: &[u8], deadline: Duration) -> bool {
    let end = tokio::time::Instant::now() + deadline;
    loop {
        if let Ok(data) = tokio::fs::read(path).await {
            if data == expected {
                return true;
            }
        }
        if tokio::time::Instant::now() >= end {
            return false;
        }
        sleep(Duration::from_millis(250)).await;
    }
}

/// Poll a realm's BraidDag until it contains `id`, or `deadline` elapses.
async fn wait_for_changeset(
    realm: &indras_network::Realm,
    id: ChangeId,
    deadline: Duration,
) -> bool {
    let end = tokio::time::Instant::now() + deadline;
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

/// Build + insert a changeset referencing `touched_paths` in the realm's vault,
/// with synthetic green evidence. Returns the new ChangeId.
///
/// Builds the manifest from the vault's list_files (local index) rather
/// than the old snapshot_patch (which relied on the shared CRDT).
async fn land_synthetic(
    vault: &indras_sync_engine::vault::Vault,
    author: [u8; 32],
    intent: &str,
    touched_paths: &[String],
) -> ChangeId {
    let files = vault.list_files().await;
    let index = SymlinkIndex::from_iter(
        touched_paths
            .iter()
            .filter_map(|p| {
                files.iter().find(|f| &f.path == p).map(|f| {
                    (LogicalPath::new(&f.path), ContentAddr::new(f.hash, f.size))
                })
            }),
    );
    let realm = vault.realm();
    let parents = realm.braid_heads().await.expect("braid_heads");
    let evidence = Evidence::Agent {
        compiled: true,
        tests_passed: vec![],
        lints_clean: true,
        runtime_ms: 0,
        signed_by: author,
    };
    let ts = chrono::Utc::now().timestamp_millis();
    let cs = Changeset::new_unsigned(author, parents, intent.into(), index, None, evidence, ts);
    let id = cs.id;
    realm
        .braid_dag()
        .await
        .expect("braid_dag")
        .update(|d| d.insert(cs))
        .await
        .expect("insert changeset");
    id
}

// ── scenario 1: two-peer braid sync + checkout ──────────────────────────────

#[tokio::test]
#[ignore] // requires iroh transport
async fn two_peer_braid_sync_and_checkout() {
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    let net_a = build_network("A", tmp_a_data.path()).await;
    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "braid-sync-vault",
        tmp_a_vault.path().to_path_buf(),
        Arc::clone(&blob_a),
    )
    .await
    .expect("A: Vault::create");

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

    vault_b
        .await_members(2, Duration::from_secs(15))
        .await
        .expect("await_members");

    // A writes a file and inserts a changeset referencing it.
    let content = b"pub fn answer() -> u32 { 42 }";
    vault_a
        .write_file_content("src/lib.rs", content)
        .await
        .expect("A: write");

    let author_a = net_a.node().pq_identity().user_id();
    let cs_id = land_synthetic(
        &vault_a,
        author_a,
        "feat: add answer fn",
        &["src/lib.rs".into()],
    )
    .await;

    // BraidDag must reach B.
    let arrived = wait_for_changeset(vault_b.realm(), cs_id, Duration::from_secs(15)).await;
    assert!(arrived, "changeset {cs_id} must propagate to B within 15s");

    // B checks out; file must materialize on B's disk.
    vault_b.checkout(cs_id).await.expect("B: checkout");
    let b_file = tmp_b_vault.path().join("src/lib.rs");
    let materialized =
        poll_file_contains(&b_file, content, Duration::from_secs(15)).await;
    assert!(
        materialized,
        "src/lib.rs must appear on B's disk with A's bytes within 15s"
    );

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}

// ── scenario 2: three-peer concurrent braid + mutual checkout ───────────────

#[tokio::test]
#[ignore] // requires iroh transport
async fn three_peer_concurrent_braid_and_checkout() {
    let tmps: [(TempDir, TempDir); 3] = [
        (TempDir::new().unwrap(), TempDir::new().unwrap()),
        (TempDir::new().unwrap(), TempDir::new().unwrap()),
        (TempDir::new().unwrap(), TempDir::new().unwrap()),
    ];

    let net_a = build_network("A", tmps[0].0.path()).await;
    let blob_a = build_blob_store(tmps[0].0.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "braid-3peer-vault",
        tmps[0].1.path().to_path_buf(),
        Arc::clone(&blob_a),
    )
    .await
    .expect("A: create");

    let net_b = build_network("B", tmps[1].0.path()).await;
    let blob_b = build_blob_store(tmps[1].0.path()).await;
    let invite_str = invite.to_string();
    let vault_b = Vault::join(
        &net_b,
        &invite_str,
        tmps[1].1.path().to_path_buf(),
        Arc::clone(&blob_b),
    )
    .await
    .expect("B: join");

    let net_c = build_network("C", tmps[2].0.path()).await;
    let blob_c = build_blob_store(tmps[2].0.path()).await;
    let vault_c = Vault::join(
        &net_c,
        &invite_str,
        tmps[2].1.path().to_path_buf(),
        Arc::clone(&blob_c),
    )
    .await
    .expect("C: join");

    vault_c
        .await_members(3, Duration::from_secs(30))
        .await
        .expect("await_members 3");

    // Each peer writes its own file and lands its own root changeset.
    let content_a = b"// file by A\n";
    let content_b = b"// file by B\n";
    let content_c = b"// file by C\n";
    vault_a.write_file_content("a.rs", content_a).await.unwrap();
    vault_b.write_file_content("b.rs", content_b).await.unwrap();
    vault_c.write_file_content("c.rs", content_c).await.unwrap();

    let id_a = net_a.node().pq_identity().user_id();
    let id_b = net_b.node().pq_identity().user_id();
    let id_c = net_c.node().pq_identity().user_id();

    let cs_a = land_synthetic(&vault_a, id_a, "A: add a.rs", &["a.rs".into()]).await;
    let cs_b = land_synthetic(&vault_b, id_b, "B: add b.rs", &["b.rs".into()]).await;
    let cs_c = land_synthetic(&vault_c, id_c, "C: add c.rs", &["c.rs".into()]).await;

    // All three changesets must reach all three peers.
    for (label, vault) in [
        ("A", &vault_a),
        ("B", &vault_b),
        ("C", &vault_c),
    ] {
        for (who, id) in [("A", cs_a), ("B", cs_b), ("C", cs_c)] {
            let ok = wait_for_changeset(vault.realm(), id, Duration::from_secs(30)).await;
            assert!(ok, "peer {label} must receive changeset from {who}");
        }
    }

    // Each peer checks out the other two and sees their files appear.
    vault_a.checkout(cs_b).await.expect("A checkout cs_b");
    vault_a.checkout(cs_c).await.expect("A checkout cs_c");
    vault_b.checkout(cs_a).await.expect("B checkout cs_a");
    vault_b.checkout(cs_c).await.expect("B checkout cs_c");
    vault_c.checkout(cs_a).await.expect("C checkout cs_a");
    vault_c.checkout(cs_b).await.expect("C checkout cs_b");

    let d = Duration::from_secs(20);
    assert!(poll_file_contains(&tmps[0].1.path().join("b.rs"), content_b, d).await);
    assert!(poll_file_contains(&tmps[0].1.path().join("c.rs"), content_c, d).await);
    assert!(poll_file_contains(&tmps[1].1.path().join("a.rs"), content_a, d).await);
    assert!(poll_file_contains(&tmps[1].1.path().join("c.rs"), content_c, d).await);
    assert!(poll_file_contains(&tmps[2].1.path().join("a.rs"), content_a, d).await);
    assert!(poll_file_contains(&tmps[2].1.path().join("b.rs"), content_b, d).await);

    // Merge-DAG-collapse mechanics are covered in-process by
    // `braid_three_peer::braid_merge_changeset_has_three_parents`; here we've
    // already demonstrated the transport + checkout path for three peers.

    net_a.stop().await.ok();
    net_b.stop().await.ok();
    net_c.stop().await.ok();
}

// ── scenario 3: braid_dag_subscribe observer ─────────────────────────────────

#[tokio::test]
#[ignore] // requires iroh transport
async fn subscribe_yields_incoming_changesets() {
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    let net_a = build_network("A", tmp_a_data.path()).await;
    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "braid-subscribe-vault",
        tmp_a_vault.path().to_path_buf(),
        Arc::clone(&blob_a),
    )
    .await
    .expect("A: create");

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
    .expect("B: join");

    vault_b
        .await_members(2, Duration::from_secs(15))
        .await
        .expect("await_members");

    // B subscribes BEFORE A lands — so the merge event is guaranteed to arrive.
    let mut rx = vault_b
        .realm()
        .braid_dag_subscribe()
        .await
        .expect("subscribe");

    let content = b"// observed\n";
    vault_a
        .write_file_content("observed.rs", content)
        .await
        .expect("A: write");
    let author_a = net_a.node().pq_identity().user_id();
    let cs_id = land_synthetic(
        &vault_a,
        author_a,
        "feat: observed",
        &["observed.rs".into()],
    )
    .await;

    // Drain receiver until we see the new changeset (or timeout).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let seen = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break false;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(change)) => {
                if change.new_state.contains(&cs_id) {
                    break true;
                }
            }
            Ok(Err(_)) => break false,
            Err(_) => break false,
        }
    };
    assert!(seen, "subscribe must yield the incoming changeset within 15s");

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}
