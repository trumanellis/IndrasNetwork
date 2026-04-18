//! Multi-instance E2E tests for the fork-rights vault architecture.
//!
//! Exercises the full flow over real iroh transport:
//!   - A writes + syncs with `Vault::sync()` (Evidence::Human)
//!   - B's DAG receives A's changeset via iroh gossip
//!   - B sees the fork via `pending_forks()`
//!   - B merges via `merge_from_peer()` and file materializes on disk
//!   - Trust-based auto-merge via `set_peer_trust()`
//!
//! Marked `#[ignore]` since they require the real iroh transport and take
//! ~10-60s each. Run explicitly with:
//!
//! ```sh
//! cargo test -p indras-sync-engine --test fork_rights_e2e -- --ignored
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{ChangeId, RealmBraid};
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

/// Poll a peer's DAG until it sees a specific changeset, or timeout.
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

/// Poll until `pending_forks` returns at least `count` forks, or timeout.
async fn wait_for_forks(vault: &Vault, count: usize, deadline: Duration) -> bool {
    let end = tokio::time::Instant::now() + deadline;
    loop {
        let forks = vault.pending_forks().await;
        if forks.len() >= count {
            return true;
        }
        if tokio::time::Instant::now() >= end {
            return false;
        }
        sleep(Duration::from_millis(250)).await;
    }
}

// ── scenario 1: human sync + manual merge ───────────────────────────────────

#[tokio::test]
#[ignore] // requires iroh transport
async fn human_sync_propagates_and_manual_merge_materializes() {
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    // Set up two peers.
    let net_a = build_network("A", tmp_a_data.path()).await;
    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "fork-rights-e2e",
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

    // A writes a file and syncs (human Evidence).
    let content_a = b"hello from peer A\n";
    vault_a
        .write_file_content("greeting.md", content_a)
        .await
        .expect("A: write");

    // Mark dirty (watcher race — deterministic for test).
    vault_a.watcher_ref().unwrap().dirty_paths.insert("greeting.md".to_string());

    let cs_a = vault_a
        .sync("add greeting.md".into(), Some("A's first sync".into()))
        .await
        .expect("A: sync");

    // Verify A's changeset has Human evidence.
    {
        let dag = vault_a.dag().read().await;
        let cs = dag.get(&cs_a).expect("A's changeset");
        assert!(
            matches!(cs.evidence, indras_sync_engine::Evidence::Human { .. }),
            "A's sync must produce Evidence::Human"
        );
    }

    // B's DAG must receive A's changeset via iroh.
    let arrived = wait_for_changeset(vault_b.realm(), cs_a, Duration::from_secs(20)).await;
    assert!(arrived, "A's changeset must propagate to B within 20s");

    // B should see the fork via pending_forks (B has no HEAD yet, A does).
    // We need B's DAG to also have A's peer_head — wait for that.
    let has_forks = wait_for_forks(&vault_b, 1, Duration::from_secs(10)).await;
    assert!(has_forks, "B must see A's fork in pending_forks");

    let forks = vault_b.pending_forks().await;
    assert_eq!(forks.len(), 1);
    let (fork_peer, fork_state) = &forks[0];
    assert_eq!(fork_state.head, cs_a);

    // B can see what files differ.
    let diff = vault_b.diff_fork(*fork_peer).await;
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].path, "greeting.md");

    // B explicitly merges A's changes.
    let merge_id = vault_b
        .merge_from_peer(*fork_peer)
        .await
        .expect("B: merge_from_peer");

    // The file must materialize on B's disk.
    let b_file = tmp_b_vault.path().join("greeting.md");
    let materialized = poll_file_contains(&b_file, content_a, Duration::from_secs(10)).await;
    assert!(
        materialized,
        "greeting.md must appear on B's disk after merge"
    );

    // B's HEAD must be the merge changeset.
    {
        let dag = vault_b.dag().read().await;
        let b_head = dag.peer_head(&vault_b.user_id()).expect("B head");
        assert_eq!(b_head.head, merge_id);
    }

    // After merge, A's HEAD (cs_a) is an ancestor of B's merge changeset.
    // pending_forks still reports A because A's head != B's head, but the
    // merge changeset has A's head as a parent — proving the merge landed.
    {
        let dag = vault_b.dag().read().await;
        let merge_cs = dag.get(&merge_id).expect("merge changeset");
        assert!(
            merge_cs.parents.contains(&cs_a),
            "merge changeset must parent on A's original head"
        );
    }

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}

