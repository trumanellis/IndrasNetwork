//! Sync panel — container that lays out one [`SyncStageView`] per hosted
//! agent.
//!
//! Stateless on purpose: takes a pre-snapshotted list of rows. The
//! parent is responsible for fetching each agent's `LocalWorkspaceIndex`
//! snapshot (async) and passing the result down. This keeps the panel
//! pure and lets different call sites drive snapshot cadence however
//! they need (use_resource, polling, on-demand Refresh button, …).

use dioxus::prelude::*;
use indras_sync_engine::braid::PatchFile;
use indras_sync_engine::team::LogicalAgentId;

use crate::state::AppState;

use super::SyncStageView;

/// Pre-materialized row for one agent: the agent id plus the current
/// snapshot of its working-tree index (as `PatchFile`s).
pub type SyncPanelRow = (LogicalAgentId, Vec<PatchFile>);

/// Sync panel container. Renders one [`SyncStageView`] per row. Empty
/// state renders "No agents bound" so the panel is still rendered when
/// a device hosts no team agents (common on read-only devices).
#[component]
pub fn SyncPanel(rows: Vec<SyncPanelRow>) -> Element {
    rsx! {
        div { class: "sync-panel",
            div { class: "sync-panel-header", "Sync" }
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
/// Click the backdrop or the close button to dismiss. Until the commit
/// flow lands (subtask 1.3), rows is always empty and the panel shows
/// its "No agents bound" state.
#[component]
pub fn SyncOverlay(mut state: Signal<AppState>) -> Element {
    if !state.read().show_sync {
        return rsx! {};
    }
    let rows: Vec<SyncPanelRow> = Vec::new();

    rsx! {
        div {
            class: "sync-overlay-backdrop",
            onclick: move |_| {
                state.write().show_sync = false;
            },
            div {
                class: "sync-overlay-card",
                onclick: move |e| e.stop_propagation(),
                div { class: "sync-overlay-header",
                    span { class: "sync-overlay-title", "Sync" }
                    button {
                        class: "sync-overlay-close",
                        title: "Close",
                        onclick: move |_| {
                            state.write().show_sync = false;
                        },
                        "\u{00D7}"
                    }
                }
                SyncPanel { rows }
            }
        }
    }
}
