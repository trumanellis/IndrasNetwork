// UI Components for Collaboration Viewer

use dioxus::prelude::*;

use crate::state::{CollaborationState, Peer, PeerStats, PlanSection, Quest, QuestStatus};

/// Header component with title and phase indicator
#[component]
pub fn Header(state: CollaborationState) -> Element {
    rsx! {
        header { class: "header",
            div { class: "header-title",
                h1 { "Collaboration Trio" }
                span { class: "realm-badge", "Realm: harmony" }
            }
            div { class: "phase-indicator",
                span { class: "phase-number", "{state.phase.number()}" }
                span { "{state.phase.name()}" }
            }
        }
    }
}

/// Left panel showing peer status
#[component]
pub fn PeerPanel(state: CollaborationState, on_select_pov: EventHandler<Peer>) -> Element {
    rsx! {
        aside { class: "peer-panel",
            div { class: "sidebar-section",
                div { class: "panel-title", "Peers" }
                for peer in Peer::all() {
                    PeerCard { peer: *peer, state: state.clone(), on_click: on_select_pov }
                }
            }
        }
    }
}

/// Individual peer card
#[component]
fn PeerCard(peer: Peer, state: CollaborationState, on_click: EventHandler<Peer>) -> Element {
    let peer_state = state.peer_states.get(&peer);
    let online = peer_state.map(|p| p.online).unwrap_or(false);
    let quests_created = peer_state.map(|p| p.quests_created).unwrap_or(0);
    let sections = peer_state.map(|p| p.sections_written).unwrap_or(0);
    let messages = peer_state.map(|p| p.messages_sent).unwrap_or(0);

    let active_class = if online { "active" } else { "" };

    rsx! {
        div {
            class: "peer-card clickable {active_class}",
            onclick: move |_| on_click.call(peer),
            div { class: "peer-header",
                div { class: "peer-avatar {peer.css_class()}",
                    "{peer.initial()}"
                }
                div { class: "peer-info",
                    h3 { "{peer.display_name()}" }
                    div { class: "peer-status",
                        span { class: "status-dot" }
                        if online { "Online" } else { "Offline" }
                    }
                }
            }
            div { class: "peer-stats",
                div { class: "stat-item",
                    span { class: "stat-label", "Quests" }
                    span { class: "stat-value", "{quests_created}" }
                }
                div { class: "stat-item",
                    span { class: "stat-label", "Sections" }
                    span { class: "stat-value", "{sections}" }
                }
                div { class: "stat-item",
                    span { class: "stat-label", "Messages" }
                    span { class: "stat-value", "{messages}" }
                }
            }
        }
    }
}

/// Center panel with network visualization and quest board
#[component]
pub fn VisualizationPanel(state: CollaborationState) -> Element {
    rsx! {
        section { class: "visualization-panel",
            div { class: "network-container",
                div { class: "network-header",
                    span { class: "network-title", "Network Topology" }
                }
                NetworkView { state: state.clone() }
            }
            QuestBoard { state: state }
        }
    }
}

