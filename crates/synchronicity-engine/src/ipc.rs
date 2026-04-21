//! Unix-socket IPC server for Claude Code agent integration.
//!
//! ## Commit requests
//!
//! The Sync socket lets a Claude Code agent running in a bound
//! worktree trigger a commit without touching the Dioxus UI:
//!
//! ```text
//! echo '{"cwd":"/path/to/agent1","intent":"feat: add thing"}' | nc -U $SOCKET
//! → {"ok":true,"change_id":"abc12345..."}
//! ```
//!
//! The agent may additionally attach `evidence` — compile/test/lint
//! outcomes from the work that produced the commit — which is carried
//! on the inner-braid changeset and later flows into the outer DAG on
//! promote:
//!
//! ```text
//! {"cwd":"…","intent":"…","evidence":{"compiled":true,"tests_passed":["indras-sync-engine"],"lints_clean":true,"runtime_ms":1820}}
//! ```
//!
//! ## Hook status events
//!
//! The `indras-agent-hook` binary sends lifecycle events on the same socket:
//!
//! ```text
//! {"kind":"agent_status","agent":"agent-foo","event":"PreToolUse","tool":"Read"}
//! ```
//!
//! These are handled by [`handle_agent_status`] and update
//! `AppState::agent_status` / `AppState::agent_last_activity_millis`
//! (fields added in Cluster 3; this cluster stubs to `tracing::debug!`).
//!
//! Protocol: newline-delimited JSON, one request per connection, one
//! response, then close. The socket lives at
//! `{data_dir}/sync.sock` (macOS: `~/Library/Application Support/indras-network/sync.sock`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use indras_network::IndrasNetwork;
use indras_sync_engine::braid::changeset::Evidence;
use indras_sync_engine::braid::derive_agent_id;
use indras_sync_engine::team::LogicalAgentId;
use indras_sync_engine::workspace::LocalWorkspaceIndex;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

use crate::vault_manager::VaultManager;

/// Well-known socket filename inside the data directory.
const SOCKET_FILENAME: &str = "sync.sock";

/// One bound agent folder the IPC server can route commits to.
///
/// Lighter-weight than a full [`crate::team::WorkspaceHandle`]: the IPC
/// server doesn't own the folder lock or watcher, it just needs the
/// agent id + a reference to the live index.
#[derive(Clone)]
pub struct IpcBinding {
    /// Logical agent this folder is bound to.
    pub agent: LogicalAgentId,
    /// The live working-tree index, kept current by the app's
    /// `WorkspaceWatcher`.
    pub index: Arc<LocalWorkspaceIndex>,
}

/// Lifecycle event variants emitted by the `indras-agent-hook` binary.
///
/// Matches the `--event` flag values Claude Code passes to the hook command.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "PascalCase")]
pub enum AgentHookEvent {
    /// The user submitted a new prompt to the agent.
    UserPromptSubmit,
    /// The agent is about to invoke a tool.
    PreToolUse {
        /// Name of the tool being invoked (e.g. `"Read"`, `"Edit"`).
        tool: String,
    },
    /// A tool invocation just completed successfully.
    PostToolUse {
        /// Name of the tool that completed.
        tool: String,
    },
    /// The agent session has ended (all turns complete or user stopped).
    Stop,
}

/// Incoming agent-status message from the `indras-agent-hook` binary.
///
/// Sent on the same unix socket as [`SyncRequest`]; distinguished by the
/// `"kind":"agent_status"` field. The handler updates `AppState` runtime
/// status fields (wired in Cluster 3; stubbed to a debug log until then).
#[derive(Debug, Deserialize)]
pub struct AgentStatusMessage {
    /// Full logical agent id (e.g. `"agent-foo"`).
    pub agent: String,
    /// Which lifecycle event fired.
    #[serde(flatten)]
    pub hook_event: AgentHookEvent,
}


/// Optional evidence payload the agent can attach to a commit.
///
/// Mirrors the `Evidence::Agent` variant minus `signed_by` (which is
/// filled in server-side from the device's PQ identity). All fields
/// optional — absent fields default to conservative values (no
/// compilation, no tests passed, no lints clean).
#[derive(Debug, Default, Deserialize)]
struct EvidencePayload {
    compiled: Option<bool>,
    tests_passed: Option<Vec<String>>,
    lints_clean: Option<bool>,
    runtime_ms: Option<u64>,
}

/// Incoming request from a Claude Code agent.
#[derive(Debug, Deserialize)]
struct SyncRequest {
    /// Absolute path to the agent's working directory. Matched against
    /// bound folder paths to identify which `IpcBinding` to use.
    cwd: PathBuf,
    /// Commit intent (one-line imperative description). Required.
    intent: String,
    /// Optional verification evidence produced by the agent for this
    /// commit. Absent fields default to "not verified".
    #[serde(default)]
    evidence: Option<EvidencePayload>,
}

/// Response sent back to the agent.
///
/// `change_id` is the inner-braid commit id. `promoted` is the outer-DAG
/// changeset id produced by the subsequent `sync_all` — present only
/// when the commit actually advanced the outer HEAD. `peer_merges` is
/// the count of trusted peer forks absorbed during the same sync, so
/// the agent can tell from one round-trip whether its work is now
/// visible to the network.
#[derive(Debug, Serialize)]
struct SyncResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    change_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    promoted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_merges: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl SyncResponse {
    fn success(
        change_id: String,
        promoted: Option<String>,
        peer_merges: usize,
    ) -> Self {
        Self {
            ok: true,
            change_id: Some(change_id),
            promoted,
            peer_merges: Some(peer_merges),
            error: None,
        }
    }
    fn fail(reason: impl Into<String>) -> Self {
        Self {
            ok: false,
            change_id: None,
            promoted: None,
            peer_merges: None,
            error: Some(reason.into()),
        }
    }
}

