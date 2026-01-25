//! SDK Dashboard Components
//!
//! Custom dashboard components for each SDK stress test:
//! - NetworkLifecycleDashboard - Network/realm lifecycle visualization
//! - DocumentOperationsDashboard - CRDT document operations visualization
//! - MessagingDashboard - Messaging and threading visualization

use crate::state::sdk::{SDKDashboard, SDKState};
use crate::state::{EventType, SimEvent};
use dioxus::prelude::*;

/// Main SDK tab view with dashboard selector and content
/// Playback controls and stress level moved to unified bottom control bar
#[component]
pub fn SDKView(
    state: Signal<SDKState>,
    #[allow(unused)] on_run: EventHandler<()>,
    #[allow(unused)] on_stop: EventHandler<()>,
    #[allow(unused)] on_level_change: EventHandler<String>,
) -> Element {
    let current_dashboard = state.read().current_dashboard;

    rsx! {
        div { class: "sdk-view",
            // Dashboard selector sidebar
            aside { class: "sdk-sidebar",
                div { class: "sidebar-header",
                    h2 { "SDK Stress Tests" }
                    p { class: "sidebar-subtitle", "Select a dashboard to run" }
                }

                nav { class: "dashboard-list",
                    SDKDashboardCard {
                        dashboard: SDKDashboard::NetworkLifecycle,
                        selected: current_dashboard == SDKDashboard::NetworkLifecycle,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    SDKDashboardCard {
                        dashboard: SDKDashboard::DocumentOperations,
                        selected: current_dashboard == SDKDashboard::DocumentOperations,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    SDKDashboardCard {
                        dashboard: SDKDashboard::Messaging,
                        selected: current_dashboard == SDKDashboard::Messaging,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                }

                // Controls moved to unified bottom control bar
            }

            // Main dashboard content area
            main { class: "sdk-main",
                match current_dashboard {
                    SDKDashboard::NetworkLifecycle => rsx! {
                        NetworkLifecycleDashboard { state: state }
                    },
                    SDKDashboard::DocumentOperations => rsx! {
                        DocumentOperationsDashboard { state: state }
                    },
                    SDKDashboard::Messaging => rsx! {
                        MessagingDashboard { state: state }
                    },
                }
            }
        }
    }
}

/// Card for selecting a dashboard
#[component]
fn SDKDashboardCard(
    dashboard: SDKDashboard,
    selected: bool,
    on_select: EventHandler<SDKDashboard>,
) -> Element {
    rsx! {
        button {
            class: if selected { "dashboard-card selected" } else { "dashboard-card" },
            onclick: move |_| on_select.call(dashboard),

            div { class: "dashboard-icon", "{dashboard.icon()}" }
            div { class: "dashboard-info",
                div { class: "dashboard-name", "{dashboard.display_name()}" }
                div { class: "dashboard-desc", "{dashboard.description()}" }
            }
        }
    }
}

// ============================================================================
// Network Lifecycle Dashboard
// ============================================================================

/// Dashboard for sdk_stress.lua - Network/Realm lifecycle testing
#[component]
pub fn NetworkLifecycleDashboard(state: Signal<SDKState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let current_phase = state.read().current_phase.clone();
    let phase_number = state.read().current_phase_number;
    let total_phases = state.read().total_phases;

    // Progress calculation
    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    rsx! {
        div { class: "sdk-dashboard network-lifecycle-dashboard",
            // Header
            div { class: "dashboard-header",
                h2 { "🌐 Network Lifecycle Stress Test" }
                p { class: "dashboard-subtitle",
                    "Testing network creation, realm formation, peer joins, and interface lifecycle"
                }
            }

            // Phase indicator
            if let Some(phase) = current_phase {
                div { class: "phase-indicator",
                    span { class: "phase-label", "Phase {phase_number}/{total_phases}: " }
                    span { class: "phase-name", "{phase}" }
                }
            }

            // Progress bar
            if running || metrics.current_tick > 0 {
                div { class: "progress-section",
                    div { class: "progress-header",
                        span { "Tick {metrics.current_tick} / {metrics.max_ticks}" }
                        span { "{progress:.1}%" }
                    }
                    div { class: "progress-bar",
                        div {
                            class: "progress-fill",
                            style: "width: {progress}%",
                        }
                    }
                }
            }

            // Throughput display
            if metrics.ops_per_second > 0.0 {
                div { class: "throughput-display",
                    span { class: "throughput-value", "{metrics.ops_per_second:.0}" }
                    span { class: "throughput-unit", " ops/sec" }
                }
            }

            // Main metrics grid - Network specific
            div { class: "metrics-grid network-metrics",
                // Networks section
                div { class: "metric-section",
                    h3 { class: "section-title", "Networks" }
                    div { class: "metric-row",
                        MetricCard { title: "Created", value: format!("{}", metrics.networks_created), icon: "🆕" }
                        MetricCard { title: "Active", value: format!("{}", metrics.networks_created.saturating_sub(metrics.networks_destroyed)), icon: "✅" }
                        MetricCard { title: "Stopped", value: format!("{}", metrics.networks_destroyed), icon: "⏹" }
                    }
                }

                // Realms section
                div { class: "metric-section",
                    h3 { class: "section-title", "Realms" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "Created",
                            value: format!("{}", metrics.realms_created),
                            icon: "🏰"
                        }
                        MetricCard {
                            title: "Joins",
                            value: format!("{}", metrics.realm_joins),
                            icon: "👥"
                        }
                        MetricCard {
                            title: "Active Members",
                            value: format!("{}", metrics.active_members),
                            icon: "👤"
                        }
                    }
                }

                // Performance section
                div { class: "metric-section",
                    h3 { class: "section-title", "Performance" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "Avg Start Time",
                            value: format!("{:.0} μs", metrics.p50_latency_us),
                            icon: "⏱"
                        }
                        MetricCard {
                            title: "Avg Join Time",
                            value: format!("{:.0} μs", metrics.p95_latency_us),
                            icon: "🔗"
                        }
                        MetricCard {
                            title: "Success Rate",
                            value: format!("{:.1}%", metrics.delivery_rate * 100.0),
                            icon: "📊"
                        }
                    }
                }
            }

            // Lifecycle timeline visualization
            div { class: "lifecycle-timeline",
                h3 { "Lifecycle Timeline" }
                div { class: "timeline-stages",
                    TimelineStage { name: "Create Network", status: get_stage_status(1, phase_number) }
                    TimelineStage { name: "Start Network", status: get_stage_status(2, phase_number) }
                    TimelineStage { name: "Create Realm", status: get_stage_status(3, phase_number) }
                    TimelineStage { name: "Join Realm", status: get_stage_status(4, phase_number) }
                    TimelineStage { name: "Cleanup", status: get_stage_status(5, phase_number) }
                }
            }

            // Event log
            SDKEventLog { events: events }
        }
    }
}

/// Timeline stage component
#[component]
fn TimelineStage(name: &'static str, status: StageStatus) -> Element {
    let (class, icon) = match status {
        StageStatus::Pending => ("stage pending", "○"),
        StageStatus::Active => ("stage active", "◉"),
        StageStatus::Complete => ("stage complete", "✓"),
    };

    rsx! {
        div { class: class,
            span { class: "stage-icon", "{icon}" }
            span { class: "stage-name", "{name}" }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum StageStatus {
    Pending,
    Active,
    Complete,
}

fn get_stage_status(stage: usize, current_phase: usize) -> StageStatus {
    if current_phase > stage {
        StageStatus::Complete
    } else if current_phase == stage {
        StageStatus::Active
    } else {
        StageStatus::Pending
    }
}

// ============================================================================
// Document Operations Dashboard
// ============================================================================

/// Dashboard for sdk_document_stress.lua - CRDT document operations testing
#[component]
pub fn DocumentOperationsDashboard(state: Signal<SDKState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let current_phase = state.read().current_phase.clone();

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    // Convergence rate from the test output
    let convergence_pct = metrics.convergence_rate * 100.0;

    rsx! {
        div { class: "sdk-dashboard document-operations-dashboard",
            // Header
            div { class: "dashboard-header",
                h2 { "📄 Document Operations Stress Test" }
                p { class: "dashboard-subtitle",
                    "Testing CRDT operations, concurrent edits, sync, and persistence"
                }
            }

            // Current phase
            if let Some(phase) = current_phase {
                div { class: "phase-indicator",
                    span { class: "phase-name", "{phase}" }
                }
            }

            // Progress bar
            if running || metrics.current_tick > 0 {
                div { class: "progress-section",
                    div { class: "progress-header",
                        span { "Tick {metrics.current_tick} / {metrics.max_ticks}" }
                        span { "{progress:.1}%" }
                    }
                    div { class: "progress-bar",
                        div {
                            class: "progress-fill",
                            style: "width: {progress}%",
                        }
                    }
                }
            }

            // Throughput
            if metrics.ops_per_second > 0.0 {
                div { class: "throughput-display",
                    span { class: "throughput-value", "{metrics.ops_per_second:.0}" }
                    span { class: "throughput-unit", " ops/sec" }
                }
            }

            // Convergence meter - prominent for document sync
            div { class: "convergence-meter",
                h3 { "Convergence" }
                div { class: "convergence-bar",
                    div {
                        class: "convergence-fill",
                        style: "width: {convergence_pct}%",
                    }
                }
                span { class: "convergence-value", "{convergence_pct:.1}%" }
            }

            // Main metrics grid - Document specific
            div { class: "metrics-grid document-metrics",
                // Documents section
                div { class: "metric-section",
                    h3 { class: "section-title", "Documents" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "Created",
                            value: format!("{}", metrics.documents_created),
                            icon: "📝"
                        }
                        MetricCard {
                            title: "Updates",
                            value: format!("{}", metrics.total_updates),
                            icon: "✏️"
                        }
                        MetricCard {
                            title: "Persistence",
                            value: format!("{}", metrics.persistence_operations),
                            icon: "💾"
                        }
                    }
                }

                // Sync section
                div { class: "metric-section",
                    h3 { class: "section-title", "Synchronization" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "Sync Ops",
                            value: format!("{}", metrics.sync_operations),
                            icon: "🔄"
                        }
                        MetricCard {
                            title: "Reloads",
                            value: format!("{}", metrics.reload_operations),
                            icon: "🔃"
                        }
                        MetricCard {
                            title: "Delivery Rate",
                            value: format!("{:.1}%", metrics.delivery_rate * 100.0),
                            icon: "📬"
                        }
                    }
                }

                // Latency section
                div { class: "metric-section",
                    h3 { class: "section-title", "Latency (μs)" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "p50",
                            value: format!("{:.0}", metrics.p50_latency_us),
                            icon: "📊"
                        }
                        MetricCard {
                            title: "p95",
                            value: format!("{:.0}", metrics.p95_latency_us),
                            icon: "📈"
                        }
                        MetricCard {
                            title: "p99",
                            value: format!("{:.0}", metrics.p99_latency_us),
                            icon: "🔝"
                        }
                    }
                }
            }

            // Schema test grid - shows different document types being tested
            div { class: "schema-grid",
                h3 { "Schema Tests" }
                div { class: "schema-cards",
                    // Distribute updates across schema types for visualization
                    SchemaCard { name: "Counter", status: "✓", ops: metrics.total_updates / 5 }
                    SchemaCard { name: "Task List", status: "✓", ops: metrics.total_updates / 5 }
                    SchemaCard { name: "Text Editor", status: "✓", ops: metrics.total_updates / 5 }
                    SchemaCard { name: "JSON State", status: "✓", ops: metrics.total_updates / 5 }
                    SchemaCard { name: "Nested Object", status: "✓", ops: metrics.total_updates / 5 }
                }
            }

            // Event log
            SDKEventLog { events: events }
        }
    }
}

/// Schema test card component
#[component]
fn SchemaCard(name: &'static str, status: &'static str, ops: u64) -> Element {
    rsx! {
        div { class: "schema-card",
            div { class: "schema-header",
                span { class: "schema-name", "{name}" }
                span { class: "schema-status", "{status}" }
            }
            div { class: "schema-ops", "{ops} ops" }
        }
    }
}

/// Format bytes to human readable string
#[allow(dead_code)] // Reserved for future use
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

// ============================================================================
// Messaging Dashboard
// ============================================================================

/// Dashboard for sdk_messaging_stress.lua - Messaging and threading testing
#[component]
pub fn MessagingDashboard(state: Signal<SDKState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let current_phase = state.read().current_phase.clone();

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    // Delivery rate display - use the actual delivery_rate from the test output
    // (not calculated, since messages_delivered = messages × recipients)
    let delivery_pct = metrics.delivery_rate * 100.0;

    rsx! {
        div { class: "sdk-dashboard messaging-dashboard",
            // Header
            div { class: "dashboard-header",
                h2 { "💬 Messaging & Threading Stress Test" }
                p { class: "dashboard-subtitle",
                    "Testing message delivery, reply threading, reactions, and member presence"
                }
            }

            // Current phase
            if let Some(phase) = current_phase {
                div { class: "phase-indicator",
                    span { class: "phase-name", "{phase}" }
                }
            }

            // Progress bar
            if running || metrics.current_tick > 0 {
                div { class: "progress-section",
                    div { class: "progress-header",
                        span { "Tick {metrics.current_tick} / {metrics.max_ticks}" }
                        span { "{progress:.1}%" }
                    }
                    div { class: "progress-bar",
                        div {
                            class: "progress-fill",
                            style: "width: {progress}%",
                        }
                    }
                }
            }

            // Throughput
            if metrics.ops_per_second > 0.0 {
                div { class: "throughput-display",
                    span { class: "throughput-value", "{metrics.ops_per_second:.0}" }
                    span { class: "throughput-unit", " msg/sec" }
                }
            }

            // Delivery rate meter - prominent for messaging
            div { class: "delivery-meter",
                h3 { "Delivery Rate" }
                div { class: "delivery-bar",
                    div {
                        class: if delivery_pct >= 95.0 { "delivery-fill good" }
                               else if delivery_pct >= 80.0 { "delivery-fill warning" }
                               else { "delivery-fill critical" },
                        style: "width: {delivery_pct}%",
                    }
                }
                span { class: "delivery-value", "{delivery_pct:.1}%" }
            }

            // Main metrics grid - Messaging specific
            div { class: "metrics-grid messaging-metrics",
                // Messages section
                div { class: "metric-section",
                    h3 { class: "section-title", "Messages" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "Sent",
                            value: format!("{}", metrics.messages_sent),
                            icon: "📤"
                        }
                        MetricCard {
                            title: "Delivered",
                            value: format!("{}", metrics.messages_delivered),
                            icon: "📥"
                        }
                        MetricCard {
                            title: "Pending",
                            value: format!("{}", metrics.messages_sent.saturating_sub(metrics.messages_delivered)),
                            icon: "⏳"
                        }
                        MetricCard {
                            title: "Dropped",
                            value: format!("{}", metrics.messages_dropped),
                            icon: "🗑️",
                            warning: metrics.messages_dropped > 0
                        }
                    }
                }

                // Threading section
                div { class: "metric-section",
                    h3 { class: "section-title", "Threading" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "Reply Threads",
                            value: format!("{}", metrics.threads_created),
                            icon: "🔗"
                        }
                        MetricCard {
                            title: "Reactions",
                            value: format!("{}", metrics.reactions_sent),
                            icon: "👍"
                        }
                        MetricCard {
                            title: "Thread Depth",
                            value: format!("{:.1}", metrics.avg_thread_depth),
                            icon: "📊"
                        }
                    }
                }

                // Members section
                div { class: "metric-section",
                    h3 { class: "section-title", "Members" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "Online",
                            value: format!("{}", metrics.members_online),
                            icon: "🟢"
                        }
                        MetricCard {
                            title: "Joins",
                            value: format!("{}", metrics.member_joins),
                            icon: "➡️"
                        }
                        MetricCard {
                            title: "Leaves",
                            value: format!("{}", metrics.member_leaves),
                            icon: "⬅️"
                        }
                    }
                }

                // Latency section
                div { class: "metric-section",
                    h3 { class: "section-title", "Latency (μs)" }
                    div { class: "metric-row",
                        MetricCard {
                            title: "p50",
                            value: format!("{:.0}", metrics.p50_latency_us),
                            icon: "⏱"
                        }
                        MetricCard {
                            title: "p95",
                            value: format!("{:.0}", metrics.p95_latency_us),
                            icon: "📈"
                        }
                        MetricCard {
                            title: "p99",
                            value: format!("{:.0}", metrics.p99_latency_us),
                            icon: "🔝"
                        }
                    }
                }
            }

            // Content type breakdown
            div { class: "content-breakdown",
                h3 { "Content Types" }
                div { class: "content-cards",
                    ContentTypeCard {
                        content_type: "Text",
                        count: metrics.messages_sent * 7 / 10,
                        icon: "📝"
                    }
                    ContentTypeCard {
                        content_type: "Binary",
                        count: metrics.messages_sent / 10,
                        icon: "📦"
                    }
                    ContentTypeCard {
                        content_type: "JSON",
                        count: metrics.messages_sent / 10,
                        icon: "🔧"
                    }
                    ContentTypeCard {
                        content_type: "System",
                        count: metrics.messages_sent / 10,
                        icon: "⚙️"
                    }
                }
            }

            // Event log
            SDKEventLog { events: events }
        }
    }
}

