//! Integration test: `RealmTeam::ensure_team_realm` creates a team realm on
//! first call and is idempotent on subsequent calls.
//!
//! Single-peer scenario: build network, create vault, call `ensure_team_realm`
//! twice, assert the returned id is `Some`, matches the vault document, and
//! is stable across invocations.

use std::path::Path;
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::realm_vault::RealmVault;
use indras_sync_engine::{RealmTeam, vault::Vault};
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
async fn ensure_team_realm_creates_and_is_idempotent() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_vault = TempDir::new().unwrap();

    let net = build_network("A", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;
    let (vault, _invite) = Vault::create(
        &net,
        "team-realm-ensure-vault",
        tmp_vault.path().to_path_buf(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Before: vault document has no team_realm_id.
    let idx = vault.realm().vault_index().await.expect("vault_index");
    assert!(
        idx.read().await.team.team_realm_id.is_none(),
        "fresh vault should have no team_realm_id"
    );

    // First call creates the realm.
    let id1 = vault
        .realm()
        .ensure_team_realm(&net, "team-for-agents")
        .await
        .expect("ensure_team_realm first call");

    // Separate document handles hold independent in-memory state — refresh
    // so this test's handle observes the write made through a sibling handle.
    idx.refresh().await.expect("refresh after ensure");
    assert_eq!(
        idx.read().await.team.team_realm_id,
        Some(id1),
        "vault doc should carry the new team_realm_id"
    );

    // Second call is idempotent — same id, no new realm created.
    let id2 = vault
        .realm()
        .ensure_team_realm(&net, "team-for-agents")
        .await
        .expect("ensure_team_realm second call");
    assert_eq!(id1, id2, "ensure_team_realm must be idempotent");
}
