//! Inline mini-braid sparkline — 60–80 pixels wide, shown in realm rows and
//! file rows to summarize the last few changesets at a glance.
//!
//! Hover to see author+hex; click bubbles up through the normal row click
//! handler to open the Braid Drawer.

use dioxus::prelude::*;

use super::braid_graph::{render_braid_svg, BraidGraphCfg};
use crate::state::{BraidView, CommitView, PeerHeadView};

/// How many most-recent commits to include in the sparkline.
const SPARKLINE_WINDOW: usize = 6;

/// A mini braid strip for a realm or file.
///
/// Pass either the full realm braid (`view`) or a pre-filtered slice of
/// commits — the component will take the last `SPARKLINE_WINDOW` entries
/// and relayout slots from 0.
#[component]
pub fn BraidSparkline(
    /// The braid snapshot to sample recent commits from.
    view: Option<BraidView>,
    /// Optional path filter: only commits touching this path are shown.
    /// `None` means show all commits for the realm.
    #[props(default)]
    path_filter: Option<String>,
    /// Width in pixels.
    #[props(default = 72.0)]
    width: f32,
) -> Element {
    let Some(braid) = view else {
        // Empty placeholder that holds space so layout stays stable.
        return rsx! {
            span { class: "braid-sparkline empty", style: "width: {width}px;" }
        };
    };

    // If a path filter is supplied, keep only commits whose short_hex is one
    // of the commits in the drawer list (real filtering by path lives in the
    // bridge). For now: empty filter = whole realm; with filter we pass
    // through unchanged because the bridge should have scoped the view.
    let _ = path_filter;

    // Take last N commits, renumber slots, keep lanes.
    let window_start = braid.commits.len().saturating_sub(SPARKLINE_WINDOW);
    let recent: Vec<CommitView> = braid
        .commits
        .iter()
        .skip(window_start)
        .cloned()
        .collect();
    if recent.is_empty() {
        return rsx! {
            span { class: "braid-sparkline empty", style: "width: {width}px;" }
        };
    }

    // Remap slots to 0..len and lanes to a compact 0..n, preserving relative
    // ordering so the sparkline reads left-to-right.
    let min_slot = recent.iter().map(|c| c.slot).min().unwrap_or(0);
    let active_peers: Vec<PeerHeadView> = {
        let mut seen: Vec<[u8; 32]> = Vec::new();
        let mut out: Vec<PeerHeadView> = Vec::new();
        for c in &recent {
            if !seen.contains(&c.author_id) {
                seen.push(c.author_id);
                if let Some(p) = braid.peers.iter().find(|p| p.user_id == c.author_id) {
                    out.push(p.clone());
                }
            }
        }
        out
    };

    let remapped: Vec<CommitView> = recent
        .iter()
        .map(|c| {
            let mut cc = c.clone();
            cc.slot = c.slot - min_slot;
            cc.lane = active_peers
                .iter()
                .position(|p| p.user_id == c.author_id)
                .unwrap_or(0);
            cc
        })
        .collect();

    let mini = BraidView {
        realm_id: braid.realm_id,
        peers: active_peers,
        commits: remapped,
        pending_forks: Vec::new(),
        conflicts: Vec::new(),
    };

    let cfg = BraidGraphCfg {
        fixed_width: Some(width),
        ..BraidGraphCfg::sparkline()
    };
    let svg = render_braid_svg(&mini, &cfg);

    rsx! {
        span {
            class: "braid-sparkline",
            style: "width: {width}px;",
            dangerous_inner_html: "{svg}",
        }
    }
}