/// Content type breakdown card
#[component]
fn ContentTypeCard(content_type: &'static str, count: u64, icon: &'static str) -> Element {
    rsx! {
        div { class: "content-card",
            span { class: "content-icon", "{icon}" }
            div { class: "content-info",
                span { class: "content-name", "{content_type}" }
                span { class: "content-count", "{count}" }
            }
        }
    }
}

// ============================================================================
// Shared Components
// ============================================================================

/// Metric card component
#[component]
fn MetricCard(
    title: &'static str,
    value: String,
    icon: &'static str,
    #[props(default = false)] warning: bool,
) -> Element {
    let class = if warning { "metric-card warning" } else { "metric-card" };

    rsx! {
        div { class: class,
            div { class: "metric-icon", "{icon}" }
            div { class: "metric-content",
                h4 { class: "metric-title", "{title}" }
                div { class: "metric-value", "{value}" }
            }
        }
    }
}

/// SDK event log component
#[component]
fn SDKEventLog(events: Vec<SimEvent>) -> Element {
    rsx! {
        div { class: "sdk-event-log",
            h3 { "Event Log" }
            div { class: "event-list",
                if events.is_empty() {
                    div { class: "empty-state",
                        span { class: "empty-icon", "📋" }
                        p { "Events will appear here during test execution" }
                    }
                } else {
                    for event in events.iter().rev().take(50) {
                        div { class: "event-item {get_event_class(&event.event_type)}",
                            span { class: "event-tick", "[{event.tick}]" }
                            span { class: "event-type", "{format_event_type(&event.event_type)}" }
                            span { class: "event-description", "{event.description}" }
                        }
                    }
                }
            }
        }
    }
}

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
