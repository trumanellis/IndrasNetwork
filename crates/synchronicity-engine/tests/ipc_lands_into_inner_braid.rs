//! Phase-1 + Phase-2 regression guard for `brisk-orbiting-lantern`.
//!
//! Before the rewire, the IPC commit path called `realm.try_land`, which
//! landed changesets directly on the outer (shared) DAG with no agent
//! attribution or auto-sync. The Phase-2 IPC path now does the full
//! braid pipeline in one round-trip:
//!
//! 1. Land the agent's working-tree snapshot on the inner (local-only)
//!    braid via `Vault::agent_land`.
//! 2. Merge every diverged agent into the user's inner HEAD.
//! 3. Promote the user's inner HEAD to a signed outer changeset.
//! 4. Auto-merge trusted peer forks.
//! 5. Materialize the resulting outer HEAD to the vault root on disk.
//!
//! This test exercises the real unix-socket surface end-to-end and
//! asserts: the IPC response carries both the inner `change_id` and
//! the outer `promoted` id, the outer DAG holds the promoted changeset
//! (not the raw inner-braid id), and the file appears on disk under
//! the vault root.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::team::LogicalAgentId;
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::{FolderLock, LocalWorkspaceIndex, WorkspaceWatcher};
use synchronicity_engine::ipc::{self, IpcBinding};
use synchronicity_engine::vault_manager::VaultManager;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

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
async fn ipc_commit_lands_inner_promotes_and_materializes() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_agent = TempDir::new().unwrap();

    let net = build_network("innerBraidIPC", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    let vault_dir = tmp_data.path().join("vaults").join("inner-braid-test");
    tokio::fs::create_dir_all(&vault_dir).await.unwrap();
    let (vault, _invite) = Vault::create(
        &net,
        "inner-braid-test",
        vault_dir.clone(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    let vm = VaultManager::new(tmp_data.path().to_path_buf())
        .await
        .expect("VaultManager::new");
    vm.ensure_vault(net.as_ref(), vault.realm(), Some("inner-braid-test"))
        .await
        .expect("ensure_vault");
    let vm_arc = Arc::new(vm);

    // Stand up a bound agent folder with a live index + watcher.
    let _lock = FolderLock::acquire(tmp_agent.path()).expect("lock");
    let index = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    let _watcher = WorkspaceWatcher::start(Arc::clone(&index)).expect("watcher");
    tokio::time::sleep(Duration::from_millis(150)).await;

    tokio::fs::write(tmp_agent.path().join("notes.md"), b"agent thoughts")
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if index.get("notes.md").await.is_some() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "index never saw notes.md"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let agent = LogicalAgentId::new("agent-inner");
    let (hook_tx, _hook_rx) = tokio::sync::mpsc::unbounded_channel();
    let _server = ipc::start_ipc_server(
        tmp_data.path().to_path_buf(),
        Arc::clone(&net),
        Arc::clone(&vm_arc),
        vec![IpcBinding { agent: agent.clone(), index: Arc::clone(&index) }],
        hook_tx,
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    let sock_path = ipc::socket_path(tmp_data.path());
    let stream = UnixStream::connect(&sock_path)
        .await
        .expect("connect to IPC socket");
    let (reader, mut writer) = stream.into_split();
    let cwd = tmp_agent.path().to_string_lossy();
    let request = format!(
        r#"{{"cwd":"{}","intent":"seed via IPC","evidence":{{"compiled":true,"tests_passed":["synchronicity-engine"],"lints_clean":true,"runtime_ms":42}}}}"#,
        cwd
    );
    writer
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    writer.shutdown().await.unwrap();

    let mut lines = BufReader::new(reader).lines();
    let response_line = lines
        .next_line()
        .await
        .expect("read response")
        .expect("non-empty response");
    let resp: serde_json::Value =
        serde_json::from_str(&response_line).expect("valid JSON response");
    assert!(
        resp["ok"].as_bool().unwrap_or(false),
        "IPC response ok=false: {resp}"
    );
    let change_id_hex = resp["change_id"].as_str().expect("change_id string").to_string();
    let change_id = parse_change_id(&change_id_hex);
    let promoted_hex = resp["promoted"]
        .as_str()
        .expect("promoted id should be present after sync_all")
        .to_string();
    let promoted_id = parse_change_id(&promoted_hex);
    assert_ne!(
        change_id_hex, promoted_hex,
        "inner-braid and outer-DAG changeset ids must differ"
    );

    // `VaultManager::ensure_vault` builds its own `Vault::attach` instance —
    // the test's local `vault` handle shares the `Realm` but not the
    // inner-braid or outer-DAG state the IPC commit landed on. Query
    // the manager's vault for the ground truth.
    let realm_id = *vault.realm().id().as_bytes();
    assert!(
        vm_arc.outer_dag_contains(&realm_id, &promoted_id).await,
        "outer DAG missing promoted change {promoted_hex}"
    );
    assert!(
        !vm_arc.outer_dag_contains(&realm_id, &change_id).await,
        "inner-braid change id leaked onto outer DAG: {change_id_hex}"
    );

    // And the agent's file must materialize under the manager-owned
    // vault root (not `vault_dir` — that's the test-owned `Vault::create`
    // path, which never sees IPC commits).
    let vault_root = vm_arc
        .vault_path(&realm_id)
        .expect("manager vault path should be registered");
    let materialized = tokio::fs::read(vault_root.join("notes.md"))
        .await
        .expect("notes.md should be materialized at the vault root");
    assert_eq!(materialized, b"agent thoughts");

    net.stop().await.ok();
}

fn parse_change_id(hex: &str) -> indras_sync_engine::braid::ChangeId {
    assert_eq!(hex.len(), 64, "expected 64-char hex change id, got {hex:?}");
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).expect("hex utf8");
        bytes[i] = u8::from_str_radix(s, 16).expect("hex digit");
    }
    indras_sync_engine::braid::ChangeId(bytes)
}