/// Network topology visualization (SVG triangle)
#[component]
fn NetworkView(state: CollaborationState) -> Element {
    let width = 300.0;
    let height = 200.0;

    // Convert normalized positions to SVG coordinates
    let pos = |peer: &Peer| -> (f64, f64) {
        let (nx, ny) = peer.position();
        (nx * width, ny * height)
    };

    // Edge lines
    let edges = [
        (Peer::A, Peer::B),
        (Peer::B, Peer::C),
        (Peer::C, Peer::A),
    ];

    rsx! {
        div { class: "network-view",
            svg {
                class: "network-svg",
                view_box: "0 0 {width} {height}",
                preserve_aspect_ratio: "xMidYMid meet",

                // Draw edges
                for (from, to) in edges.iter() {
                    {
                        let (x1, y1) = pos(from);
                        let (x2, y2) = pos(to);
                        let is_active = state.active_edges.contains(&(*from, *to))
                            || state.active_edges.contains(&(*to, *from));
                        let class = if is_active { "network-edge active" } else { "network-edge" };

                        rsx! {
                            line {
                                class: "{class}",
                                x1: "{x1}",
                                y1: "{y1}",
                                x2: "{x2}",
                                y2: "{y2}",
                            }
                        }
                    }
                }

                // Draw peer nodes
                for peer in Peer::all() {
                    {
                        let (cx, cy) = pos(peer);
                        let online = state.peer_states.get(peer).map(|p| p.online).unwrap_or(false);
                        let fill = match peer {
                            Peer::A => "var(--peer-a)",
                            Peer::B => "var(--peer-b)",
                            Peer::C => "var(--peer-c)",
                        };
                        let opacity = if online { "1" } else { "0.3" };

                        rsx! {
                            g { class: "peer-node",
                                circle {
                                    class: "peer-node-circle",
                                    cx: "{cx}",
                                    cy: "{cy}",
                                    r: "25",
                                    fill: "{fill}",
                                    opacity: "{opacity}",
                                }
                                text {
                                    class: "peer-node-label",
                                    x: "{cx}",
                                    y: "{cy}",
                                    "{peer.display_name()}"
                                }
                            }
                        }
                    }
                }

                // Draw packet animations
                for packet in state.active_packets.iter() {
                    {
                        let (px, py) = packet.position();
                        let x = px * width;
                        let y = py * height;

                        rsx! {
                            circle {
                                class: "packet-dot",
                                cx: "{x}",
                                cy: "{y}",
                                r: "6",
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Quest board with kanban columns
#[component]
fn QuestBoard(state: CollaborationState) -> Element {
    rsx! {
        div { class: "quest-board",
            div { class: "quest-board-header",
                span { class: "quest-board-title", "Quest Board" }
            }
            div { class: "quest-columns",
                QuestColumn {
                    title: "Pending",
                    quests: state.quests_by_status(QuestStatus::Pending)
                        .into_iter().cloned().collect(),
                    highlighted: state.highlighted_quest,
                }
                QuestColumn {
                    title: "In Progress",
                    quests: state.quests_by_status(QuestStatus::InProgress)
                        .into_iter().cloned().collect(),
                    highlighted: state.highlighted_quest,
                }
                QuestColumn {
                    title: "Completed",
                    quests: state.quests_by_status(QuestStatus::Completed)
                        .into_iter().cloned().collect(),
                    highlighted: state.highlighted_quest,
                }
            }
        }
    }
}

/// A single quest column
#[component]
fn QuestColumn(title: &'static str, quests: Vec<Quest>, highlighted: Option<u32>) -> Element {
    let count = quests.len();

    rsx! {
        div { class: "quest-column",
            div { class: "quest-column-header",
                span { class: "quest-column-title", "{title}" }
                span { class: "quest-count", "{count}" }
            }
            div { class: "quest-list",
                for quest in quests.iter() {
                    QuestCard { quest: quest.clone(), highlighted: highlighted == Some(quest.id) }
                }
            }
        }
    }
}

/// Individual quest card
#[component]
fn QuestCard(quest: Quest, highlighted: bool) -> Element {
    let highlight_class = if highlighted { "highlight" } else { "" };

    rsx! {
        div { class: "quest-card {highlight_class}",
            div { class: "quest-title", "{quest.title}" }
            div { class: "quest-meta",
                span { class: "quest-creator", "by {quest.creator.display_name()}" }
                div { class: "quest-assignee",
                    span { class: "assignee-dot {quest.assignee.css_class()}" }
                    span { "{quest.assignee.display_name()}" }
                }
            }
        }
    }
}

/// Right panel with project plan and timeline
#[component]
pub fn RightPanel(state: CollaborationState) -> Element {
    rsx! {
        aside { class: "right-panel",
            ProjectPlan {
                title: state.plan_title.clone(),
                sections: state.plan_sections.clone(),
            }
            EventTimeline { events: state.events.clone() }
        }
    }
}

/// Project plan document display
#[component]
fn ProjectPlan(title: String, sections: Vec<PlanSection>) -> Element {
    rsx! {
        div { class: "project-plan",
            div { class: "project-header",
                span { class: "project-title", "{title}" }
                span { class: "project-version", "v{sections.len()}" }
            }
            div { class: "project-sections",
                for section in sections.iter() {
                    div { class: "section-card {section.author.css_class()}",
                        div { class: "section-header",
                            span { class: "section-author", "{section.author.display_name()}" }
                        }
                        p { class: "section-content", "{section.content}" }
                    }
                }
                if sections.is_empty() {
                    div { class: "section-card",
                        p { class: "section-content", style: "color: var(--text-muted); font-style: italic;",
                            "No sections yet..."
                        }
                    }
                }
            }
        }
    }
}

/// Event timeline
#[component]
fn EventTimeline(events: Vec<crate::state::SimEvent>) -> Element {
    rsx! {
        div { class: "event-timeline",
            div { class: "timeline-header",
                div { class: "panel-title", "Activity" }
            }
            div { class: "timeline-list",
                for event in events.iter().rev().take(20) {
                    div { class: "timeline-event",
                        span { class: "event-tick", "T{event.tick}" }
                        span { class: "event-icon", "{event.event_type.icon()}" }
                        span { class: "event-message {event.event_type.css_class()}",
                            "{event.message}"
                        }
                    }
                }
                if events.is_empty() {
                    div { class: "timeline-event",
                        span { class: "event-message", style: "color: var(--text-muted);",
                            "Waiting to start..."
                        }
                    }
                }
            }
        }
    }
}

/// Floating simulation control bar - Apple-inspired design
#[component]
pub fn ControlBar(
    state: CollaborationState,
    on_step: EventHandler<()>,
    on_play_pause: EventHandler<()>,
    on_reset: EventHandler<()>,
    on_speed_change: EventHandler<f64>,
) -> Element {
    let is_complete = state.tick >= state.max_tick;
    let progress = (state.tick as f64 / state.max_tick as f64) * 100.0;

    // Phase indicator
    let phase_name = state.phase.name();

    rsx! {
        div { class: "floating-controls",
            // Progress track (background)
            div { class: "progress-track",
                div {
                    class: "progress-fill",
                    style: "width: {progress}%",
                }
            }

            // Main control pill
            div { class: "control-pill",
                // Left section: Reset
                button {
                    class: "control-icon-btn",
                    title: "Reset",
                    onclick: move |_| on_reset.call(()),
                    span { class: "control-icon", "\u{21BA}" } // ↺
                }

                // Divider
                div { class: "control-divider" }

                // Center section: Play/Pause (hero button)
                button {
                    class: "play-pause-btn",
                    class: if is_complete { "complete" } else { "" },
                    disabled: is_complete,
                    onclick: move |_| on_play_pause.call(()),
                    span { class: "play-pause-icon",
                        if is_complete {
                            "\u{2713}" // ✓
                        } else if state.paused {
                            "\u{25B6}" // ▶
                        } else {
                            "\u{23F8}" // ⏸
                        }
                    }
                }

                // Step button
                button {
                    class: "control-icon-btn",
                    class: if is_complete { "disabled" } else { "" },
                    title: "Step",
                    disabled: is_complete,
                    onclick: move |_| on_step.call(()),
                    span { class: "control-icon", "\u{23ED}" } // ⏭
                }

                // Divider
                div { class: "control-divider" }

                // Right section: Tick counter
                div { class: "tick-counter",
                    span { class: "tick-current", "{state.tick}" }
                    span { class: "tick-separator", "/" }
                    span { class: "tick-max", "{state.max_tick}" }
                }
            }

            // Speed control (separate floating element)
            div { class: "speed-pill",
                span { class: "speed-value", "{state.speed:.1}x" }
                input {
                    r#type: "range",
                    class: "speed-slider-apple",
                    min: "0.5",
                    max: "10",
                    step: "0.5",
                    value: "{state.speed}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            on_speed_change.call(v);
                        }
                    },
                }
            }

            // Phase indicator (floating badge)
            div { class: "phase-pill",
                span { class: "phase-dot" }
                span { class: "phase-name", "{phase_name}" }
            }
        }
    }
}

// ============================================
// POV DASHBOARD COMPONENTS
// ============================================

/// POV Dashboard - First-person view for a specific peer
#[component]
pub fn POVDashboard(
    peer: Peer,
    state: CollaborationState,
    on_back: EventHandler<()>,
    on_switch_pov: EventHandler<Peer>,
) -> Element {
    let stats = state.stats_for_peer(peer);

    rsx! {
        div { class: "pov-dashboard {peer.css_class()} view-transition view-active",
            POVHeader { peer: peer, on_back: on_back }
            main { class: "pov-content",
                div { class: "pov-left-column",
                    ProfileHero { peer: peer, state: state.clone(), stats: stats }
                }
                div { class: "pov-center-column",
                    MyNetworkView { peer: peer, state: state.clone(), on_switch_pov: on_switch_pov }
                    MyQuestsBoard { peer: peer, state: state.clone() }
                }
                div { class: "pov-right-column",
                    MyContributions { peer: peer, state: state.clone() }
                    MyActivity { peer: peer, state: state }
                }
            }
        }
    }
}

/// POV Header with back button and peer's dashboard title
#[component]
fn POVHeader(peer: Peer, on_back: EventHandler<()>) -> Element {
    rsx! {
        header { class: "pov-header",
            div {
                class: "back-button",
                onclick: move |_| on_back.call(()),
                span { class: "back-arrow", "\u{2190}" }
                span { "Back to Overview" }
            }
            h1 { class: "pov-title", "{peer.display_name()}'s Dashboard" }
            div { class: "pov-header-spacer" }
        }
    }
}

/// Profile hero with large avatar and stats
#[component]
fn ProfileHero(peer: Peer, state: CollaborationState, stats: PeerStats) -> Element {
    let peer_state = state.peer_states.get(&peer);
    let online = peer_state.map(|p| p.online).unwrap_or(false);

    rsx! {
        div { class: "profile-hero",
            div { class: "profile-avatar-large {peer.css_class()}",
                "{peer.initial()}"
            }
            h2 { class: "profile-name", "{peer.display_name()}" }
            div { class: "profile-status",
                span { class: "status-dot" }
                if online { "Online" } else { "Offline" }
            }
            div { class: "profile-stats",
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.quests_created}" }
                    span { class: "profile-stat-label", "Created" }
                }
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.quests_assigned}" }
                    span { class: "profile-stat-label", "Assigned" }
                }
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.quests_completed}" }
                    span { class: "profile-stat-label", "Complete" }
                }
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.sections_written}" }
                    span { class: "profile-stat-label", "Sections" }
                }
            }
        }
    }
}

