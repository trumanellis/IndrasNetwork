//! Phase 4 final: wire `WorkspaceHandle` → `Vault` inner braid.
//!
//! Agent edits in a bound folder land in the inner (local-only) braid via
//! [`WorkspaceHandle::land_to_inner_braid`], parented through
//! [`Vault::agent_land`]. No CRDT sync, no outer DAG entry — the user
//! still has to merge + promote to share with peers.

use std::path::Path;
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::braid::changeset::Evidence;
use indras_sync_engine::braid::derive_agent_id;
use indras_sync_engine::vault::Vault;
use indras_sync_engine::{LogicalAgentId, LogicalPath};
use synchronicity_engine::team::{spawn_workspace_watchers, TeamBindingRegistry};
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
async fn workspace_lands_into_inner_braid_then_promotes() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();
    let tmp_agent = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "workspace-wire",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Seed the agent folder with a file, then stand up the workspace
    // handle via the real spawn_workspace_watchers helper so the test
    // exercises the production path.
    let agent = LogicalAgentId::new("agentA");
    tokio::fs::write(tmp_agent.path().join("notes.md"), b"agent thoughts")
        .await
        .unwrap();

    let mut registry = TeamBindingRegistry::new();
    registry.bind(agent.clone(), tmp_agent.path().to_path_buf());
    let handles = spawn_workspace_watchers(&registry, Arc::clone(&blob)).await;
    assert_eq!(handles.len(), 1, "one bound folder -> one handle");
    let handle = handles.into_iter().next().unwrap();

    // Land the workspace snapshot into the inner braid.
    let user_id = vault.user_id();
    let evidence = Evidence::Agent {
        compiled: true,
        tests_passed: vec!["test-crate".into()],
        lints_clean: true,
        runtime_ms: 7,
        signed_by: derive_agent_id(&user_id, agent.as_str()),
    };
    let change_id = handle
        .land_to_inner_braid(&vault, "seed notes".into(), evidence)
        .await;

    // Inner braid carries the changeset; outer DAG does not.
    {
        let inner = vault.inner_braid().read().await;
        assert!(inner.dag().contains(&change_id));
        let cs = inner.dag().get(&change_id).unwrap();
        assert_eq!(
            cs.index.get(&LogicalPath::new("notes.md")).map(|a| a.size),
            Some("agent thoughts".len() as u64),
        );
    }
    {
        let outer = vault.dag().read().await;
        assert!(
            !outer.contains(&change_id),
            "inner-braid changesets must not leak into the outer DAG"
        );
    }

    // Merge the agent HEAD and promote — the notes show up on the outer DAG.
    let merge = vault.merge_agent(&agent).await.expect("merge_agent");
    assert!(merge.conflicts.is_empty());
    let promoted = vault.promote("publish notes".into()).await.expect("promote");

    let outer = vault.dag().read().await;
    let cs = outer.get(&promoted).expect("promoted changeset on outer DAG");
    assert_eq!(
        cs.index.get(&LogicalPath::new("notes.md")).map(|a| a.size),
        Some("agent thoughts".len() as u64),
    );
}
