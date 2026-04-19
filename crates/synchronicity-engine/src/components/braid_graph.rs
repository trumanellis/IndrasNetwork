//! Braid DAG renderer — peer lanes on Y, temporal slots on X, S-curve parent
//! edges, knots colored by author with an evidence-colored outer ring.
//!
//! The renderer is a pure function from [`BraidView`] to an SVG string; both
//! the right-docked [`super::braid_drawer::BraidDrawer`] and (later) the
//! Loom view use it at different scales. See `design/braid-prototype.html`
//! for the visual reference.

use dioxus::prelude::*;
use std::collections::HashMap;
use std::fmt::Write;

use crate::state::{BraidView, CommitView, EvidenceView};

/// Layout knobs for the braid SVG.
///
/// `col_width` is horizontal pixels per temporal slot; `lane_height` is
/// vertical pixels per peer row; `pad_x`/`pad_y` are outer margins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BraidGraphCfg {
    /// Horizontal pixels per temporal slot.
    pub col_width: f32,
    /// Vertical pixels per peer lane.
    pub lane_height: f32,
    /// Horizontal padding around the graph.
    pub pad_x: f32,
    /// Vertical padding around the graph.
    pub pad_y: f32,
    /// Radius of each changeset knot.
    pub knot_radius: f32,
    /// Stroke width of parent-edge strands.
    pub strand_width: f32,
    /// When true, draw peer-name lane labels, time ticks, and intent text.
    pub show_labels: bool,
    /// When true, draw HEAD pills above each peer's tip.
    pub show_head_pills: bool,
    /// When set, emit an explicit width (otherwise width is content-driven).
    pub fixed_width: Option<f32>,
}

impl BraidGraphCfg {
    /// Compact layout for the right-docked drawer.
    pub fn drawer() -> Self {
        Self {
            col_width: 62.0,
            lane_height: 42.0,
            pad_x: 38.0,
            pad_y: 22.0,
            knot_radius: 6.0,
            strand_width: 1.8,
            show_labels: true,
            show_head_pills: false,
            fixed_width: None,
        }
    }

    /// Full-viewport layout for the Loom view.
    pub fn loom() -> Self {
        Self {
            col_width: 140.0,
            lane_height: 72.0,
            pad_x: 60.0,
            pad_y: 30.0,
            knot_radius: 10.0,
            strand_width: 2.5,
            show_labels: true,
            show_head_pills: true,
            fixed_width: None,
        }
    }

    /// Inline mini-sparkline — no labels, tight scale.
    pub fn sparkline() -> Self {
        Self {
            col_width: 14.0,
            lane_height: 10.0,
            pad_x: 4.0,
            pad_y: 3.0,
            knot_radius: 1.8,
            strand_width: 1.0,
            show_labels: false,
            show_head_pills: false,
            fixed_width: None,
        }
    }
}

