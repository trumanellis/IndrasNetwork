//! IPC sync socket integration test.
//!
//! Spins up a full commit pipeline (network + vault + local-workspace-index
//! + IPC server), writes a file into the agent's folder, sends a JSON
//! commit request over the unix socket, and asserts the response carries
//! a valid `change_id`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::vault::Vault;
use indras_sync_engine::workspace::{FolderLock, LocalWorkspaceIndex, WorkspaceWatcher};
use synchronicity_engine::ipc;
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
async fn ipc_commit_returns_change_id() {
    let tmp_data = TempDir::new().unwrap();
    let tmp_agent = TempDir::new().unwrap();

    let net = build_network("IPC", tmp_data.path()).await;
    let blob = build_blob_store(tmp_data.path()).await;

    // Create a vault so VaultManager::realms() returns something.
    let vault_dir = tmp_data.path().join("vaults").join("ipc-test-vault");
    tokio::fs::create_dir_all(&vault_dir).await.unwrap();
    let (vault, _invite) = Vault::create(
        &net,
        "ipc-test-vault",
        vault_dir.clone(),
        Arc::clone(&blob),
    )
    .await
    .expect("Vault::create");

    // Build a VaultManager and register the vault.
    let vm = VaultManager::new(tmp_data.path().to_path_buf())
        .await
        .expect("VaultManager::new");
    vm.ensure_vault(net.as_ref(), vault.realm(), Some("ipc-test-vault"))
        .await
        .expect("ensure_vault");
    let vm_arc = Arc::new(vm);

    // Set up agent folder with local index + watcher.
    let _lock = FolderLock::acquire(tmp_agent.path()).expect("lock");
    let index = Arc::new(LocalWorkspaceIndex::new(
        tmp_agent.path().to_path_buf(),
        Arc::clone(&blob),
    ));
    let _watcher = WorkspaceWatcher::start(Arc::clone(&index)).expect("watcher");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Write a file and wait for the index.
    tokio::fs::write(tmp_agent.path().join("hello.rs"), b"fn main() {}")
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if index.get("hello.rs").await.is_some() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "index never saw hello.rs"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Start the IPC server with this agent's index.
    let _server = ipc::start_ipc_server(
        tmp_data.path().to_path_buf(),
        Arc::clone(&net),
        Arc::clone(&vm_arc),
        vec![Arc::clone(&index)],
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect and send a commit request.
    let sock_path = ipc::socket_path(tmp_data.path());
    let stream = UnixStream::connect(&sock_path)
        .await
        .expect("connect to IPC socket");
    let (reader, mut writer) = stream.into_split();

    let cwd = tmp_agent.path().to_string_lossy();
    let request = format!(r#"{{"cwd":"{}","intent":"test commit via IPC"}}"#, cwd);
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
        "IPC response should be ok=true; got: {resp}"
    );
    assert!(
        resp["change_id"].as_str().is_some(),
        "response must carry a change_id"
    );
    let change_id = resp["change_id"].as_str().unwrap();
    assert!(
        change_id.len() >= 8,
        "change_id should be a hex string, got: {change_id}"
    );

    net.stop().await.ok();
}
