//! Auto-parenting: sequential agent commits inherit prior heads.
//!
//! Two agents commit to the same vault in sequence. The second commit's
//! Changeset must have the first commit's ChangeId as a parent — proving
//! the DAG's `heads()` mechanic gives automatic rebase-like sequencing
//! without any explicit "pull" step. This is the Phase-3 invariant that
//! makes parallel-agent braid sync safe: every commit builds on what
//! came before.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::{PatchManifest, RealmBraid};
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::LocalWorkspaceIndex;
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
async fn sequential_commits_auto_parent_on_prior_heads() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();
    let tmp_agent1 = TempDir::new().unwrap();
    let tmp_agent2 = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "rebase-vault",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let pq = net.node().pq_identity();
    let user_id = pq.user_id();

    // Agent 1 writes and commits.
    let idx1 = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent1.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    tokio::fs::write(tmp_agent1.path().join("a.rs"), b"agent 1 work")
        .await
        .unwrap();
    idx1.ingest_bytes("a.rs", b"agent 1 work").await.unwrap();

    let manifest1 = PatchManifest::new(idx1.snapshot_all().await);
    let id1 = vault
        .realm()
        .try_land(
            "agent1: add a.rs".into(),
            manifest1,
            Vec::new(),
            tmp_agent1.path().to_path_buf(),
            user_id,
            pq,
        )
        .await
        .expect("agent1 try_land");

    // Agent 2 writes and commits. Its try_land reads the DAG heads,
    // which now include id1 — so id1 is automatically a parent.
    let idx2 = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent2.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    tokio::fs::write(tmp_agent2.path().join("b.rs"), b"agent 2 work")
        .await
        .unwrap();
    idx2.ingest_bytes("b.rs", b"agent 2 work").await.unwrap();

    let manifest2 = PatchManifest::new(idx2.snapshot_all().await);
    let id2 = vault
        .realm()
        .try_land(
            "agent2: add b.rs".into(),
            manifest2,
            Vec::new(),
            tmp_agent2.path().to_path_buf(),
            user_id,
            pq,
        )
        .await
        .expect("agent2 try_land");

    // The DAG should have a single head (id2); id1 is no longer a head
    // because id2 superseded it. And id2's parents must include id1.
    let dag_doc = vault.realm().braid_dag().await.expect("braid_dag");
    let dag = dag_doc.read().await;

    // id2 is the sole head (id1 was superseded).
    let heads = dag.heads();
    assert!(
        heads.contains(&id2),
        "id2 must be a head; heads = {heads:?}"
    );
    assert!(
        !heads.contains(&id1),
        "id1 must NOT be a head after id2 superseded it"
    );

    // id2's parents include id1.
    let cs2 = dag.get(&id2).expect("changeset id2");
    assert!(
        cs2.parents.contains(&id1),
        "id2's parents must include id1; parents = {:?}",
        cs2.parents
    );

    net.stop().await.ok();
}
