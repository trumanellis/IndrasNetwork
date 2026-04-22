//! Persistence test for the per-realm `Document<ProjectRegistry>`.
//!
//! Proves that the CRDT document is the source of truth for project metadata:
//! a fresh `VaultManager` instance (no in-memory DashMap state shared with
//! the first) recovers the project list from the document via
//! `subscribe_to_registry`. That's the same mechanism the app uses on every
//! boot, so a passing test here proves projects survive restart.
//!
//! We share a single `IndrasNetwork` across the two managers to sidestep the
//! file-level redb lock that `IndrasNetwork::new` takes on `data_dir`.
//! Persistence semantics are unaffected — the CRDT document lives inside the
//! network's storage either way, and `VaultManager` owns no other durable
//! state that contributes to the project list.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use indras_network::IndrasNetwork;
use synchronicity_engine::vault_manager::VaultManager;
use tempfile::TempDir;

async fn build_network(name: &str, data_dir: &Path) -> Arc<IndrasNetwork> {
    IndrasNetwork::builder()
        .data_dir(data_dir)
        .display_name(name)
        .build()
        .await
        .unwrap_or_else(|e| panic!("build_network({name}): {e}"))
}

/// Stand up a `VaultManager` wired to the given network with its home vault
/// registered and the `_projects` registry subscription running. Mirrors the
/// app's boot flow at `crates/synchronicity-engine/src/components/app.rs`.
async fn wire_vm(
    data_dir: &Path,
    net: &Arc<IndrasNetwork>,
    display_name: &str,
) -> Arc<VaultManager> {
    let vm = Arc::new(
        VaultManager::new(data_dir.to_path_buf())
            .await
            .expect("VaultManager::new"),
    );
    vm.set_network(Arc::clone(net)).await;
    let home = net.home_realm().await.expect("home_realm");
    let home_id = home.id();
    let home_realm = net
        .get_realm_by_id(&home_id)
        .expect("home realm by id");
    // Seed the private sentinel path so `create_project` under `[0u8; 32]`
    // has a parent root.
    let _ = vm.start_private_vault(display_name).await;
    vm.ensure_vault(net.as_ref(), &home_realm, Some(display_name))
        .await
        .expect("ensure_vault home");
    // Drain the already-persisted registry into the caches.
    vm.subscribe_to_registry(&[0u8; 32]).await;
    vm
}

#[tokio::test]
async fn vault_manager_persists_via_registry() {
    let tmp_data = TempDir::new().unwrap();
    let display_name = "RegistryPersist";
    let net = build_network(display_name, tmp_data.path()).await;

    // ----- First boot: create a project and let the registry write land. -----
    let vm1 = wire_vm(tmp_data.path(), &net, display_name).await;
    let info = vm1
        .create_project(&[0u8; 32], "my-persisted-project")
        .await
        .expect("create_project");
    let pid = info.id;

    // Sanity: the originating boot can see it via its own caches.
    assert!(
        vm1.projects_of(&[0u8; 32]).contains(&pid),
        "first VaultManager must surface the project locally"
    );

    // Drop vm1 so its DashMap caches go away — any signal we read from vm2
    // must have come from the CRDT document, not leaked in-memory state.
    drop(vm1);

    // ----- Second boot: fresh VaultManager, same network, same data_dir. -----
    let vm2 = wire_vm(tmp_data.path(), &net, display_name).await;

    // `subscribe_to_registry` drained the doc synchronously before returning,
    // but the broadcast notification fires asynchronously — poll briefly.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !vm2.projects_of(&[0u8; 32]).contains(&pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let listed = vm2.projects_of(&[0u8; 32]);
    assert!(
        listed.contains(&pid),
        "fresh VaultManager must surface the project from the persisted registry; got {:?}",
        listed
    );
    assert_eq!(
        vm2.project_name(&pid).as_deref(),
        Some("my-persisted-project"),
        "project name must round-trip through the registry document"
    );

    // Clean shutdown so the TempDir's redb lock releases before drop.
    drop(vm2);
    net.stop().await.ok();
}