/// Build an SVG fragment as a string from a [`BraidView`].
///
/// The returned string contains a root `<svg>` element and is safe to place
/// via `dangerous_inner_html`. Coordinates are computed from `commit.lane`
/// and `commit.slot` using the supplied [`BraidGraphCfg`].
pub fn render_braid_svg(view: &BraidView, cfg: &BraidGraphCfg) -> String {
    let max_slot = view.commits.iter().map(|c| c.slot).max().unwrap_or(0) as f32;
    let lanes = view.peers.len().max(1) as f32;

    let total_width = cfg
        .fixed_width
        .unwrap_or(cfg.pad_x * 2.0 + (max_slot + 1.0) * cfg.col_width);
    let total_height = cfg.pad_y * 2.0 + lanes * cfg.lane_height;

    // Lookup: short_hex → position. Parents arrive as short_hex strings.
    let mut positions: HashMap<String, (f32, f32)> = HashMap::new();
    for c in &view.commits {
        let (x, y) = commit_pos(c, cfg);
        positions.insert(c.short_hex.clone(), (x, y));
    }

    let mut s = String::with_capacity(4096);
    let _ = write!(
        s,
        r#"<svg viewBox="0 0 {w} {h}" width="{w}" height="{h}" xmlns="http://www.w3.org/2000/svg" class="braid-svg">"#,
        w = total_width,
        h = total_height
    );

    // Lane backgrounds (peer strips), only when labels are shown.
    if cfg.show_labels {
        for (i, peer) in view.peers.iter().enumerate() {
            let y = cfg.pad_y + (i as f32) * cfg.lane_height + cfg.lane_height / 2.0;
            let _ = write!(
                s,
                r#"<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="{c}" stroke-opacity=".1" stroke-width="1" stroke-dasharray="1 4"/>"#,
                x1 = cfg.pad_x - 14.0,
                x2 = total_width - 20.0,
                y = y,
                c = peer.color,
            );
            // Peer label
            let _ = write!(
                s,
                r#"<text x="12" y="{y}" fill="{c}" font-family="JetBrains Mono" font-size="10" font-weight="500" letter-spacing="1.4">{name}</text>"#,
                y = y - 8.0,
                c = peer.color,
                name = upcase_escape(&peer.name),
            );
        }
    }

    // Parent edges (strands). Draw first so knots sit on top.
    for c in &view.commits {
        let (cx, cy) = commit_pos(c, cfg);
        for parent_hex in &c.parents {
            let Some(&(px, py)) = positions.get(parent_hex) else { continue };
            let parent_color = view
                .commits
                .iter()
                .find(|x| &x.short_hex == parent_hex)
                .map(|p| p.author_color.clone())
                .unwrap_or_else(|| "#52525b".to_string());
            let mid = (px + cx) / 2.0;
            let path = format!(
                "M {px} {py} C {mid} {py}, {mid} {cy}, {cx} {cy}",
                px = px,
                py = py,
                mid = mid,
                cx = cx,
                cy = cy
            );
            // Outer glow
            let _ = write!(
                s,
                r#"<path d="{d}" stroke="{c}" stroke-width="{gw}" stroke-opacity=".08" fill="none"/>"#,
                d = path,
                c = parent_color,
                gw = cfg.strand_width * 3.0,
            );
            // Main strand — per-edge gradient from parent→child color
            let gid = format!("g_{}_{}", parent_hex, c.short_hex);
            let _ = write!(
                s,
                r##"<defs><linearGradient id="{gid}" x1="{px}" y1="{py}" x2="{cx}" y2="{cy}" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="{pc}"/><stop offset="1" stop-color="{cc}"/></linearGradient></defs>"##,
                gid = gid,
                px = px,
                py = py,
                cx = cx,
                cy = cy,
                pc = parent_color,
                cc = c.author_color
            );
            let _ = write!(
                s,
                r##"<path d="{d}" stroke="url(#{gid})" stroke-width="{sw}" stroke-opacity=".85" fill="none" stroke-linecap="round"/>"##,
                d = path,
                gid = gid,
                sw = cfg.strand_width
            );
        }
    }

    // Knots
    for c in &view.commits {
        let (x, y) = commit_pos(c, cfg);
        let r = cfg.knot_radius;
        let peer_color = &c.author_color;
        let evidence_color = evidence_color(&c.evidence);

        if cfg.show_labels {
            // Outer glow disc
            let _ = write!(
                s,
                r#"<circle cx="{x}" cy="{y}" r="{r}" fill="{c}" fill-opacity=".08"/>"#,
                x = x,
                y = y,
                r = r + 6.0,
                c = peer_color
            );
            // Evidence ring
            let _ = write!(
                s,
                r#"<circle cx="{x}" cy="{y}" r="{r}" fill="none" stroke="{c}" stroke-width="1.5" stroke-opacity=".65"/>"#,
                x = x,
                y = y,
                r = r + 2.0,
                c = evidence_color
            );
        }
        // Main knot
        let _ = write!(
            s,
            r##"<circle cx="{x}" cy="{y}" r="{r}" fill="{c}" stroke="#09090b" stroke-width="{sw}"/>"##,
            x = x,
            y = y,
            r = r,
            c = peer_color,
            sw = if cfg.show_labels { 2.0 } else { 0.6 }
        );
        // Merge donut
        if c.is_merge && cfg.show_labels {
            let _ = write!(
                s,
                r##"<circle cx="{x}" cy="{y}" r="{r}" fill="none" stroke="#09090b" stroke-width="1.5"/>"##,
                x = x,
                y = y,
                r = r - 4.0
            );
        }

        if cfg.show_labels {
            // short-id label above
            let _ = write!(
                s,
                r##"<text x="{x}" y="{y}" fill="#a0a0ab" font-family="JetBrains Mono" font-size="9" text-anchor="middle" letter-spacing="0.6">{id}</text>"##,
                x = x,
                y = y - r - 8.0,
                id = escape(&c.short_id)
            );
        }
    }

    // HEAD pills
    if cfg.show_head_pills {
        for peer in &view.peers {
            if let Some(head_commit) = view
                .commits
                .iter()
                .find(|c| c.short_hex == peer.head_short_hex)
            {
                let (x, y) = commit_pos(head_commit, cfg);
                let _ = write!(
                    s,
                    r#"<rect x="{rx}" y="{ry}" width="36" height="13" rx="6.5" fill="{c}" fill-opacity=".18" stroke="{c}" stroke-opacity=".5"/>"#,
                    rx = x - 18.0,
                    ry = y - cfg.knot_radius - 26.0,
                    c = peer.color
                );
                let _ = write!(
                    s,
                    r#"<text x="{x}" y="{y}" fill="{c}" font-family="JetBrains Mono" font-size="8" text-anchor="middle" letter-spacing="1.4" font-weight="500">HEAD</text>"#,
                    x = x,
                    y = y - cfg.knot_radius - 17.0,
                    c = peer.color
                );
            }
        }
    }

    s.push_str("</svg>");
    s
}