/// Ego-centric network view with the selected peer at center
#[component]
fn MyNetworkView(peer: Peer, state: CollaborationState, on_switch_pov: EventHandler<Peer>) -> Element {
    let width = 280.0;
    let height = 180.0;
    let center_x = width / 2.0;
    let center_y = height / 2.0;

    // Get other peers positioned around the center
    let other_peers: Vec<Peer> = Peer::all().iter().filter(|p| **p != peer).copied().collect();

    // Calculate positions for other peers (spread around center)
    let angle_offset = std::f64::consts::PI / 2.0; // Start from top
    let angle_step = std::f64::consts::PI * 2.0 / (other_peers.len() as f64);
    let radius = 65.0;

    rsx! {
        div { class: "my-network-view",
            div { class: "network-header",
                span { class: "network-title", "My Network" }
            }
            svg {
                class: "network-svg ego-centric",
                view_box: "0 0 {width} {height}",
                preserve_aspect_ratio: "xMidYMid meet",

                // Draw edges from center to other peers
                for (i, other) in other_peers.iter().enumerate() {
                    {
                        let angle = angle_offset + (i as f64) * angle_step;
                        let other_x = center_x + radius * angle.cos();
                        let other_y = center_y + radius * angle.sin();
                        let is_active = state.active_edges.iter().any(|(from, to)| {
                            (*from == peer && *to == *other) || (*from == *other && *to == peer)
                        });
                        let class = if is_active { "network-edge active" } else { "network-edge" };

                        rsx! {
                            line {
                                class: "{class}",
                                x1: "{center_x}",
                                y1: "{center_y}",
                                x2: "{other_x}",
                                y2: "{other_y}",
                            }
                        }
                    }
                }

                // Draw center (ego) node - larger
                {
                    let online = state.peer_states.get(&peer).map(|p| p.online).unwrap_or(false);
                    let fill = match peer {
                        Peer::A => "var(--peer-a)",
                        Peer::B => "var(--peer-b)",
                        Peer::C => "var(--peer-c)",
                    };
                    let opacity = if online { "1" } else { "0.3" };

                    rsx! {
                        g { class: "ego-node",
                            circle {
                                class: "peer-node-circle ego",
                                cx: "{center_x}",
                                cy: "{center_y}",
                                r: "32",
                                fill: "{fill}",
                                opacity: "{opacity}",
                            }
                            text {
                                class: "peer-node-label ego",
                                x: "{center_x}",
                                y: "{center_y}",
                                "{peer.display_name()}"
                            }
                        }
                    }
                }

                // Draw other peer nodes (smaller, clickable)
                for (i, other) in other_peers.iter().enumerate() {
                    {
                        let angle = angle_offset + (i as f64) * angle_step;
                        let other_x = center_x + radius * angle.cos();
                        let other_y = center_y + radius * angle.sin();
                        let online = state.peer_states.get(other).map(|p| p.online).unwrap_or(false);
                        let fill = match other {
                            Peer::A => "var(--peer-a)",
                            Peer::B => "var(--peer-b)",
                            Peer::C => "var(--peer-c)",
                        };
                        let opacity = if online { "1" } else { "0.3" };
                        let other_peer = *other;

                        rsx! {
                            g {
                                class: "other-node clickable",
                                onclick: move |_| on_switch_pov.call(other_peer),
                                circle {
                                    class: "peer-node-circle",
                                    cx: "{other_x}",
                                    cy: "{other_y}",
                                    r: "22",
                                    fill: "{fill}",
                                    opacity: "{opacity}",
                                }
                                text {
                                    class: "peer-node-label",
                                    x: "{other_x}",
                                    y: "{other_y}",
                                    "{other.display_name()}"
                                }
                            }
                        }
                    }
                }

                // Draw packet animations (only those involving this peer)
                for packet in state.active_packets.iter().filter(|p| p.from == peer || p.to == peer) {
                    {
                        // Calculate packet position relative to ego-centric view
                        let (from_x, from_y) = if packet.from == peer {
                            (center_x, center_y)
                        } else {
                            let idx = other_peers.iter().position(|p| *p == packet.from).unwrap_or(0);
                            let angle = angle_offset + (idx as f64) * angle_step;
                            (center_x + radius * angle.cos(), center_y + radius * angle.sin())
                        };

                        let (to_x, to_y) = if packet.to == peer {
                            (center_x, center_y)
                        } else {
                            let idx = other_peers.iter().position(|p| *p == packet.to).unwrap_or(0);
                            let angle = angle_offset + (idx as f64) * angle_step;
                            (center_x + radius * angle.cos(), center_y + radius * angle.sin())
                        };

                        let t = packet.progress;
                        let px = from_x + (to_x - from_x) * t;
                        let py = from_y + (to_y - from_y) * t;

                        rsx! {
                            circle {
                                class: "packet-dot",
                                cx: "{px}",
                                cy: "{py}",
                                r: "5",
                            }
                        }
                    }
                }
            }
        }
    }
}

