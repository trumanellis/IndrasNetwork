//! Right-docked Braid Drawer — the detailed view of a realm's (or file's)
//! braid: inline DAG graph, per-peer HEADs, pending merges, conflicts, and a
//! reverse-chronological commit list.
//!
//! Opens when [`crate::state::AppState::braid_drawer_open`] is `true`.
//! Data comes from [`crate::state::BraidView`] — pre-computed by a bridge
//! task so the drawer renders synchronously.
//!
//! When `braid_drawer_focus` is [`crate::state::BraidFocus::AgentReview`], the
//! drawer renders an agent review panel instead of the braid graph.

use std::sync::Arc;

use dioxus::prelude::*;

use super::braid_graph::{BraidGraph, BraidGraphCfg};
use crate::state::{
    AppState, BraidFocus, CommitView, ConflictView, EvidenceView, PeerHeadView,
};
use crate::team::WorkspaceHandle;
use crate::vault_manager::VaultManager;

/// Right-docked braid panel. Closed unless `state.braid_drawer_open`.
#[component]
pub fn BraidDrawer(
    mut state: Signal<AppState>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
) -> Element {
    if !state.read().braid_drawer_open {
        return rsx! {};
    }

    // ── Agent Review branch ──────────────────────────────────────────────────
    // Check this before the BraidView path so AgentReview renders even when
    // no braid_view is cached.
    if let Some(BraidFocus::AgentReview { agent }) =
        state.read().braid_drawer_focus.clone()
    {
        let fork = state
            .read()
            .agent_forks
            .iter()
            .find(|f| indras_sync_engine::team::LogicalAgentId::new(&f.name) == agent)
            .cloned();

        return rsx! {
            aside { class: "braid-drawer",
                DrawerHeader { state, title_name: Some(agent.as_str().to_string()) }
                AgentReviewBody {
                    state,
                    vault_manager,
                    workspace_handles,
                    agent_name: agent.as_str().to_string(),
                    fork,
                }
            }
        };
    }

    // ── Normal braid view branch ─────────────────────────────────────────────
    let Some(view) = state.read().braid_view.clone() else {
        return rsx! {
            aside { class: "braid-drawer",
                DrawerHeader { state, title_name: None }
                div { class: "braid-drawer-body",
                    div { class: "braid-drawer-empty",
                        "Select a realm or file to see its braid."
                    }
                }
            }
        };
    };

    let title_name = drawer_title_for_focus(&state);
    let is_empty = view.commits.is_empty();

    rsx! {
        aside { class: "braid-drawer",
            DrawerHeader { state, title_name }
            div { class: "braid-drawer-body",
                if is_empty {
                    NoBraidYet {}
                } else {
                    // Inline braid graph at the top — compact drawer scale.
                    div { class: "braid-graph",
                        BraidGraph { view: view.clone(), cfg: BraidGraphCfg::drawer() }
                    }

                    if !view.peers.is_empty() {
                        PeerHeadsSection { heads: view.peers.clone() }
                    }
                    if !view.pending_forks.is_empty() {
                        PendingMergesSection { forks: view.pending_forks.clone() }
                    }
                    if !view.conflicts.is_empty() {
                        ConflictsSection { conflicts: view.conflicts.clone() }
                    }
                    RecentCommitsSection { commits: view.commits.clone() }
                }
            }
        }
    }
}

// ── Agent Review Panel ────────────────────────────────────────────────────────

/// Agent review body — shows uncommitted changeset cards for an agent with
/// `[discard]` and `[merge HEAD]` actions.
#[component]
fn AgentReviewBody(
    mut state: Signal<AppState>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
    agent_name: String,
    fork: Option<crate::state::AgentForkView>,
) -> Element {
    rsx! {
        div { class: "braid-drawer-body agent-review-body",
            if let Some(ref fork) = fork {
                // Changeset summary card
                div { class: "agent-review-card",
                    div {
                        class: "agent-review-card-dot {fork.color_class}",
                        style: "background:{fork.color_hex}",
                    }
                    div { class: "agent-review-card-info",
                        div { class: "agent-review-card-name", "{fork.name}" }
                        {
                            let plural = if fork.change_count == 1 { "" } else { "s" };
                            let meta = format!("{} changeset{} · {}", fork.change_count, plural, fork.head_short_hex);
                            rsx! { div { class: "agent-review-card-meta", "{meta}" } }
                        }
                    }
                }
            } else {
                div { class: "braid-drawer-empty",
                    "No uncommitted changes for this agent."
                }
            }

            // Action buttons — always visible so user can dismiss even with no fork
            div { class: "agent-review-actions",
                button {
                    class: "agent-review-btn agent-review-btn--discard",
                    title: "Discard this agent's uncommitted changes (removes from review queue)",
                    onclick: {
                        let agent_name_d = agent_name.clone();
                        move |_| {
                            discard_agent_fork(&mut state, &agent_name_d);
                        }
                    },
                    "discard"
                }
                button {
                    class: "agent-review-btn agent-review-btn--merge",
                    title: "Merge agent HEAD into the inner braid",
                    onclick: {
                        let agent_name_m = agent_name.clone();
                        move |_| {
                            merge_agent_head(
                                &mut state,
                                &workspace_handles,
                                &vault_manager,
                                &agent_name_m,
                            );
                        }
                    },
                    "merge HEAD"
                }
            }
        }
    }
}

