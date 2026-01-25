// UI Components for Collaboration Viewer

use dioxus::prelude::*;

use crate::state::{CollaborationState, Peer, PlanSection, Quest, QuestStatus};

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
pub fn PeerPanel(state: CollaborationState) -> Element {
    rsx! {
        aside { class: "peer-panel",
            div { class: "sidebar-section",
                div { class: "panel-title", "Peers" }
                for peer in Peer::all() {
                    PeerCard { peer: *peer, state: state.clone() }
                }
            }
        }
    }
}

/// Individual peer card
#[component]
fn PeerCard(peer: Peer, state: CollaborationState) -> Element {
    let peer_state = state.peer_states.get(&peer);
    let online = peer_state.map(|p| p.online).unwrap_or(false);
    let quests_created = peer_state.map(|p| p.quests_created).unwrap_or(0);
    let sections = peer_state.map(|p| p.sections_written).unwrap_or(0);
    let messages = peer_state.map(|p| p.messages_sent).unwrap_or(0);

    let active_class = if online { "active" } else { "" };

    rsx! {
        div { class: "peer-card {active_class}",
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
        (Peer::Love, Peer::Joy),
        (Peer::Joy, Peer::Peace),
        (Peer::Peace, Peer::Love),
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
                            Peer::Love => "var(--peer-love)",
                            Peer::Joy => "var(--peer-joy)",
                            Peer::Peace => "var(--peer-peace)",
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

/// Bottom control bar
#[component]
pub fn ControlBar(
    state: CollaborationState,
    on_step: EventHandler<()>,
    on_play_pause: EventHandler<()>,
    on_reset: EventHandler<()>,
    on_speed_change: EventHandler<f64>,
) -> Element {
    let is_complete = state.tick >= state.max_tick;

    rsx! {
        div { class: "control-bar",
            div { class: "control-group",
                button {
                    class: "control-button",
                    onclick: move |_| on_reset.call(()),
                    "Reset"
                }
                button {
                    class: "control-button primary",
                    disabled: is_complete,
                    onclick: move |_| on_play_pause.call(()),
                    if state.paused { "Play" } else { "Pause" }
                }
                button {
                    class: "control-button",
                    disabled: is_complete,
                    onclick: move |_| on_step.call(()),
                    "Step"
                }
            }

            div { class: "tick-display",
                "Tick "
                span { class: "current", "{state.tick}" }
                " / {state.max_tick}"
            }

            div { class: "speed-control",
                span { class: "speed-label", "Speed" }
                input {
                    r#type: "range",
                    class: "speed-slider",
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
                span { class: "speed-label", "{state.speed:.1}x" }
            }
        }
    }
}