/// My quests board - filtered to quests where peer is creator or assignee
#[component]
fn MyQuestsBoard(peer: Peer, state: CollaborationState) -> Element {
    let pending = state.quests_for_peer_by_status(peer, QuestStatus::Pending);
    let in_progress = state.quests_for_peer_by_status(peer, QuestStatus::InProgress);
    let completed = state.quests_for_peer_by_status(peer, QuestStatus::Completed);

    rsx! {
        div { class: "my-quests-board",
            div { class: "quest-board-header",
                span { class: "quest-board-title", "My Quests" }
            }
            div { class: "quest-columns",
                MyQuestColumn {
                    title: "Pending",
                    quests: pending.into_iter().cloned().collect(),
                    peer: peer,
                    highlighted: state.highlighted_quest,
                }
                MyQuestColumn {
                    title: "In Progress",
                    quests: in_progress.into_iter().cloned().collect(),
                    peer: peer,
                    highlighted: state.highlighted_quest,
                }
                MyQuestColumn {
                    title: "Completed",
                    quests: completed.into_iter().cloned().collect(),
                    peer: peer,
                    highlighted: state.highlighted_quest,
                }
            }
        }
    }
}

/// Quest column for POV view with role badges
#[component]
fn MyQuestColumn(title: &'static str, quests: Vec<Quest>, peer: Peer, highlighted: Option<u32>) -> Element {
    let count = quests.len();

    rsx! {
        div { class: "quest-column",
            div { class: "quest-column-header",
                span { class: "quest-column-title", "{title}" }
                span { class: "quest-count", "{count}" }
            }
            div { class: "quest-list",
                for quest in quests.iter() {
                    MyQuestCard {
                        quest: quest.clone(),
                        peer: peer,
                        highlighted: highlighted == Some(quest.id),
                    }
                }
            }
        }
    }
}