// ── scenario 2: bidirectional sync — both peers edit, then merge ─────────

#[tokio::test]
#[ignore] // requires iroh transport
async fn bidirectional_sync_and_merge() {
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    let net_a = build_network("A", tmp_a_data.path()).await;
    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "bidir-e2e",
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
        .expect("members");

    // Both peers write different files and sync.
    let content_a = b"A's work\n";
    let content_b = b"B's work\n";

    vault_a.write_file_content("a.md", content_a).await.unwrap();
    vault_a.watcher_ref().unwrap().dirty_paths.insert("a.md".to_string());
    let cs_a = vault_a.sync("A: add a.md".into(), None).await.expect("A sync");

    vault_b.write_file_content("b.md", content_b).await.unwrap();
    vault_b.watcher_ref().unwrap().dirty_paths.insert("b.md".to_string());
    let cs_b = vault_b.sync("B: add b.md".into(), None).await.expect("B sync");

    // Wait for both changesets to propagate.
    let a_sees_b = wait_for_changeset(vault_a.realm(), cs_b, Duration::from_secs(20)).await;
    let b_sees_a = wait_for_changeset(vault_b.realm(), cs_a, Duration::from_secs(20)).await;
    assert!(a_sees_b, "A must see B's changeset");
    assert!(b_sees_a, "B must see A's changeset");

    // Both should see each other as forks.
    let a_forks = wait_for_forks(&vault_a, 1, Duration::from_secs(10)).await;
    let b_forks = wait_for_forks(&vault_b, 1, Duration::from_secs(10)).await;
    assert!(a_forks, "A must see B's fork");
    assert!(b_forks, "B must see A's fork");

    // A merges B's changes.
    let user_b = vault_b.user_id();
    vault_a.merge_from_peer(user_b).await.expect("A merges B");

    // B merges A's changes.
    let user_a = vault_a.user_id();
    vault_b.merge_from_peer(user_a).await.expect("B merges A");

    // Both files should exist on both peers' disks.
    let d = Duration::from_secs(10);
    assert!(poll_file_contains(&tmp_a_vault.path().join("b.md"), content_b, d).await,
        "b.md must appear on A's disk");
    assert!(poll_file_contains(&tmp_b_vault.path().join("a.md"), content_a, d).await,
        "a.md must appear on B's disk");

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}

// ── scenario 3: trust-based auto-merge ──────────────────────────────────

#[tokio::test]
#[ignore] // requires iroh transport
async fn trusted_peer_auto_merges() {
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    let net_a = build_network("A", tmp_a_data.path()).await;
    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "trust-e2e",
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
        .expect("members");

    // B trusts A.
    let user_a = vault_a.user_id();
    vault_b.set_peer_trust(user_a, true).await.expect("B trusts A");
    assert!(vault_b.is_peer_trusted(&user_a).await);

    // A writes and syncs.
    let content = b"auto-merged content\n";
    vault_a.write_file_content("auto.md", content).await.unwrap();
    vault_a.watcher_ref().unwrap().dirty_paths.insert("auto.md".to_string());
    let cs_a = vault_a.sync("A: add auto.md".into(), None).await.expect("A sync");

    // Wait for A's changeset to reach B.
    let arrived = wait_for_changeset(vault_b.realm(), cs_a, Duration::from_secs(20)).await;
    assert!(arrived, "A's changeset must reach B");

    // Since B trusts A, set_peer_trust should auto-merge when the fork
    // is detected. But we need to trigger trust evaluation — call
    // set_peer_trust again to trigger auto-merge of any pending fork.
    // Wait a moment for peer_heads to propagate.
    sleep(Duration::from_secs(2)).await;
    vault_b.set_peer_trust(user_a, true).await.expect("re-trigger trust");

    // The file should materialize on B's disk via auto-merge.
    let b_file = tmp_b_vault.path().join("auto.md");
    let materialized = poll_file_contains(&b_file, content, Duration::from_secs(15)).await;
    assert!(
        materialized,
        "auto.md must appear on B's disk via trust-based auto-merge"
    );

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}