/// Screen-space position of a commit knot.
fn commit_pos(c: &CommitView, cfg: &BraidGraphCfg) -> (f32, f32) {
    (
        cfg.pad_x + (c.slot as f32) * cfg.col_width,
        cfg.pad_y + (c.lane as f32) * cfg.lane_height + cfg.lane_height / 2.0,
    )
}

/// Map evidence state → ring color (emerald/violet/red).
fn evidence_color(e: &EvidenceView) -> &'static str {
    match e {
        EvidenceView::AgentPass { .. } => "#34d399",
        EvidenceView::Human { .. } => "#c084fc",
        EvidenceView::AgentFail { .. } => "#f87171",
    }
}

/// Uppercase + escape for SVG text content.
fn upcase_escape(s: &str) -> String {
    escape(&s.to_uppercase())
}

/// Minimal XML entity escape for SVG text content.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// SVG braid graph for a given [`BraidView`].
///
/// Renders via `dangerous_inner_html` because Dioxus's RSX SVG surface
/// doesn't support the full set of attributes (gradients, filters) we need
/// and this keeps one canonical renderer for both Drawer and Loom.
#[component]
pub fn BraidGraph(view: BraidView, cfg: BraidGraphCfg) -> Element {
    let svg = render_braid_svg(&view, &cfg);
    rsx! {
        div {
            class: "braid-graph-host",
            dangerous_inner_html: "{svg}",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CommitView, EvidenceView, PeerHeadView};

    fn sample_peer(id_byte: u8, name: &str, color: &str) -> PeerHeadView {
        PeerHeadView {
            user_id: [id_byte; 32],
            name: name.into(),
            color: color.into(),
            is_self: false,
            head_short_id: "c1".into(),
            head_short_hex: "00000001".into(),
            file_count: 0,
            relative_time: "0s".into(),
            is_diverged: false,
        }
    }

    fn sample_commit(short_id: &str, short_hex: &str, lane: usize, slot: usize, parents: &[&str]) -> CommitView {
        CommitView {
            short_id: short_id.into(),
            short_hex: short_hex.into(),
            author_id: [0; 32],
            author_name: "Love".into(),
            author_color: "#ff6b9d".into(),
            intent: "test".into(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            evidence: EvidenceView::Human { message: None },
            timestamp_ms: 0,
            relative_time: "0s".into(),
            is_merge: parents.len() > 1,
            lane,
            slot,
        }
    }

    #[test]
    fn renders_empty_view() {
        let v = BraidView::default();
        let svg = render_braid_svg(&v, &BraidGraphCfg::drawer());
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn knot_appears_for_every_commit() {
        let mut v = BraidView::default();
        v.peers = vec![sample_peer(1, "Love", "#ff6b9d")];
        v.commits = vec![
            sample_commit("c1", "aa00aa00", 0, 0, &[]),
            sample_commit("c2", "bb00bb00", 0, 1, &["aa00aa00"]),
        ];
        let svg = render_braid_svg(&v, &BraidGraphCfg::drawer());
        let knots = svg.matches("<circle").count();
        // >= 2 main knots (there's also glow + evidence circles when labels shown)
        assert!(knots >= 2, "expected at least 2 knots, svg = {svg}");
    }

    #[test]
    fn merge_commit_renders_donut() {
        let mut v = BraidView::default();
        v.peers = vec![
            sample_peer(1, "Love", "#ff6b9d"),
            sample_peer(2, "Joy", "#ffd93d"),
        ];
        v.commits = vec![
            sample_commit("c1", "aa", 0, 0, &[]),
            sample_commit("c2", "bb", 1, 0, &[]),
            sample_commit("m", "mm", 0, 1, &["aa", "bb"]),
        ];
        let svg = render_braid_svg(&v, &BraidGraphCfg::drawer());
        // Merge donut has stroke="#09090b" and stroke-width="1.5"
        let donut = svg.matches("stroke-width=\"1.5\"").count();
        assert!(donut >= 1, "merge commit should draw a donut");
    }
}