/// Start the IPC server. Returns a `JoinHandle` that runs for the app's
/// lifetime; drop or abort to shut down. Removes any stale socket file
/// before binding.
pub fn start_ipc_server(
    data_dir: PathBuf,
    network: Arc<IndrasNetwork>,
    vault_manager: Arc<VaultManager>,
    bindings: Vec<IpcBinding>,
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
            let bs = bindings.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, &net, &vm, &bs).await {
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
    bindings: &[IpcBinding],
) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let line = match lines.next_line().await? {
        Some(l) => l,
        None => return Ok(()),
    };

    // Dispatch on the optional `"kind"` field.
    // - `"kind":"agent_status"` → hook status event (no response expected).
    // - anything else (including no `kind`) → sync/commit request.
    let val: serde_json::Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(e) => {
            let resp = SyncResponse::fail(format!("bad JSON: {e}"));
            let mut out = serde_json::to_vec(&resp)?;
            out.push(b'\n');
            writer.write_all(&out).await?;
            return Ok(());
        }
    };

    if val.get("kind").and_then(|k| k.as_str()) == Some("agent_status") {
        // Hook status event — handle and close without writing a response.
        // (Claude Code hooks do not read the response.)
        match serde_json::from_value::<AgentStatusMessage>(val) {
            Ok(msg) => handle_agent_status(msg),
            Err(e) => tracing::debug!(error = %e, "malformed agent_status message"),
        }
        return Ok(());
    }

    // Sync/commit request path.
    let resp = match serde_json::from_value::<SyncRequest>(val) {
        Ok(req) => process_request(req, network, vault_manager, bindings).await,
        Err(e) => SyncResponse::fail(format!("bad request: {e}")),
    };
    let mut out = serde_json::to_vec(&resp)?;
    out.push(b'\n');
    writer.write_all(&out).await?;
    Ok(())
}

/// Handle an incoming agent hook-status event.
///
/// Updates `AppState::agent_status` and `AppState::agent_last_activity_millis`
/// (fields added in Cluster 3). Until those fields exist this function logs
/// the event at debug level so the hook binary can connect and the wiring is
/// exercised end-to-end without panics.
fn handle_agent_status(msg: AgentStatusMessage) {
    tracing::debug!(
        agent = %msg.agent,
        event = ?msg.hook_event,
        "agent hook event received (AppState wiring pending Cluster 3)"
    );
    // Cluster 3 will replace this stub with:
    //   state.write().agent_status.insert(agent_id, AgentRuntimeStatus::Thinking);
    //   state.write().agent_last_activity_millis.insert(agent_id, now_millis());
}

async fn process_request(
    req: SyncRequest,
    network: &IndrasNetwork,
    vault_manager: &VaultManager,
    bindings: &[IpcBinding],
) -> SyncResponse {
    if req.intent.trim().is_empty() {
        return SyncResponse::fail("intent is required");
    }

    let cwd = std::fs::canonicalize(&req.cwd)
        .unwrap_or_else(|_| req.cwd.clone());

    let binding = bindings.iter().find(|b| {
        let bound = std::fs::canonicalize(b.index.root())
            .unwrap_or_else(|_| b.index.root().to_path_buf());
        bound == cwd
    });
    let binding = match binding {
        Some(b) => b,
        None => {
            return SyncResponse::fail(format!(
                "no agent bound at {}",
                cwd.display()
            ));
        }
    };

    let pq = network.node().pq_identity();
    let user_id = pq.user_id();
    let signed_by = derive_agent_id(&user_id, binding.agent.as_str());
    let ev = req.evidence.unwrap_or_default();
    let evidence = Evidence::Agent {
        compiled: ev.compiled.unwrap_or(false),
        tests_passed: ev.tests_passed.unwrap_or_default(),
        lints_clean: ev.lints_clean.unwrap_or(false),
        runtime_ms: ev.runtime_ms.unwrap_or(0),
        signed_by,
    };

    let change_id = match vault_manager
        .land_agent_snapshot_on_first(
            &binding.agent,
            &binding.index,
            req.intent.clone(),
            evidence,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => return SyncResponse::fail(e),
    };

    // Every IPC commit is a full /sync-equivalent: merge agent forks,
    // promote if the inner HEAD advanced beyond the outer HEAD, pull
    // trusted peers, and materialize. The roster is every currently
    // bound agent on this device.
    let roster: Vec<LogicalAgentId> =
        bindings.iter().map(|b| b.agent.clone()).collect();
    let (promoted, peer_merges) = match vault_manager
        .sync_all_on_first(req.intent, &roster)
        .await
    {
        Ok(report) => (
            report.promoted.map(|id| id.to_string()),
            report.peer_merges.len(),
        ),
        Err(e) => {
            tracing::warn!(error = %e, "sync_all after land failed; returning land-only result");
            (None, 0)
        }
    };

    SyncResponse::success(change_id.to_string(), promoted, peer_merges)
}
