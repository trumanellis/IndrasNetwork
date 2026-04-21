//! Agent Lane — shown inside the Private column when any vault's inner
//! braid has diverged agent heads.
//!
//! One row per (agent, realm) fork. Each row renders the agent name, a
//! tinted inline strand, a "N changes" meta, and a "merge" action pill.
//! Visual reference: `design/braid-prototype.html` lines 1204–1243.
//!
//! The merge button is wired as a no-op for now — merging an agent
//! HEAD into the user's inner HEAD is a Phase 1b-plus operation and
//! will land alongside the richer agent-work controls.

use dioxus::prelude::*;

use crate::state::{AgentForkView, AppState};

/// Fixed SVG viewBox width used by each agent strand. `preserveAspectRatio`
/// is set to "none" so the strand stretches to fill the row regardless of
/// the actual pixel width.
const STRAND_VIEW_W: u32 = 120;
const STRAND_VIEW_H: u32 = 14;

/// Render the Agent Lane strip. Hidden entirely when no forks exist.
#[component]
pub fn AgentLane(state: Signal<AppState>) -> Element {
    let forks = state.read().agent_forks.clone();
    if forks.is_empty() {
        return rsx! {};
    }

    let fork_count = forks.len();
    let title = if fork_count == 1 {
        "agents · 1 fork".to_string()
    } else {
        format!("agents · {fork_count} forks")
    };

    rsx! {
        div { class: "agent-lane",
            div { class: "agent-lane-title", "{title}" }
            for (idx, fork) in forks.iter().cloned().enumerate() {
                AgentRow { index: idx, fork }
            }
        }
    }
}

/// One `(agent, realm)` row inside the lane.
///
/// Accepts a row `index` to derive a unique SVG gradient id — Dioxus
/// `dangerous_inner_html` would otherwise share the id across rows and
/// the browser would fall back to the first row's gradient for every
/// strand.
#[component]
fn AgentRow(index: usize, fork: AgentForkView) -> Element {
    let grad_id = format!("agentStrand{index}");
    let changes_label = if fork.change_count == 1 {
        "1 change".to_string()
    } else {
        format!("{} changes", fork.change_count)
    };

    let svg = build_strand_svg(&grad_id, &fork);
    let tooltip = format!("{} · {}", fork.name, fork.head_short_hex);

    rsx! {
        div { class: "agent-row",
            span { class: "agent-name {fork.color_class}", "{fork.name}" }
            span {
                class: "agent-strand",
                title: "{tooltip}",
                dangerous_inner_html: "{svg}",
            }
            span { class: "agent-meta", "{changes_label}" }
            button {
                class: "agent-merge",
                title: "Merge agent HEAD into inner HEAD",
                // Merge wiring lands in a follow-up slice; for now this
                // is purely decorative so the row mirrors the prototype.
                onclick: move |_| {},
                "merge"
            }
        }
    }
}

/// Build a compact inline SVG for one agent strand. Uses the agent's
/// member-identity color and varies the curve shape/circle count based
/// on `change_count` so rows with more work look denser.
fn build_strand_svg(grad_id: &str, fork: &AgentForkView) -> String {
    let hex = fork.color_hex;
    let (path, circles) = if fork.change_count <= 1 {
        (
            format!("M 0 7 L 60 7 L {STRAND_VIEW_W} 7"),
            vec![(90u32, 7u32, 2.4f32)],
        )
    } else {
        (
            "M 0 7 C 30 2, 60 12, 90 7 S 120 6, 120 7".to_string(),
            vec![(30, 5, 2.0), (60, 9, 2.0), (90, 6, 2.4)],
        )
    };

    let mut circles_xml = String::new();
    for (cx, cy, r) in &circles {
        circles_xml.push_str(&format!(
            r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{hex}"/>"##
        ));
    }

    format!(
        r##"<svg viewBox="0 0 {STRAND_VIEW_W} {STRAND_VIEW_H}" width="100%" height="{STRAND_VIEW_H}" preserveAspectRatio="none">
<defs>
<linearGradient id="{grad_id}" x1="0" x2="1" y1="0" y2="0">
<stop offset="0" stop-color="{hex}" stop-opacity="0.1"/>
<stop offset="1" stop-color="{hex}" stop-opacity="0.8"/>
</linearGradient>
</defs>
<path d="{path}" stroke="url(#{grad_id})" stroke-width="1.5" fill="none" stroke-linecap="round"/>
{circles_xml}
</svg>"##
    )
}