/// Remove the agent's fork entry from `AppState::agent_forks` and close the
/// drawer. The agent's on-disk files are untouched; the row returns to `Idle`.
fn discard_agent_fork(state: &mut Signal<AppState>, agent_name: &str) {
    let mut w = state.write();
    w.agent_forks.retain(|f| f.name != agent_name);
    w.braid_drawer_open = false;
    w.braid_drawer_focus = None;
}

/// Land the agent's working tree snapshot into the inner braid and close the
/// drawer. Uses the same `land_agent_snapshot_on_first` path as the `[land]`
/// pill on the agent row.
fn merge_agent_head(
    state: &mut Signal<AppState>,
    workspace_handles: &Signal<Vec<WorkspaceHandle>>,
    vault_manager: &Signal<Option<Arc<VaultManager>>>,
    agent_name: &str,
) {
    use indras_sync_engine::team::LogicalAgentId;

    let agent_id = LogicalAgentId::new(agent_name);
    let vm_opt = vault_manager.read().clone();
    let handle = workspace_handles
        .read()
        .iter()
        .find(|h| h.agent == agent_id)
        .map(|h| (h.agent.clone(), Arc::clone(&h.index)));

    // Close the drawer immediately for snappy UX; the async land runs in background
    {
        let mut w = state.write();
        w.braid_drawer_open = false;
        w.braid_drawer_focus = None;
        // Clear fork from the queue — it will reappear if land fails
        w.agent_forks.retain(|f| f.name != agent_name);
    }

    let agent_id_c = agent_id.clone();
    spawn(async move {
        let Some(vm) = vm_opt else { return };
        let Some((_aid, index)) = handle else { return };

        let intent = format!("agent merge HEAD: {}", agent_id_c.as_str());
        let signed_by: indras_sync_engine::vault::vault_file::UserId = {
            let mut arr = [0u8; 32];
            let b = agent_id_c.as_str().as_bytes();
            let n = b.len().min(32);
            arr[..n].copy_from_slice(&b[..n]);
            arr
        };
        let evidence = indras_sync_engine::braid::changeset::Evidence::Agent {
            compiled: false,
            tests_passed: vec![],
            lints_clean: false,
            runtime_ms: 0,
            signed_by,
        };
        if let Err(e) = vm
            .land_agent_snapshot_on_first(&agent_id_c, &index, intent, evidence)
            .await
        {
            tracing::warn!(agent = %agent_id_c.as_str(), error = %e, "merge_agent_head failed");
        }
    });
}

// ── Shared helpers ─────────────────────────────────────────────────────────────

/// Placeholder shown when the realm's DAG has no changesets yet — typical
/// for a freshly-created DM or group where nobody has called sync. The
/// braid appears once the first commit lands.
#[component]
fn NoBraidYet() -> Element {
    rsx! {
        div { class: "braid-no-yet",
            div { class: "braid-no-yet-glyph",
                svg {
                    view_box: "0 0 80 40",
                    width: "80",
                    height: "40",
                    dangerous_inner_html: r##"<defs><linearGradient id="nby" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#818cf8" stop-opacity=".1"/><stop offset=".5" stop-color="#c084fc" stop-opacity=".5"/><stop offset="1" stop-color="#f472b6" stop-opacity=".1"/></linearGradient></defs><path d="M 4 12 C 20 12, 30 28, 40 20 C 50 12, 60 28, 76 28" stroke="url(#nby)" stroke-width="1.5" fill="none" stroke-dasharray="2 3"/><path d="M 4 28 C 20 28, 30 12, 40 20 C 50 28, 60 12, 76 12" stroke="url(#nby)" stroke-width="1.5" fill="none" stroke-dasharray="2 3"/>"##,
                }
            }
            div { class: "braid-no-yet-title", "No braid yet" }
            div { class: "braid-no-yet-body",
                "This realm's DAG is empty — sync to create the first changeset and begin the weave."
            }
        }
    }
}

fn drawer_title_for_focus(state: &Signal<AppState>) -> Option<String> {
    let st = state.read();
    let focus = st.braid_drawer_focus.as_ref()?;
    let realm_id = focus.realm_id()?;
    st.realms.iter().find(|r| &r.id == realm_id).map(|r| r.display_name.clone())
}

