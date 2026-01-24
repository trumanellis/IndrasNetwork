use crate::runner::ScenarioRunner;
use crate::state::{
    format_network_event, EventType, InstanceState, PacketAnimation, SimEvent, SimMetrics, Tab,
};
use dioxus::prelude::*;
use indras_simulation::{NetworkEvent, PacketId, PeerId};

pub mod charts;
pub mod documents;
pub mod panels;
pub mod sdk;

pub use documents::DocumentsView;
pub use sdk::SDKView;

/// Header component with dashboard title
#[component]
pub fn Header() -> Element {
    rsx! {
        div {
            class: "header",
            h1 { "IndrasNetwork Stress Test Dashboard" }
        }
    }
}

/// Sidebar component for scenario selection
#[component]
pub fn Sidebar(selected: Option<String>, on_select: EventHandler<String>) -> Element {
    // Use the categorized scenarios from ScenarioRunner
    let categories = ScenarioRunner::get_categorized_scenarios();

    rsx! {
        div {
            class: "sidebar",
            h2 { "Scenarios" }
            for (group, items) in categories {
                div {
                    class: "scenario-group",
                    h3 { "{group}" }
                    ul {
                        for info in items {
                            li {
                                class: if selected.as_deref() == Some(info.name) {
                                    "scenario-item selected"
                                } else {
                                    "scenario-item"
                                },
                                onclick: move |_| on_select.call(info.name.to_string()),
                                "{info.name}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Description panel showing what the selected scenario does
#[component]
pub fn ScenarioDescription(selected: Option<String>) -> Element {
    let description = selected
        .as_ref()
        .and_then(|name| ScenarioRunner::get_scenario_description(name));

    rsx! {
        div {
            class: "scenario-description",
            style: "background: var(--bg-card); border-radius: 8px; padding: 16px; margin-bottom: 16px;",
            h3 {
                style: "font-size: 0.875rem; color: var(--text-muted); margin-bottom: 8px; text-transform: uppercase; letter-spacing: 0.05em;",
                "Selected Scenario"
            }
            if let Some(name) = &selected {
                div {
                    style: "margin-bottom: 8px;",
                    span {
                        style: "color: var(--accent-primary); font-weight: 600; font-size: 1rem;",
                        "{name}"
                    }
                }
                if let Some(desc) = description {
                    p {
                        style: "color: var(--text-secondary); font-size: 0.875rem; line-height: 1.5; margin: 0;",
                        "{desc}"
                    }
                }
            } else {
                p {
                    style: "color: var(--text-muted); font-style: italic; margin: 0;",
                    "Select a scenario from the sidebar to see its description and run it."
                }
            }
        }
    }
}

/// Controls component for stress level selection and run/stop actions
#[component]
pub fn Controls(
    selected: Option<String>,
    level: String,
    running: bool,
    on_run: EventHandler<()>,
    on_stop: EventHandler<()>,
    on_level_change: EventHandler<String>,
) -> Element {
    let has_selection = selected.is_some();

    rsx! {
        div {
            class: "controls",
            div {
                class: "stress-level-selector",
                h3 { "Stress Level" }
                div {
                    class: "radio-group",
                    label {
                        input {
                            r#type: "radio",
                            name: "stress-level",
                            value: "quick",
                            checked: level == "quick",
                            disabled: running,
                            onchange: move |_| on_level_change.call("quick".to_string()),
                        }
                        "Quick"
                    }
                    label {
                        input {
                            r#type: "radio",
                            name: "stress-level",
                            value: "medium",
                            checked: level == "medium",
                            disabled: running,
                            onchange: move |_| on_level_change.call("medium".to_string()),
                        }
                        "Medium"
                    }
                    label {
                        input {
                            r#type: "radio",
                            name: "stress-level",
                            value: "full",
                            checked: level == "full",
                            disabled: running,
                            onchange: move |_| on_level_change.call("full".to_string()),
                        }
                        "Full"
                    }
                }
                div {
                    style: "font-size: 0.75rem; color: var(--text-muted); margin-top: 4px;",
                    match level.as_str() {
                        "quick" => "Fast smoke test (~10 peers, 100 ops)",
                        "full" => "Maximum stress (~26 peers, 10k ops)",
                        _ => "Balanced test (~20 peers, 1k ops)",
                    }
                }
            }

            div {
                class: "action-buttons",
                if !running {
                    button {
                        class: "run-button",
                        disabled: !has_selection,
                        onclick: move |_| on_run.call(()),
                        "Run"
                    }
                } else {
                    button {
                        class: "stop-button",
                        onclick: move |_| on_stop.call(()),
                        "Stop"
                    }
                }
            }

            if running {
                div {
                    class: "progress-indicator",
                    div { class: "spinner" }
                    span { "Running..." }
                }
            }
        }
    }
}

/// MetricsPanel component for displaying simulation metrics
#[component]
pub fn MetricsPanel(metrics: SimMetrics) -> Element {
    // Determine if this is a PQ crypto test based on which metrics are present
    let is_pq_test = metrics.pq_signatures_created > 0 || metrics.kem_encapsulations > 0;
    let is_routing_test = metrics.messages_sent > 0;

    // Calculate derived metrics
    let delivery_rate = if metrics.messages_sent > 0 {
        (metrics.messages_delivered as f64 / metrics.messages_sent as f64) * 100.0
    } else {
        0.0
    };

    let signature_failure_rate = if metrics.pq_signatures_created > 0 {
        (metrics.pq_signature_failures as f64 / metrics.pq_signatures_created as f64) * 100.0
    } else {
        0.0
    };

    let kem_failure_rate = if metrics.kem_encapsulations > 0 {
        (metrics.kem_failures as f64 / metrics.kem_encapsulations as f64) * 100.0
    } else {
        0.0
    };

    // Progress percentage
    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    rsx! {
        div {
            class: "metrics-panel",
            h2 { "Metrics" }

            // Progress bar if running
            if metrics.max_ticks > 0 {
                div {
                    style: "margin-bottom: 16px;",
                    div {
                        style: "display: flex; justify-content: space-between; font-size: 0.75rem; color: var(--text-muted); margin-bottom: 4px;",
                        span { "Tick {metrics.current_tick} / {metrics.max_ticks}" }
                        span { "{progress:.1}%" }
                    }
                    div {
                        style: "height: 8px; background: var(--bg-tertiary); border-radius: 4px; overflow: hidden;",
                        div {
                            style: "height: 100%; background: var(--accent-primary); width: {progress}%; transition: width 0.3s ease;",
                        }
                    }
                }
            }

            // Throughput if available
            if metrics.ops_per_second > 0.0 {
                div {
                    style: "margin-bottom: 16px; padding: 12px; background: var(--bg-tertiary); border-radius: 8px; text-align: center;",
                    span { style: "color: var(--text-muted); font-size: 0.75rem;", "Throughput: " }
                    span { style: "color: var(--accent-primary); font-size: 1.25rem; font-weight: 600;", "{metrics.ops_per_second:.0} ops/sec" }
                }
            }

            div {
                class: "metrics-grid",

                // PQ Signature metrics (show if PQ test)
                if is_pq_test {
                    div {
                        class: "metric-card",
                        h3 { "Signatures Created" }
                        div {
                            class: "metric-value large",
                            "{metrics.pq_signatures_created}"
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Signatures Verified" }
                        div {
                            class: "metric-value large",
                            "{metrics.pq_signatures_verified}"
                        }
                    }

                    if metrics.avg_sign_latency_us > 0.0 {
                        div {
                            class: "metric-card",
                            h3 { "Avg Sign Latency" }
                            div {
                                class: "metric-value",
                                "{metrics.avg_sign_latency_us:.1} µs"
                            }
                        }
                    }

                    if metrics.avg_verify_latency_us > 0.0 {
                        div {
                            class: "metric-card",
                            h3 { "Avg Verify Latency" }
                            div {
                                class: "metric-value",
                                "{metrics.avg_verify_latency_us:.1} µs"
                            }
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Signature Failures" }
                        div {
                            class: "metric-value",
                            style: if metrics.pq_signature_failures > 0 { "color: var(--accent-error);" } else { "" },
                            "{metrics.pq_signature_failures} ({signature_failure_rate:.2}%)"
                        }
                    }
                }

                // KEM metrics (show if KEM operations present)
                if metrics.kem_encapsulations > 0 {
                    div {
                        class: "metric-card",
                        h3 { "KEM Encapsulations" }
                        div {
                            class: "metric-value large",
                            "{metrics.kem_encapsulations}"
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "KEM Decapsulations" }
                        div {
                            class: "metric-value large",
                            "{metrics.kem_decapsulations}"
                        }
                    }

                    if metrics.avg_encap_latency_us > 0.0 {
                        div {
                            class: "metric-card",
                            h3 { "Avg Encap Latency" }
                            div {
                                class: "metric-value",
                                "{metrics.avg_encap_latency_us:.1} µs"
                            }
                        }
                    }

                    if metrics.avg_decap_latency_us > 0.0 {
                        div {
                            class: "metric-card",
                            h3 { "Avg Decap Latency" }
                            div {
                                class: "metric-value",
                                "{metrics.avg_decap_latency_us:.1} µs"
                            }
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "KEM Failures" }
                        div {
                            class: "metric-value",
                            style: if metrics.kem_failures > 0 { "color: var(--accent-error);" } else { "" },
                            "{metrics.kem_failures} ({kem_failure_rate:.2}%)"
                        }
                    }
                }

                // Routing/messaging metrics (show if routing test)
                if is_routing_test {
                    div {
                        class: "metric-card",
                        h3 { "Delivery Rate" }
                        div {
                            class: "metric-value large",
                            "{delivery_rate:.1}%"
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Avg Latency" }
                        div {
                            class: "metric-value large",
                            if metrics.avg_latency > 0.0 {
                                "{metrics.avg_latency:.1} ticks"
                            } else {
                                "{metrics.avg_latency_ticks} ticks"
                            }
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Messages Sent" }
                        div {
                            class: "metric-value",
                            "{metrics.messages_sent}"
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Messages Delivered" }
                        div {
                            class: "metric-value",
                            "{metrics.messages_delivered}"
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Messages Dropped" }
                        div {
                            class: "metric-value",
                            style: if metrics.messages_dropped > 0 { "color: var(--accent-warning);" } else { "" },
                            "{metrics.messages_dropped}"
                        }
                    }

                    if metrics.backprops_completed > 0 {
                        div {
                            class: "metric-card",
                            h3 { "Backprops Completed" }
                            div {
                                class: "metric-value",
                                "{metrics.backprops_completed}"
                            }
                        }
                    }

                    if metrics.avg_hops > 0.0 {
                        div {
                            class: "metric-card",
                            h3 { "Avg Hops" }
                            div {
                                class: "metric-value",
                                "{metrics.avg_hops:.2}"
                            }
                        }
                    }
                }

                // Show placeholder if no metrics yet
                if !is_pq_test && !is_routing_test {
                    div {
                        class: "metric-card",
                        style: "grid-column: span 2;",
                        h3 { "Waiting for data..." }
                        div {
                            class: "metric-value",
                            style: "color: var(--text-muted);",
                            "Run a scenario to see metrics"
                        }
                    }
                }
            }
        }
    }
}

/// EventLog component for displaying recent simulation events
#[component]
pub fn EventLog(events: Vec<SimEvent>) -> Element {
    rsx! {
        div {
            class: "event-log",
            h2 { "Event Log" }
            div {
                class: "event-list",
                if events.is_empty() {
                    div {
                        class: "no-events",
                        "No events yet. Run a scenario to see activity."
                    }
                } else {
                    for event in events.iter().rev().take(100) {
                        div {
                            class: "event-item {get_event_class(&event.event_type)}",
                            span {
                                class: "event-tick",
                                "[{event.tick}]"
                            }
                            span {
                                class: "event-type",
                                "{format_event_type(&event.event_type)}"
                            }
                            span {
                                class: "event-description",
                                "{event.description}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// Helper functions

fn get_event_class(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::Info => "event-info",
        EventType::Warning => "event-warning",
        EventType::Error => "event-error",
        EventType::Success => "event-success",
    }
}

fn format_event_type(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::Info => "INFO",
        EventType::Warning => "WARN",
        EventType::Error => "ERROR",
        EventType::Success => "OK",
    }
}

// ============================================================================
// Tab Navigation Components
// ============================================================================

/// Tab bar for switching between Metrics, Simulations, Documents, and SDK views
#[component]
pub fn TabBar(current_tab: Tab, on_select: EventHandler<Tab>) -> Element {
    rsx! {
        div { class: "tab-bar",
            button {
                class: if current_tab == Tab::Metrics { "tab-btn active" } else { "tab-btn" },
                onclick: move |_| on_select.call(Tab::Metrics),
                "Metrics"
            }
            button {
                class: if current_tab == Tab::Simulations { "tab-btn active" } else { "tab-btn" },
                onclick: move |_| on_select.call(Tab::Simulations),
                "Simulations"
            }
            button {
                class: if current_tab == Tab::Documents { "tab-btn active" } else { "tab-btn" },
                onclick: move |_| on_select.call(Tab::Documents),
                "Documents"
            }
            button {
                class: if current_tab == Tab::SDK { "tab-btn active" } else { "tab-btn" },
                onclick: move |_| on_select.call(Tab::SDK),
                "SDK"
            }
        }
    }
}

// ============================================================================
// Simulations View Components
// ============================================================================

/// Scenario definition for the simulations sidebar
struct SimScenario {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    node_count: usize,
    topology_icon: &'static str,
}

const SCENARIOS: &[SimScenario] = &[
    SimScenario {
        id: "triangle",
        name: "Triangle",
        description: "Simple 3-node fully connected network",
        node_count: 3,
        topology_icon: "△",
    },
    SimScenario {
        id: "line",
        name: "Linear Chain",
        description: "Sequential hop-by-hop routing",
        node_count: 5,
        topology_icon: "—",
    },
    SimScenario {
        id: "star",
        name: "Star Hub",
        description: "Central hub with leaf nodes",
        node_count: 6,
        topology_icon: "✱",
    },
    SimScenario {
        id: "ring",
        name: "Ring Network",
        description: "Circular topology with dual paths",
        node_count: 8,
        topology_icon: "◯",
    },
    SimScenario {
        id: "mesh",
        name: "Full Mesh",
        description: "Every node connected to all others",
        node_count: 5,
        topology_icon: "⬡",
    },
];

/// Main container for network simulation visualization
#[component]
pub fn SimulationsView(
    state: Signal<InstanceState>,
    on_load_scenario: EventHandler<String>,
) -> Element {
    let selected_scenario = state.read().scenario_name.clone();

    rsx! {
        div { class: "simulations-view",
            // Left sidebar with scenario selection
            aside { class: "simulations-sidebar",
                div { class: "sidebar-header",
                    h2 { "Network Topologies" }
                    p { class: "sidebar-subtitle", "Select a topology to simulate" }
                }

                nav { class: "scenario-list",
                    for scenario in SCENARIOS.iter() {
                        {
                            let is_selected = selected_scenario.as_deref() == Some(scenario.id);
                            let scenario_id = scenario.id.to_string();
                            rsx! {
                                button {
                                    class: if is_selected { "scenario-card selected" } else { "scenario-card" },
                                    onclick: move |_| on_load_scenario.call(scenario_id.clone()),

                                    div { class: "scenario-icon", "{scenario.topology_icon}" }
                                    div { class: "scenario-info",
                                        div { class: "scenario-name", "{scenario.name}" }
                                        div { class: "scenario-meta",
                                            span { class: "node-count", "{scenario.node_count} nodes" }
                                        }
                                        div { class: "scenario-desc", "{scenario.description}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Main content area
            main { class: "simulations-main",
                // Playback controls at top - always show, disabled when no simulation
                SimulationControls { state: state }

                // Topology visualization
                TopologyView { state: state }

                // Bottom panels in a grid
                div { class: "simulation-panels",
                    PeerPanel { state: state }
                    SimulationEventTimeline { state: state }
                }
            }
        }
    }
}

/// Playback controls for the simulation
#[component]
pub fn SimulationControls(state: Signal<InstanceState>) -> Element {
    let has_simulation = state.read().simulation.is_some();
    let tick = state.read().current_tick();
    let max_ticks = state.read().max_ticks();
    let paused = state.read().paused;
    let playback_speed = state.read().playback_speed;

    // Compute disabled opacity for inline styles
    let step_opacity = if !has_simulation || !paused { "0.4" } else { "1" };
    let btn_opacity = if !has_simulation { "0.4" } else { "1" };

    rsx! {
        div {
            style: "background: #1e1e1e; border-radius: 8px; padding: 14px 20px; margin-bottom: 8px;",

            // Control buttons row
            div {
                style: "display: flex; align-items: center; justify-content: space-between; gap: 20px;",

                // Left: Control buttons
                div {
                    style: "display: flex; gap: 10px;",

                    // Step button
                    button {
                        style: "padding: 10px 18px; border: none; background: #333; color: #F7F3E9; border-radius: 6px; cursor: pointer; font-size: 0.9rem; font-weight: 500; opacity: {step_opacity};",
                        disabled: !has_simulation || !paused,
                        onclick: move |_| {
                            let new_events = {
                                let mut state_write = state.write();
                                if let Some(ref mut sim) = state_write.simulation {
                                    sim.step();
                                    sim.event_log.clone()
                                } else {
                                    return;
                                }
                            };
                            let mut state_write = state.write();
                            let current_count = state_write.recent_events.len();
                            let current_tick = state_write.current_tick();

                            for event in new_events.into_iter().skip(current_count) {
                                match &event {
                                    NetworkEvent::Send { from, to, .. } => {
                                        let packet_id = PacketId { source: *from, sequence: current_tick };
                                        state_write.packets_in_flight.push(PacketAnimation::new(
                                            packet_id, *from, *to, current_tick
                                        ));
                                    }
                                    NetworkEvent::Relay { via, to, packet_id, .. } => {
                                        state_write.packets_in_flight.push(PacketAnimation::new(
                                            *packet_id, *via, *to, current_tick
                                        ));
                                    }
                                    NetworkEvent::Delivered { packet_id, .. } => {
                                        state_write.packets_in_flight.retain(|p| p.packet_id != *packet_id);
                                    }
                                    _ => {}
                                }
                                state_write.add_event(event);
                            }
                            state_write.packets_in_flight.iter_mut().for_each(|p| p.update(current_tick));
                            state_write.packets_in_flight.retain(|p| !p.is_complete());
                        },
                        "⏭ Step"
                    }

                    // Play/Pause button
                    button {
                        style: if paused {
                            format!("padding: 10px 18px; border: none; background: #22c55e; color: white; border-radius: 6px; cursor: pointer; font-size: 0.9rem; font-weight: 600; opacity: {btn_opacity};")
                        } else {
                            format!("padding: 10px 18px; border: none; background: #f59e0b; color: #0a0a0a; border-radius: 6px; cursor: pointer; font-size: 0.9rem; font-weight: 600; opacity: {btn_opacity};")
                        },
                        disabled: !has_simulation,
                        onclick: move |_| {
                            let current = state.read().paused;
                            state.write().paused = !current;
                        },
                        if paused { "▶ Play" } else { "⏸ Pause" }
                    }

                    // Reset button
                    button {
                        style: format!("padding: 10px 18px; border: 1px solid #555; background: transparent; color: #aaa; border-radius: 6px; cursor: pointer; font-size: 0.9rem; font-weight: 500; opacity: {btn_opacity};"),
                        disabled: !has_simulation,
                        onclick: move |_| {
                            state.write().simulation = None;
                            state.write().clear_events();
                            state.write().packets_in_flight.clear();
                            state.write().peer_positions.clear();
                            state.write().scenario_name = None;
                        },
                        "↻ Reset"
                    }
                }

                // Right: Status info
                div {
                    style: "display: flex; align-items: center; gap: 20px;",

                    // Tick display
                    span {
                        style: "color: #00d4aa; font-size: 0.9rem; font-weight: 500; font-variant-numeric: tabular-nums;",
                        if has_simulation {
                            "Tick {tick} / {max_ticks}"
                        } else {
                            "Select a topology"
                        }
                    }

                    // Speed control
                    div {
                        style: "display: flex; align-items: center; gap: 10px;",
                        span {
                            style: "color: #888; font-size: 0.85rem; min-width: 40px;",
                            "{playback_speed:.1}×"
                        }
                        input {
                            r#type: "range",
                            min: "0.5",
                            max: "10",
                            step: "0.5",
                            value: "{playback_speed}",
                            disabled: !has_simulation,
                            style: "width: 100px; accent-color: #00d4aa;",
                            onchange: move |e| {
                                if let Ok(speed) = e.value().parse::<f64>() {
                                    state.write().playback_speed = speed;
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// Event timeline for simulation view
#[component]
pub fn SimulationEventTimeline(state: Signal<InstanceState>) -> Element {
    let state_read = state.read();
    let events = &state_read.recent_events;

    rsx! {
        div { class: "simulation-event-log",
            h3 { class: "panel-title", "Event Log" }
            div { class: "timeline-scroll",
                if events.is_empty() {
                    div { class: "empty-state",
                        span { class: "empty-icon", "📋" }
                        p { "Events will appear here during simulation" }
                    }
                } else {
                    for event in events.iter().rev().take(50) {
                        {
                            let (event_class, tick, text) = format_network_event(event);
                            rsx! {
                                div { class: "timeline-event {event_class}",
                                    span { class: "event-tick", "[{tick}]" }
                                    span { class: "event-text", "{text}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Keep old names as aliases for backward compatibility during transition
#[component]
pub fn InstanceView(
    state: Signal<InstanceState>,
    on_load_scenario: EventHandler<String>,
) -> Element {
    rsx! {
        SimulationsView { state: state, on_load_scenario: on_load_scenario }
    }
}

/// SVG-based topology visualization
#[component]
pub fn TopologyView(state: Signal<InstanceState>) -> Element {
    let state_read = state.read();
    let positions = &state_read.peer_positions;
    let edges = state_read.compute_edges();
    let packets = &state_read.packets_in_flight;
    let peer_ids = state_read.peer_ids();

    rsx! {
        div { class: "topology-container",
            h3 { class: "panel-title", "Network Topology" }
            svg {
                class: "topology-svg",
                view_box: "0 0 700 400",
                preserve_aspect_ratio: "xMidYMid meet",

                // Draw edges first (underneath nodes)
                for (from, to) in edges.iter() {
                    if let (Some(&from_pos), Some(&to_pos)) = (positions.get(from), positions.get(to)) {
                        line {
                            class: "edge-line",
                            x1: "{from_pos.0}",
                            y1: "{from_pos.1}",
                            x2: "{to_pos.0}",
                            y2: "{to_pos.1}",
                        }
                    }
                }

                // Draw packet animations
                for packet in packets.iter() {
                    if let (Some(&from_pos), Some(&to_pos)) = (positions.get(&packet.from), positions.get(&packet.to)) {
                        {
                            let pos = packet.interpolate_position(from_pos, to_pos);
                            rsx! {
                                circle {
                                    class: "packet-dot",
                                    cx: "{pos.0}",
                                    cy: "{pos.1}",
                                    r: "12",
                                    fill: "#00d9ff",
                                    style: "filter: drop-shadow(0 0 8px #00d9ff);",
                                }
                            }
                        }
                    }
                }

                // Draw peer nodes on top
                for peer in peer_ids.iter() {
                    if let Some(&(x, y)) = positions.get(peer) {
                        PeerNode {
                            peer: *peer,
                            x: x,
                            y: y,
                            online: state_read.is_peer_online(*peer),
                            queue_depth: state_read.get_queue_depth(*peer),
                        }
                    }
                }

                // Show placeholder if no simulation
                if peer_ids.is_empty() {
                    text {
                        x: "350",
                        y: "200",
                        text_anchor: "middle",
                        fill: "var(--text-muted)",
                        font_size: "14",
                        "Load a scenario to see the network topology"
                    }
                }
            }
        }
    }
}

/// Individual peer node visualization (SVG group)
#[component]
pub fn PeerNode(peer: PeerId, x: f64, y: f64, online: bool, queue_depth: usize) -> Element {
    let fill = if online {
        "var(--accent-success)"
    } else {
        "var(--accent-error)"
    };
    let label = peer.0.to_string();

    rsx! {
        g {
            class: "peer-node",
            transform: "translate({x}, {y})",

            // Outer ring for queue depth indicator
            if queue_depth > 0 {
                circle {
                    r: "28",
                    fill: "none",
                    stroke: "var(--accent-warning)",
                    stroke_width: "2",
                    stroke_dasharray: format!("{} {}", queue_depth * 10, 100 - queue_depth.min(10) * 10),
                }
            }

            // Main node circle
            circle {
                r: "24",
                fill: fill,
                stroke: "white",
                stroke_width: "2",
            }

            // Peer label
            text {
                text_anchor: "middle",
                dy: "0.35em",
                fill: "white",
                font_weight: "bold",
                font_size: "14",
                "{label}"
            }
        }
    }
}

/// Playback controls for the instance view
#[component]
pub fn InstanceControls(
    state: Signal<InstanceState>,
    on_load_scenario: EventHandler<String>,
) -> Element {
    // Read all values from the signal - this subscribes to updates
    let tick = state.read().current_tick();
    let max_ticks = state.read().max_ticks();
    let paused = state.read().paused;
    let has_simulation = state.read().simulation.is_some();
    let playback_speed = state.read().playback_speed;

    rsx! {
        div { class: "instance-controls",
            // Scenario selector buttons
            div { class: "scenario-buttons",
                span { class: "control-label", "Load: " }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("triangle".to_string()),
                    "Triangle (3)"
                }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("line".to_string()),
                    "Line (5)"
                }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("star".to_string()),
                    "Star (6)"
                }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("ring".to_string()),
                    "Ring (8)"
                }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("mesh".to_string()),
                    "Full Mesh (5)"
                }
            }

            // Step button (advance one tick)
            button {
                class: "control-btn",
                disabled: !has_simulation || !paused,
                onclick: move |_| {
                    // First, step the simulation and collect events
                    let new_events = {
                        let mut state_write = state.write();
                        if let Some(ref mut sim) = state_write.simulation {
                            sim.step();
                            sim.event_log.clone()
                        } else {
                            return;
                        }
                    };
                    // Then update events and create animations in a separate borrow
                    let mut state_write = state.write();
                    let current_count = state_write.recent_events.len();
                    let current_tick = state_write.current_tick();

                    for event in new_events.into_iter().skip(current_count) {
                        // Create packet animations for visual movement
                        match &event {
                            NetworkEvent::Send { from, to, .. } => {
                                let packet_id = PacketId { source: *from, sequence: current_tick };
                                state_write.packets_in_flight.push(PacketAnimation::new(
                                    packet_id, *from, *to, current_tick
                                ));
                            }
                            NetworkEvent::Relay { via, to, packet_id, .. } => {
                                state_write.packets_in_flight.push(PacketAnimation::new(
                                    *packet_id, *via, *to, current_tick
                                ));
                            }
                            NetworkEvent::Delivered { packet_id, .. } => {
                                state_write.packets_in_flight.retain(|p| p.packet_id != *packet_id);
                            }
                            _ => {}
                        }
                        state_write.add_event(event);
                    }

                    // Update animation progress and remove completed ones
                    state_write.packets_in_flight.iter_mut().for_each(|p| p.update(current_tick));
                    state_write.packets_in_flight.retain(|p| !p.is_complete());
                },
                "Step"
            }

            // Play/Pause toggle
            button {
                class: "control-btn",
                disabled: !has_simulation,
                onclick: move |_| {
                    let current = state.read().paused;
                    state.write().paused = !current;
                },
                if paused { "Play" } else { "Pause" }
            }

            // Reset button
            button {
                class: "control-btn secondary",
                disabled: !has_simulation,
                onclick: move |_| {
                    state.write().simulation = None;
                    state.write().clear_events();
                    state.write().packets_in_flight.clear();
                    state.write().peer_positions.clear();
                },
                "Reset"
            }

            // Tick counter and debug info
            if has_simulation {
                {
                    let event_count = state.read().recent_events.len();
                    let log_count = state.read().simulation.as_ref().map(|s| s.event_log.len()).unwrap_or(0);
                    let packet_count = state.read().packets_in_flight.len();
                    rsx! {
                        span { class: "tick-counter", "Tick: {tick} / {max_ticks}" }
                        span { class: "tick-counter", " | Events: {event_count} | Log: {log_count} | Packets: {packet_count}" }
                    }
                }
            }

            // Speed slider
            if has_simulation {
                div { class: "speed-control",
                    label { "Speed: {playback_speed:.1}x" }
                    input {
                        r#type: "range",
                        min: "0.5",
                        max: "10",
                        step: "0.5",
                        value: "{playback_speed}",
                        onchange: move |e| {
                            if let Ok(speed) = e.value().parse::<f64>() {
                                state.write().playback_speed = speed;
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Peer status panel showing all peers
#[component]
pub fn PeerPanel(state: Signal<InstanceState>) -> Element {
    let state_read = state.read();
    let peer_ids = state_read.peer_ids();

    rsx! {
        div { class: "peer-panel",
            h3 { class: "panel-title", "Peers" }
            div { class: "peer-list",
                if peer_ids.is_empty() {
                    div { class: "no-peers", "No peers loaded" }
                } else {
                    for peer in peer_ids.iter() {
                        {
                            let online = state_read.is_peer_online(*peer);
                            let queue = state_read.get_queue_depth(*peer);
                            let inbox = state_read.get_inbox_count(*peer);
                            let status_class = if online { "peer-status online" } else { "peer-status offline" };

                            rsx! {
                                div { class: "peer-item",
                                    span { class: "peer-id", "{peer}" }
                                    span { class: status_class,
                                        if online { "Online" } else { "Offline" }
                                    }
                                    if queue > 0 {
                                        span { class: "peer-queue", "Queue: {queue}" }
                                    }
                                    if inbox > 0 {
                                        span { class: "peer-inbox", "Inbox: {inbox}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Event timeline for instance view
#[component]
pub fn InstanceEventTimeline(state: Signal<InstanceState>) -> Element {
    let state_read = state.read();
    let events = &state_read.recent_events;

    rsx! {
        div { class: "instance-event-timeline",
            h3 { class: "panel-title", "Event Log" }
            div { class: "timeline-scroll",
                if events.is_empty() {
                    div { class: "no-events", "No events yet" }
                } else {
                    for event in events.iter().rev().take(50) {
                        {
                            let (event_class, tick, text) = format_network_event(event);
                            rsx! {
                                div { class: "timeline-event {event_class}",
                                    span { class: "event-tick", "[{tick}]" }
                                    span { class: "event-text", "{text}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
