//! Integration test for `Vault::sync_all` — the Phase-2 composite of
//! `brisk-orbiting-lantern`.
//!
//! Two agents land independently into the inner braid. A single
//! `sync_all` call should:
//!
//! 1. Merge both agents into the user's inner HEAD.
//! 2. Promote the merged HEAD to the outer DAG (since the outer DAG
//!    had no head yet).
//! 3. Materialize every file referenced by the outer HEAD to the
//!    vault root on disk.
//! 4. Report all of this in the returned [`SyncAllReport`].
//!
//! A second `sync_all` with no new work should be a no-op — no agent
//! merges (forks already collapsed), no promote (head indexes match),
//! materialized count may be non-zero (re-writes are idempotent).
//!
//! Peer auto-merge (step 3 inside sync_all) is already covered by
//! `auto_merge_trusted` unit tests; exercising it here would require
//! spinning up a second iroh peer, which we intentionally skip.

use std::path::Path;
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::changeset::Evidence;
use indras_sync_engine::braid::derive_agent_id;
use indras_sync_engine::vault::Vault;
use indras_sync_engine::{ContentAddr, LogicalAgentId, LogicalPath, SymlinkIndex};
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

/// Stash `content` in the shared blob store and build a one-entry
/// `SymlinkIndex` pointing at it — enough to stand in for an agent's
/// working-tree snapshot without spinning up a full
/// `LocalWorkspaceIndex`.
async fn seed_index(blob: &BlobStore, path: &str, content: &[u8]) -> SymlinkIndex {
    let content_ref = blob.store(content).await.expect("store blob");
    SymlinkIndex::from_iter([(
        LogicalPath::new(path),
        ContentAddr::new(content_ref.hash, content_ref.size),
    )])
}

#[tokio::test]
async fn sync_all_merges_agents_promotes_and_materializes() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("syncAll", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "sync-all-test",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let user_id = vault.user_id();
    let agent_a = LogicalAgentId::new("A");
    let agent_b = LogicalAgentId::new("B");
    let roster = [agent_a.clone(), agent_b.clone()];

    let idx_a = seed_index(&blob, "from_a.md", b"A contents").await;
    let idx_b = seed_index(&blob, "from_b.md", b"B contents").await;

    let evidence_a = Evidence::Agent {
        compiled: true,
        tests_passed: vec!["crate-a".into()],
        lints_clean: true,
        runtime_ms: 3,
        signed_by: derive_agent_id(&user_id, agent_a.as_str()),
    };
    let evidence_b = Evidence::Agent {
        compiled: true,
        tests_passed: vec!["crate-b".into()],
        lints_clean: true,
        runtime_ms: 4,
        signed_by: derive_agent_id(&user_id, agent_b.as_str()),
    };

    vault
        .agent_land(&agent_a, "A: add note".into(), idx_a, evidence_a)
        .await;
    vault
        .agent_land(&agent_b, "B: add note".into(), idx_b, evidence_b)
        .await;

    // First sync: both agents should merge and the merged HEAD should
    // promote to the outer DAG. No peer forks exist so peer_merges is
    // empty. Both files must land on disk.
    let report = vault
        .sync_all("publish agent work".into(), &roster)
        .await
        .expect("sync_all");

    assert_eq!(report.agent_merges.len(), 2, "both agents should merge");
    assert!(
        report.agent_merges.iter().any(|(a, _)| a == &agent_a),
        "A missing from agent_merges: {:?}",
        report.agent_merges
    );
    assert!(
        report.agent_merges.iter().any(|(a, _)| a == &agent_b),
        "B missing from agent_merges: {:?}",
        report.agent_merges
    );
    assert!(report.promoted.is_some(), "outer DAG must promote");
    assert!(report.peer_merges.is_empty(), "no peers in this test");
    assert!(report.conflicts.is_empty(), "disjoint files, no conflicts");
    assert_eq!(
        report.materialized, 2,
        "both files must be written to vault root"
    );

    // Disk check: both files must be readable under the vault root.
    let on_disk_a = tokio::fs::read(tmp_vault.path().join("from_a.md"))
        .await
        .expect("from_a.md materialized");
    assert_eq!(on_disk_a, b"A contents");
    let on_disk_b = tokio::fs::read(tmp_vault.path().join("from_b.md"))
        .await
        .expect("from_b.md materialized");
    assert_eq!(on_disk_b, b"B contents");

    // Outer DAG carries the promoted changeset.
    {
        let outer = vault.dag().read().await;
        let promoted_id = report.promoted.unwrap();
        assert!(
            outer.contains(&promoted_id),
            "promoted changeset must be in outer DAG"
        );
    }

    // Second sync with no new work: no agent merges, no promote, no
    // conflicts. Materialize just rewrites the same files — we only
    // check that nothing breaks.
    let second = vault
        .sync_all("idempotent resync".into(), &roster)
        .await
        .expect("sync_all idempotent");
    assert!(
        second.agent_merges.is_empty(),
        "no pending agent work expected: {:?}",
        second.agent_merges
    );
    assert!(
        second.promoted.is_none(),
        "outer HEAD already reflects inner HEAD; nothing to promote"
    );
    assert!(second.conflicts.is_empty());

    net.stop().await.ok();
}