#[component]
fn DrawerHeader(mut state: Signal<AppState>, title_name: Option<String>) -> Element {
    rsx! {
        div { class: "braid-drawer-header",
            span { class: "braid-drawer-title", "braid" }
            if let Some(name) = title_name {
                span { class: "braid-drawer-realm", "{name}" }
            }
            button {
                class: "braid-drawer-close",
                "aria-label": "Close braid drawer",
                onclick: move |_| {
                    state.write().braid_drawer_open = false;
                },
                "×"
            }
        }
    }
}

#[component]
fn PeerHeadsSection(heads: Vec<PeerHeadView>) -> Element {
    let count = heads.len();
    rsx! {
        div { class: "braid-section",
            div { class: "braid-section-title",
                "peer heads"
                span { class: "braid-count", "{count}" }
            }
            for head in heads {
                {
                    let row_class = if head.is_self { "peer-head-row you" } else { "peer-head-row" };
                    let dot_style = format!("background: {}", head.color);
                    let letter = head.name.chars().next().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
                    rsx! {
                        div {
                            class: "{row_class}",
                            key: "{head.head_short_hex}",
                            span { class: "peer-head-dot", style: "{dot_style}", "{letter}" }
                            span { class: "peer-head-name", "{head.name}" }
                            span { class: "peer-head-hash",
                                "{head.head_short_id} · {head.head_short_hex}"
                            }
                            span { class: "peer-head-meta",
                                "{head.file_count} files · {head.relative_time}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PendingMergesSection(forks: Vec<PeerHeadView>) -> Element {
    let count = forks.len();
    rsx! {
        div { class: "braid-section",
            div { class: "braid-section-title",
                "pending merges"
                span { class: "braid-count", "{count}" }
            }
            for fork in forks {
                {
                    let dot_style = format!("background: {}", fork.color);
                    let letter = fork.name.chars().next().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
                    rsx! {
                        div { class: "fork-row", key: "{fork.head_short_hex}",
                            span { class: "peer-head-dot lg", style: "{dot_style}", "{letter}" }
                            div { class: "fork-info",
                                div { class: "fork-author", "{fork.name} · {fork.file_count} files changed" }
                                div { class: "fork-detail", "{fork.head_short_id} · {fork.head_short_hex}" }
                            }
                            button { class: "btn-merge trusted", "merge" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConflictsSection(conflicts: Vec<ConflictView>) -> Element {
    let count = conflicts.len();
    rsx! {
        div { class: "braid-section",
            div { class: "braid-section-title warn",
                "conflicts"
                span { class: "braid-count warn", "{count}" }
            }
            for c in conflicts {
                {
                    let theirs_label = format!("theirs ({})", c.theirs_peer);
                    rsx! {
                        div { class: "conflict-row", key: "{c.path}",
                            div { class: "conflict-path", "{c.path}" }
                            div { class: "conflict-versions",
                                div { class: "conflict-side",
                                    div { class: "conflict-side-label", "ours" }
                                    div { class: "conflict-side-hash", "{c.ours_hex}" }
                                }
                                div { class: "conflict-side",
                                    div { class: "conflict-side-label", "{theirs_label}" }
                                    div { class: "conflict-side-hash", "{c.theirs_hex}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RecentCommitsSection(commits: Vec<CommitView>) -> Element {
    let count = commits.len();
    rsx! {
        div { class: "braid-section",
            div { class: "braid-section-title",
                "recent commits"
                span { class: "braid-count", "{count}" }
            }
            for c in commits {
                {
                    let knot_style = format!("background: {}; color: {};", c.author_color, c.author_color);
                    let merge_suffix = if c.is_merge { " · merge" } else { "" };
                    rsx! {
                        div { class: "commit-row", key: "{c.short_hex}",
                            span { class: "commit-knot", style: "{knot_style}" }
                            div { class: "commit-body",
                                div { class: "commit-intent", "{c.intent}" }
                                div { class: "commit-meta",
                                    span { class: "commit-hash", "{c.short_id} · {c.short_hex}" }
                                    span { class: "commit-author", "{c.author_name} · {c.relative_time}{merge_suffix}" }
                                }
                                EvidenceBadge { evidence: c.evidence.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvidenceBadge(evidence: EvidenceView) -> Element {
    match evidence {
        EvidenceView::AgentPass { summary } => rsx! {
            span { class: "evidence agent-pass", "● {summary}" }
        },
        EvidenceView::AgentFail { reason } => rsx! {
            span { class: "evidence agent-fail", "✕ {reason}" }
        },
        EvidenceView::Human { message } => {
            let text = message.unwrap_or_else(|| "human approved".into());
            rsx! {
                span { class: "evidence human", "✦ {text}" }
            }
        }
    }
}
