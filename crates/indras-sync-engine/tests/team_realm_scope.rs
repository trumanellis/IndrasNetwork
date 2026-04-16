//! Team realm scope: only agent-hosting devices open the team realm.
//!
//! Two peers A and B share a synced vault. A hosts agents so A calls
//! `ensure_team_realm` and materializes the team realm. B does NOT
//! host agents and never calls it. The vault-document `team_realm_id`
//! eventually gossips to B, but B's network has no such realm open.
//!
//! Marked `#[ignore]` — requires real iroh transport. Run explicitly:
//!
//! ```sh
//! cargo test -p indras-sync-engine --test team_realm_scope -- --ignored
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::realm_team::{derive_team_artifact_id, RealmTeam};
use indras_sync_engine::realm_vault::RealmVault;
use indras_sync_engine::vault::Vault;
use tempfile::TempDir;
use tokio::time::sleep;

async fn build_blob_store(data_dir: &Path) -> Arc<BlobStore> {
    let cfg = BlobStoreConfig {
        base_dir: data_dir.join("shared-blobs"),
        ..Default::default()
    };
    Arc::new(BlobStore::new(cfg).await.expect("BlobStore::new"))
}

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

#[tokio::test]
#[ignore] // requires iroh transport
async fn non_hosting_device_never_opens_team_realm() {
    let tmp_a_data = TempDir::new().unwrap();
    let tmp_a_vault = TempDir::new().unwrap();
    let tmp_b_data = TempDir::new().unwrap();
    let tmp_b_vault = TempDir::new().unwrap();

    // A creates the shared vault.
    let net_a = build_network("A", tmp_a_data.path()).await;
    let blob_a = build_blob_store(tmp_a_data.path()).await;
    let (vault_a, invite) = Vault::create(
        &net_a,
        "team-scope-vault",
        tmp_a_vault.path().to_path_buf(),
        Arc::clone(&blob_a),
    )
    .await
    .expect("A: Vault::create");

    // B joins.
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

    // A materializes the team realm (hosts agents, in this scenario).
    let team_realm_id = vault_a
        .realm()
        .ensure_team_realm(&net_a, "team-realm")
        .await
        .expect("A: ensure_team_realm");

    // Deterministic derivation: B can compute the same id from the shared
    // vault id, but that's just math — it doesn't open the realm.
    let expected_artifact = derive_team_artifact_id(&vault_a.realm().id());
    let _ = expected_artifact; // asserted indirectly via `team_realm_id`.

    // A's network holds the team realm.
    assert!(
        net_a.get_realm_by_id(&team_realm_id).is_some(),
        "A must have opened the team realm after ensure_team_realm"
    );

    // Wait for the vault document to gossip the `team_realm_id` update
    // to B. Poll B's vault-index until the field lands.
    let b_idx = vault_b.realm().vault_index().await.expect("B: vault_index");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut saw = false;
    while tokio::time::Instant::now() < deadline {
        b_idx.refresh().await.expect("B: refresh");
        if b_idx.read().await.team.team_realm_id == Some(team_realm_id) {
            saw = true;
            break;
        }
        sleep(Duration::from_millis(250)).await;
    }
    assert!(
        saw,
        "B must observe A's team_realm_id via vault-doc gossip within 30s"
    );

    // The invariant: B has NOT opened the team realm. It knows the id,
    // but never called create_realm_with_artifact, so the transport has
    // no interface for it.
    assert!(
        net_b.get_realm_by_id(&team_realm_id).is_none(),
        "B must NOT hold the team realm — it does not host any agent and \
         should never have opened the DAG's realm"
    );

    net_a.stop().await.ok();
    net_b.stop().await.ok();
}
