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
