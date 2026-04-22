//! Sync panel — per-agent commit UI for the current device.
//!
//! Each row shows one bound agent's working-tree (`SyncStageView`),
//! an intent input, and a Commit button that snapshots the agent's
//! [`LocalWorkspaceIndex`] and lands it on the first vault's inner
//! braid via [`VaultManager::land_agent_snapshot`]. Commits
//! carry a placeholder `Evidence::Agent` payload for now — once there's
//! a UI affordance to report build/test outcomes, the evidence fields
//! can be wired through. Materialization + broadcast happen on
//! `Vault::promote`, not here.

use std::collections::HashMap;
use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_sync_engine::braid::changeset::Evidence;
use indras_sync_engine::braid::{derive_agent_id, ChangeId, PatchFile};

use indras_sync_engine::team::LogicalAgentId;
use indras_sync_engine::workspace::LocalWorkspaceIndex;

use crate::state::AppState;
use crate::team::WorkspaceHandle;
use crate::vault_manager::VaultManager;

use super::SyncStageView;

/// Status of a per-agent commit attempt, shown next to the Commit button.
#[derive(Clone, Debug, PartialEq)]
enum CommitStatus {
    /// Commit in flight.
    Running,
    /// Commit landed; carries the new changeset id (short-hex displayed).
    Done(ChangeId),
    /// Commit failed; carries a user-visible reason.
    Failed(String),
}

/// Pre-materialized row for one agent: the agent id plus the current
/// snapshot of its working-tree index (as `PatchFile`s).
pub type SyncPanelRow = (LogicalAgentId, Vec<PatchFile>);

/// Full-function sync panel. Renders one composite row per bound agent:
/// the stage view, an intent input, a Commit button, and commit status.
///
/// Commits target the first realm reported by `VaultManager::realms()`.
/// Multi-vault routing is deferred — MVP assumes one "project vault" per
/// device (see progress notes in the active plan).
#[component]
pub fn SyncPanel(
    rows: Vec<SyncPanelRow>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
    refresh: Signal<u32>,
) -> Element {
    let intents: Signal<HashMap<LogicalAgentId, String>> = use_signal(HashMap::new);
    let statuses: Signal<HashMap<LogicalAgentId, CommitStatus>> = use_signal(HashMap::new);

    rsx! {
        div { class: "sync-panel",
            if rows.is_empty() {
                div { class: "sync-panel-empty", "No agents bound on this device" }
            } else {
                div { class: "sync-panel-agents",
                    for (agent, files) in rows {
                        SyncAgentRow {
                            agent,
                            files,
                            intents,
                            statuses,
                            network,
                            vault_manager,
                            workspace_handles,
                            refresh,
                        }
                    }
                }
            }
        }
    }
}

/// One composite row: stage view + intent + Commit + status.
///
/// Broken out so each row's `onclick` captures `agent` cleanly without
/// fighting borrow semantics in a rsx for-loop.
#[component]
fn SyncAgentRow(
    agent: LogicalAgentId,
    files: Vec<PatchFile>,
    mut intents: Signal<HashMap<LogicalAgentId, String>>,
    mut statuses: Signal<HashMap<LogicalAgentId, CommitStatus>>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
    refresh: Signal<u32>,
) -> Element {
    let intent_val = intents
        .read()
        .get(&agent)
        .cloned()
        .unwrap_or_default();
    let status = statuses.read().get(&agent).cloned();
    let has_files = !files.is_empty();
    let running = matches!(status, Some(CommitStatus::Running));
    let commit_disabled = !has_files || intent_val.trim().is_empty() || running;

    let agent_for_intent = agent.clone();
    let agent_for_commit = agent.clone();

    rsx! {
        div { class: "sync-agent-row",
            SyncStageView { agent: agent.clone(), files }
            div { class: "sync-agent-controls",
                input {
                    class: "sync-intent-input",
                    r#type: "text",
                    value: "{intent_val}",
                    placeholder: "intent — what this commit does",
                    oninput: move |e| {
                        intents.write().insert(agent_for_intent.clone(), e.value());
                    },
                }
                button {
                    class: "se-btn-primary sync-commit-btn",
                    disabled: commit_disabled,
                    onclick: move |_| {
                        commit_for_agent(
                            agent_for_commit.clone(),
                            intents,
                            statuses,
                            network,
                            vault_manager,
                            workspace_handles,
                            refresh,
                        );
                    },
                    if running { "Committing…" } else { "Commit" }
                }
            }
            if let Some(s) = status {
                CommitStatusLine { status: s }
            }
        }
    }
}

