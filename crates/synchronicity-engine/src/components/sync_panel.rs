//! Sync panel — container that lays out one [`SyncStageView`] per hosted
//! agent.
//!
//! Stateless on purpose: takes a pre-snapshotted list of rows. The
//! parent is responsible for fetching each agent's `LocalWorkspaceIndex`
//! snapshot (async) and passing the result down. This keeps the panel
//! pure and lets different call sites drive snapshot cadence however
//! they need (use_resource, polling, on-demand Refresh button, …).

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_sync_engine::braid::PatchFile;
use indras_sync_engine::team::LogicalAgentId;
use indras_sync_engine::workspace::LocalWorkspaceIndex;

use crate::state::AppState;
use crate::team::WorkspaceHandle;

use super::SyncStageView;

/// Pre-materialized row for one agent: the agent id plus the current
/// snapshot of its working-tree index (as `PatchFile`s).
pub type SyncPanelRow = (LogicalAgentId, Vec<PatchFile>);

/// Sync panel content. Renders one [`SyncStageView`] per row; shows an
/// empty state when no agents are bound on this device (common on
/// read-only devices). Meant to be composed inside an overlay or column
/// layout — it renders only the inner content, not a header or chrome.
#[component]
pub fn SyncPanel(rows: Vec<SyncPanelRow>) -> Element {
    rsx! {
        div { class: "sync-panel",
            if rows.is_empty() {
                div { class: "sync-panel-empty", "No agents bound on this device" }
            } else {
                div { class: "sync-panel-agents",
                    for (agent, files) in rows {
                        SyncStageView { agent, files }
                    }
                }
            }
        }
    }
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
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
) -> Element {
    // Silence unused-prop warnings until 1.3c wires commit — keeping the
    // prop visible on the signature keeps the App → HomeVault → Overlay
    // chain stable as we layer in try_land.
    let _ = network;

    if !state.read().show_sync {
        return rsx! {};
    }

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
                    SyncPanel { rows }
                }
            }
        }
    }
}
