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
#[component]
pub fn SyncOverlay(mut state: Signal<AppState>) -> Element {
    if !state.read().show_sync {
        return rsx! {};
    }
    let rows: Vec<SyncPanelRow> = Vec::new();
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
