//! Unix-socket IPC server for Claude Code agent integration.
//!
//! The Sync socket lets a Claude Code agent running in a bound
//! worktree trigger a commit without touching the Dioxus UI:
//!
//! ```text
//! echo '{"cwd":"/path/to/agent1","intent":"feat: add thing"}' | nc -U $SOCKET
//! → {"ok":true,"change_id":"abc12345..."}
//! ```
//!
//! Protocol: newline-delimited JSON, one request per connection, one
//! response, then close. The socket lives at
//! `{data_dir}/sync.sock` (macOS: `~/Library/Application Support/indras-network/sync.sock`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_sync_engine::braid::{PatchManifest, RealmBraid};
use indras_sync_engine::workspace::LocalWorkspaceIndex;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

use crate::vault_manager::VaultManager;

/// Well-known socket filename inside the data directory.
const SOCKET_FILENAME: &str = "sync.sock";

/// Incoming request from a Claude Code agent.
#[derive(Debug, Deserialize)]
struct SyncRequest {
    /// Absolute path to the agent's working directory. Matched against
    /// bound folder paths to identify which `WorkspaceHandle` to use.
    cwd: PathBuf,
    /// Commit intent (one-line imperative description). Required.
    intent: String,
}

/// Response sent back to the agent.
#[derive(Debug, Serialize)]
struct SyncResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    change_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl SyncResponse {
    fn success(change_id: String) -> Self {
        Self { ok: true, change_id: Some(change_id), error: None }
    }
    fn fail(reason: impl Into<String>) -> Self {
        Self { ok: false, change_id: None, error: Some(reason.into()) }
    }
}

/// Start the IPC server. Returns a `JoinHandle` that runs for the app's
/// lifetime; drop or abort to shut down. Removes any stale socket file
/// before binding.
pub fn start_ipc_server(
    data_dir: PathBuf,
    network: Arc<IndrasNetwork>,
    vault_manager: Arc<VaultManager>,
    indexes: Vec<Arc<LocalWorkspaceIndex>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let sock_path = data_dir.join(SOCKET_FILENAME);
        // Remove stale socket from a prior run.
        let _ = tokio::fs::remove_file(&sock_path).await;
        let listener = match UnixListener::bind(&sock_path) {
            Ok(l) => {
                tracing::info!(path = %sock_path.display(), "IPC sync socket listening");
                l
            }
            Err(e) => {
                tracing::error!(error = %e, path = %sock_path.display(), "failed to bind IPC socket");
                return;
            }
        };
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::debug!(error = %e, "IPC accept error");
                    continue;
                }
            };
            let net = Arc::clone(&network);
            let vm = Arc::clone(&vault_manager);
            let hs = indexes.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, &net, &vm, &hs).await {
                    tracing::debug!(error = %e, "IPC connection error");
                }
            });
        }
    })
}

/// Path to the IPC socket for a given data directory.
pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SOCKET_FILENAME)
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    network: &IndrasNetwork,
    vault_manager: &VaultManager,
    indexes: &[Arc<LocalWorkspaceIndex>],
) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let line = match lines.next_line().await? {
        Some(l) => l,
        None => return Ok(()),
    };
    let resp = match serde_json::from_str::<SyncRequest>(&line) {
        Ok(req) => process_request(req, network, vault_manager, indexes).await,
        Err(e) => SyncResponse::fail(format!("bad request: {e}")),
    };
    let mut out = serde_json::to_vec(&resp)?;
    out.push(b'\n');
    writer.write_all(&out).await?;
    Ok(())
}

async fn process_request(
    req: SyncRequest,
    network: &IndrasNetwork,
    vault_manager: &VaultManager,
    indexes: &[Arc<LocalWorkspaceIndex>],
) -> SyncResponse {
    if req.intent.trim().is_empty() {
        return SyncResponse::fail("intent is required");
    }

    let cwd = std::fs::canonicalize(&req.cwd)
        .unwrap_or_else(|_| req.cwd.clone());

    let index = indexes.iter().find(|idx| {
        let bound = std::fs::canonicalize(idx.root())
            .unwrap_or_else(|_| idx.root().to_path_buf());
        bound == cwd
    });
    let index = match index {
        Some(i) => i,
        None => {
            return SyncResponse::fail(format!(
                "no agent bound at {}",
                cwd.display()
            ));
        }
    };

    let files = index.snapshot_all().await;
    if files.is_empty() {
        return SyncResponse::fail("nothing to commit (empty index)");
    }
    let manifest = PatchManifest::new(files);

    let realm = match vault_manager.realms().await.into_iter().next() {
        Some(r) => r,
        None => return SyncResponse::fail("no vault realm on this device"),
    };

    let user_id = network.node().pq_identity().user_id();
    let workspace_root = index.root().to_path_buf();

    match realm
        .try_land(
            network,
            req.intent,
            manifest,
            Vec::new(),
            workspace_root,
            user_id,
        )
        .await
    {
        Ok(id) => SyncResponse::success(id.to_string()),
        Err(e) => SyncResponse::fail(format!("{e}")),
    }
}
