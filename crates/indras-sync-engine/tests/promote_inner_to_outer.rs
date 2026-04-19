//! Integration tests for Phase 3 — `Vault::promote()`.
//!
//! Exercises the bridge from the inner (agent-level, local-only) braid into
//! the outer peer-synced DAG. Agent lands a changeset into the inner braid,
//! the user merges it into their inner HEAD, then promotes to a signed
//! outer-DAG changeset visible to peers.

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

fn addr(byte: u8) -> ContentAddr {
    ContentAddr::new([byte; 32], byte as u64 * 100)
}

fn index(entries: &[(&str, u8)]) -> SymlinkIndex {
    SymlinkIndex::from_iter(
        entries
            .iter()
            .map(|(p, b)| (LogicalPath::new(*p), addr(*b))),
    )
}

#[tokio::test]
async fn promote_without_inner_head_errors() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "promote-empty",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let result = vault.promote("empty".into()).await;
    assert!(result.is_err(), "promote with no inner HEAD must error");
}

#[tokio::test]
async fn promote_lands_inner_head_into_outer_dag() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "promote-flow",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let agent = LogicalAgentId::new("A");
    let user_id = vault.user_id();
    let agent_evidence = Evidence::Agent {
        compiled: true,
        tests_passed: vec!["test-crate".into()],
        lints_clean: true,
        runtime_ms: 42,
        signed_by: derive_agent_id(&user_id, agent.as_str()),
    };

    // Agent lands work into inner braid, user merges it.
    {
        let mut inner = vault.inner_braid().write().await;
        inner.agent_land(
            &agent,
            "add foo.rs".into(),
            index(&[("foo.rs", 1)]),
            agent_evidence,
        );
        let merge_result = inner.merge_agent(&agent).expect("merge_agent");
        assert!(merge_result.conflicts.is_empty());
    }

    // Promote to outer DAG.
    let change_id = vault.promote("ship it".into()).await.expect("promote");

    // Outer DAG contains the promoted changeset.
    let dag = vault.dag().read().await;
    let cs = dag.get(&change_id).expect("changeset must exist in outer DAG");
    assert_eq!(cs.author, user_id);
    assert!(cs.parents.is_empty(), "first promote has no outer parents");
    assert_eq!(
        cs.index.get(&LogicalPath::new("foo.rs")),
        Some(&addr(1)),
        "promoted index carries agent work"
    );
    assert!(matches!(cs.evidence, Evidence::Human { .. }));

    // User's outer peer_head advanced.
    let head = dag.peer_head(&user_id).expect("peer head must be set");
    assert_eq!(head.head, change_id);
    assert_eq!(head.head_index.get(&LogicalPath::new("foo.rs")), Some(&addr(1)));
}

#[tokio::test]
async fn vault_routing_agent_land_and_merge_agent() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "routing-flow",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let agent = LogicalAgentId::new("A");
    let user_id = vault.user_id();
    let evidence = Evidence::Agent {
        compiled: true,
        tests_passed: vec!["test-crate".into()],
        lints_clean: true,
        runtime_ms: 42,
        signed_by: derive_agent_id(&user_id, agent.as_str()),
    };

    // Exercise the Vault-level routing API end-to-end.
    let land_id = vault
        .agent_land(&agent, "add foo".into(), index(&[("foo.rs", 1)]), evidence)
        .await;
    let merge = vault.merge_agent(&agent).await.expect("merge_agent");
    assert!(merge.conflicts.is_empty());

    // Inner DAG observable through the accessor still agrees.
    {
        let inner = vault.inner_braid().read().await;
        assert!(inner.dag().contains(&land_id));
        let head = inner.user_head().expect("user inner HEAD");
        assert_eq!(head.head, merge.change_id);
    }

    // Promote now works through the outer DAG.
    let promote_id = vault.promote("ship".into()).await.expect("promote");
    let dag = vault.dag().read().await;
    let cs = dag.get(&promote_id).expect("promoted cs");
    assert_eq!(
        cs.index.get(&LogicalPath::new("foo.rs")),
        Some(&addr(1))
    );
}

#[tokio::test]
async fn second_promote_parents_on_first() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let (vault, _invite) = Vault::create(
        &net,
        "promote-chain",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let agent = LogicalAgentId::new("A");
    let user_id = vault.user_id();
    let ev = |name: &str| Evidence::Agent {
        compiled: true,
        tests_passed: vec![name.into()],
        lints_clean: true,
        runtime_ms: 10,
        signed_by: derive_agent_id(&user_id, agent.as_str()),
    };

    // First round: land + merge + promote.
    {
        let mut inner = vault.inner_braid().write().await;
        inner.agent_land(&agent, "v1".into(), index(&[("a.rs", 1)]), ev("v1"));
        inner.merge_agent(&agent).expect("merge v1");
    }
    let id1 = vault.promote("v1".into()).await.expect("promote v1");

    // Second round: land + merge + promote.
    {
        let mut inner = vault.inner_braid().write().await;
        inner.agent_land(&agent, "v2".into(), index(&[("a.rs", 2)]), ev("v2"));
        inner.merge_agent(&agent).expect("merge v2");
    }
    let id2 = vault.promote("v2".into()).await.expect("promote v2");

    let dag = vault.dag().read().await;
    let cs2 = dag.get(&id2).expect("second promoted changeset");
    assert_eq!(cs2.parents, vec![id1], "second promote must parent on first");
    assert_eq!(cs2.index.get(&LogicalPath::new("a.rs")), Some(&addr(2)));

    let head = dag.peer_head(&user_id).expect("peer head");
    assert_eq!(head.head, id2);
}