/// Quest card with role badge for POV view
#[component]
fn MyQuestCard(quest: Quest, peer: Peer, highlighted: bool) -> Element {
    let highlight_class = if highlighted { "highlight" } else { "" };

    // Determine role(s)
    let is_creator = quest.creator == peer;
    let is_assignee = quest.assignee == peer;

    rsx! {
        div { class: "quest-card {highlight_class}",
            div { class: "quest-title", "{quest.title}" }
            div { class: "quest-meta",
                div { class: "quest-roles",
                    if is_creator {
                        RoleBadge { role: "creator" }
                    }
                    if is_assignee {
                        RoleBadge { role: "assignee" }
                    }
                }
                if !is_creator {
                    span { class: "quest-creator", "by {quest.creator.display_name()}" }
                }
            }
        }
    }
}

/// Role badge component
#[component]
fn RoleBadge(role: &'static str) -> Element {
    rsx! {
        span { class: "role-badge {role}", "{role}" }
    }
}

/// My contributions - sections authored by this peer
#[component]
fn MyContributions(peer: Peer, state: CollaborationState) -> Element {
    let sections = state.sections_for_peer(peer);

    rsx! {
        div { class: "my-contributions",
            div { class: "panel-header",
                div { class: "panel-title", "My Contributions" }
            }
            div { class: "contributions-list",
                for section in sections.iter() {
                    div { class: "section-card {peer.css_class()}",
                        div { class: "section-header",
                            span { class: "section-author", "Section {section.id}" }
                        }
                        p { class: "section-content", "{section.content}" }
                    }
                }
                if sections.is_empty() {
                    div { class: "empty-state",
                        "No contributions yet..."
                    }
                }
            }
        }
    }
}

/// My activity - events involving this peer
#[component]
fn MyActivity(peer: Peer, state: CollaborationState) -> Element {
    let events = state.events_for_peer(peer);

    rsx! {
        div { class: "my-activity",
            div { class: "panel-header",
                div { class: "panel-title", "My Activity" }
            }
            div { class: "activity-list",
                for event in events.iter().rev().take(15) {
                    div { class: "timeline-event",
                        span { class: "event-tick", "T{event.tick}" }
                        span { class: "event-icon", "{event.event_type.icon()}" }
                        span { class: "event-message {event.event_type.css_class()}",
                            "{event.message}"
                        }
                    }
                }
                if events.is_empty() {
                    div { class: "empty-state",
                        "No activity yet..."
                    }
                }
            }
        }
    }
}