/// Small HEAD indicator shown above the agent rows. Passive read of
/// the team realm's current braid DAG heads.
///
/// - Empty: "no commits yet" — the team realm has no head because
///   nothing has been committed (or the team realm hasn't been
///   materialized on this device yet).
/// - One head: the short-hex of that changeset.
/// - Multiple heads: "N concurrent heads" — agents have diverged; a
///   merge changeset will collapse them on the next commit.
#[component]
fn HeadIndicator(heads: Vec<ChangeId>) -> Element {
    let kind = match heads.len() {
        0 => "empty",
        1 => "single",
        _ => "multiple",
    };
    rsx! {
        div { class: "sync-head-indicator {kind}",
            span { class: "sync-head-label", "HEAD" }
            if heads.is_empty() {
                span { class: "sync-head-value", "no commits yet" }
            } else if heads.len() == 1 {
                span { class: "sync-head-value", "{short_head_hex(&heads[0])}" }
            } else {
                span { class: "sync-head-value", "{heads.len()} concurrent heads" }
            }
        }
    }
}

/// Short 8-hex rendering of a ChangeId (first 4 bytes), matching the
/// abbreviation style used elsewhere in this module.
fn short_head_hex(id: &ChangeId) -> String {
    let mut s = String::with_capacity(8);
    for b in &id.as_bytes()[..4] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Status line below the Commit button — separate component so the
/// status pill can re-render independently of the row's heavier bits.
#[component]
fn CommitStatusLine(status: CommitStatus) -> Element {
    match status {
        CommitStatus::Running => rsx! {
            div { class: "sync-commit-status running", "working…" }
        },
        CommitStatus::Done(id) => {
            let short: String = id
                .as_bytes()
                .iter()
                .take(4)
                .map(|b| format!("{b:02x}"))
                .collect();
            rsx! {
                div { class: "sync-commit-status done", "landed {short}" }
            }
        }
        CommitStatus::Failed(reason) => rsx! {
            div { class: "sync-commit-status failed", "{reason}" }
        },
    }
}

/// Kick off the async commit pipeline for `agent`:
/// 1. locate the bound agent's live index and the full agent roster in
///    `workspace_handles`,
/// 2. build `Evidence::Agent` signed by the device's PQ identity,
/// 3. call `VaultManager::land_agent_snapshot` (with `realm_id = None`),
///    which lifts the live index into a `SymlinkIndex` and lands it on
///    the first vault's inner (local-only) braid via `Vault::agent_land`,
/// 4. call `VaultManager::sync_all_on_first` to merge any other
///    diverged agent forks, promote the inner HEAD to the outer DAG,
///    auto-merge trusted peers, and materialize the resulting outer
///    HEAD to the vault root on disk,
/// 5. record the inner-braid `ChangeId` in `statuses` so the row
///    re-renders.
///
/// A failed `sync_all` is logged but does NOT roll back the landed
/// inner-braid commit — the user can retry promote later. Agents'
/// evidence payloads are not yet surfaced through this UI; they stay
/// at the placeholder (no compile/test/lint claim).
fn commit_for_agent(
    agent: LogicalAgentId,
    mut intents: Signal<HashMap<LogicalAgentId, String>>,
    mut statuses: Signal<HashMap<LogicalAgentId, CommitStatus>>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
    mut refresh: Signal<u32>,
) {
    let intent = intents
        .read()
        .get(&agent)
        .cloned()
        .unwrap_or_default();
    if intent.trim().is_empty() {
        return;
    }

    let net_opt = network.read().clone();
    let vm_opt = vault_manager.read().clone();
    let (index_opt, roster): (Option<Arc<LocalWorkspaceIndex>>, Vec<LogicalAgentId>) = {
        let handles = workspace_handles.read();
        let idx = handles
            .iter()
            .find(|h| h.agent == agent)
            .map(|h| Arc::clone(&h.index));
        let roster = handles.iter().map(|h| h.agent.clone()).collect();
        (idx, roster)
    };

    let (net, vm, index) = match (net_opt, vm_opt, index_opt) {
        (Some(n), Some(v), Some(i)) => (n, v, i),
        _ => {
            statuses.write().insert(
                agent.clone(),
                CommitStatus::Failed("not ready: missing network, vault, or binding".into()),
            );
            return;
        }
    };

    statuses.write().insert(agent.clone(), CommitStatus::Running);
    intents.write().remove(&agent);

    spawn(async move {
        // Pre-stage: snapshot every registered Project folder so that any
        // on-disk edits are captured as new manifest blobs before the
        // agent's working-tree commit lands on the braid.
        if let Err(e) = vm.snapshot_all_projects().await {
            tracing::warn!(error = %e, "snapshot_all_projects before commit failed");
        }

        let pq = net.node().pq_identity();
        let user_id = pq.user_id();
        let evidence = Evidence::Agent {
            compiled: false,
            tests_passed: Vec::new(),
            lints_clean: false,
            runtime_ms: 0,
            signed_by: derive_agent_id(&user_id, agent.as_str()),
        };
        let landed = vm
            .land_agent_snapshot(None, &agent, &index, intent.clone(), evidence)
            .await;
        let change_id = match landed {
            Ok(id) => id,
            Err(e) => {
                statuses.write().insert(agent, CommitStatus::Failed(e));
                return;
            }
        };
        // Full /sync-equivalent: merge agent forks, promote, auto-merge
        // trusted peers, materialize to the vault root. Any error here
        // is logged but doesn't roll back the landed inner-braid commit.
        if let Err(e) = vm.sync_all_on_first(intent, &roster).await {
            tracing::warn!(error = %e, "sync_all after land failed");
        }
        statuses.write().insert(agent, CommitStatus::Done(change_id));
        refresh += 1;
    });
}

/// Modal overlay wrapping [`SyncPanel`]. Gated by `AppState::show_sync`.
/// Mirrors the profile overlay class pattern (`file-modal-overlay` +
/// `file-modal` + `relay-eyebrow` / `relay-title` + `file-modal-close`)
/// so the sync modal inherits the global nocturnal dashboard styling
/// without a bespoke card definition.
///
/// Each render re-snapshots every bound agent's [`LocalWorkspaceIndex`]
/// via `use_resource`, so opening the overlay always shows the latest
/// working-tree state. No polling — the underlying indexes are kept
/// current by [`WorkspaceWatcher`] as the agent edits files.
#[component]
pub fn SyncOverlay(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
) -> Element {
    if !state.read().show_sync {
        return rsx! {};
    }

    // Look up current team-realm DAG heads for this device. Passive read:
    // never materializes a team realm — just reports what's there. The
    // list is empty until `team_realm_id` has been set on the vault doc
    // (via `ensure_team_realm` — currently only fires during commits in
    // the Phase-1 surface).
    let refresh: Signal<u32> = use_signal(|| 0);

    // Read HEAD from the braid DAG's per-peer head tracking.
    let head_resource = use_resource(move || async move {
        let _ = *refresh.read();
        let vm_opt = vault_manager.read().clone();
        let vm = match vm_opt {
            Some(v) => v,
            _ => return Vec::new(),
        };
        let Some(vault_realm) = vm.realms().await.into_iter().next() else {
            return Vec::new();
        };
        use indras_sync_engine::braid::RealmBraid;
        let dag = match vault_realm.braid_dag().await {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };
        let _ = dag.refresh().await;
        let guard = dag.read().await;
        guard.heads().into_iter().collect()
    });
    let heads: Vec<ChangeId> = head_resource
        .read()
        .as_ref()
        .cloned()
        .unwrap_or_default();

    // Snapshot each handle's index into a `SyncPanelRow`. Clones refs
    // synchronously (the Signal read guard can't cross the `await`),
    // then iterates the async boundary.
    let rows_resource = use_resource(move || async move {
        let pairs: Vec<(LogicalAgentId, Arc<LocalWorkspaceIndex>)> = workspace_handles
            .read()
            .iter()
            .map(|h| (h.agent.clone(), Arc::clone(&h.index)))
            .collect();
        let mut out: Vec<SyncPanelRow> = Vec::with_capacity(pairs.len());
        for (agent, index) in pairs {
            let files = index.snapshot_all().await;
            out.push((agent, files));
        }
        out
    });
    let rows: Vec<SyncPanelRow> = rows_resource
        .read()
        .as_ref()
        .cloned()
        .unwrap_or_default();

    let close = move |_| {
        state.write().show_sync = false;
    };

    rsx! {
        div {
            class: "file-modal-overlay",
            onclick: close,
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    state.write().show_sync = false;
                }
            },
            div {
                class: "file-modal sync-modal",
                onclick: move |e| e.stop_propagation(),
                div { class: "file-modal-header",
                    div { class: "relay-header-titles",
                        div { class: "relay-eyebrow", "TEAM" }
                        div { class: "relay-title", "Sync" }
                    }
                    button {
                        class: "file-modal-close",
                        onclick: close,
                        "\u{00D7}"
                    }
                }
                div { class: "file-modal-content relay-body",
                    HeadIndicator { heads }
                    SyncPanel { rows, network, vault_manager, workspace_handles, refresh }
                }
            }
        }
    }
}
