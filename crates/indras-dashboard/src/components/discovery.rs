//! Discovery Dashboard Components
//!
//! Custom dashboard components for each discovery scenario:
//! - TwoPeerDashboard - Basic mutual discovery visualization
//! - PeerGroupDashboard - Multi-peer realm formation
//! - LateJoinerDashboard - IntroductionRequest timeline
//! - RateLimitDashboard - Rate limiting metrics
//! - ReconnectDashboard - Disconnect/reconnect handling
//! - PQKeysDashboard - Post-quantum key exchange
//! - StressDashboard - High-churn stress test

use crate::state::discovery::{DiscoveryDashboard, DiscoveryState};
use crate::state::EventType;
use dioxus::prelude::*;

/// Main Discovery tab view with dashboard selector and content
#[component]
pub fn DiscoveryView(
    state: Signal<DiscoveryState>,
    #[allow(unused)] on_run: EventHandler<()>,
    #[allow(unused)] on_stop: EventHandler<()>,
    #[allow(unused)] on_level_change: EventHandler<String>,
) -> Element {
    let current_dashboard = state.read().current_dashboard;

    rsx! {
        div { class: "discovery-view",
            // Dashboard selector sidebar
            aside { class: "discovery-sidebar",
                div { class: "sidebar-header",
                    h2 { "Discovery Tests" }
                    p { class: "sidebar-subtitle", "Select a scenario to run" }
                }

                nav { class: "dashboard-list",
                    DiscoveryDashboardCard {
                        dashboard: DiscoveryDashboard::TwoPeer,
                        selected: current_dashboard == DiscoveryDashboard::TwoPeer,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    DiscoveryDashboardCard {
                        dashboard: DiscoveryDashboard::PeerGroup,
                        selected: current_dashboard == DiscoveryDashboard::PeerGroup,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    DiscoveryDashboardCard {
                        dashboard: DiscoveryDashboard::LateJoiner,
                        selected: current_dashboard == DiscoveryDashboard::LateJoiner,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    DiscoveryDashboardCard {
                        dashboard: DiscoveryDashboard::RateLimit,
                        selected: current_dashboard == DiscoveryDashboard::RateLimit,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    DiscoveryDashboardCard {
                        dashboard: DiscoveryDashboard::Reconnect,
                        selected: current_dashboard == DiscoveryDashboard::Reconnect,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    DiscoveryDashboardCard {
                        dashboard: DiscoveryDashboard::PQKeys,
                        selected: current_dashboard == DiscoveryDashboard::PQKeys,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                    DiscoveryDashboardCard {
                        dashboard: DiscoveryDashboard::Stress,
                        selected: current_dashboard == DiscoveryDashboard::Stress,
                        on_select: move |d| state.write().current_dashboard = d,
                    }
                }
            }

            // Main dashboard content area
            main { class: "discovery-main",
                match current_dashboard {
                    DiscoveryDashboard::TwoPeer => rsx! {
                        TwoPeerDashboard { state: state }
                    },
                    DiscoveryDashboard::PeerGroup => rsx! {
                        PeerGroupDashboard { state: state }
                    },
                    DiscoveryDashboard::LateJoiner => rsx! {
                        LateJoinerDashboard { state: state }
                    },
                    DiscoveryDashboard::RateLimit => rsx! {
                        RateLimitDashboard { state: state }
                    },
                    DiscoveryDashboard::Reconnect => rsx! {
                        ReconnectDashboard { state: state }
                    },
                    DiscoveryDashboard::PQKeys => rsx! {
                        PQKeysDashboard { state: state }
                    },
                    DiscoveryDashboard::Stress => rsx! {
                        StressDashboard { state: state }
                    },
                }
            }
        }
    }
}

/// Card for selecting a dashboard
#[component]
fn DiscoveryDashboardCard(
    dashboard: DiscoveryDashboard,
    selected: bool,
    on_select: EventHandler<DiscoveryDashboard>,
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
// Two Peer Dashboard
// ============================================================================

/// Dashboard for discovery_two_peer.lua - Basic mutual discovery
#[component]
pub fn TwoPeerDashboard(state: Signal<DiscoveryState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let realms = state.read().realms_formed.clone();

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    let completeness = metrics.discovery_completeness * 100.0;

    rsx! {
        div { class: "discovery-dashboard two-peer-dashboard",
            // Header
            div { class: "dashboard-header",
                h2 { "Two Peer Discovery" }
                p { class: "dashboard-subtitle",
                    "Basic mutual peer discovery and realm formation"
                }
            }

            // Progress bar
            if running || metrics.current_tick > 0 {
                ProgressSection { current: metrics.current_tick, max: metrics.max_ticks, progress: progress }
            }

            // Peer cards visualization
            div { class: "two-peer-visual",
                PeerCard {
                    name: "Peer A",
                    online: metrics.peers_discovered >= 1,
                    discovered: metrics.discovery_completeness > 0.0
                }

                // Discovery arrows
                div { class: "discovery-arrows",
                    div {
                        class: if completeness >= 50.0 { "arrow arrow-right active" } else { "arrow arrow-right" },
                        "A"
                    }
                    div {
                        class: if completeness >= 100.0 { "arrow arrow-left active" } else { "arrow arrow-left" },
                        "B"
                    }
                }

                PeerCard {
                    name: "Peer B",
                    online: metrics.peers_discovered >= 2,
                    discovered: metrics.discovery_completeness >= 1.0
                }
            }

            // Realm emergence
            div { class: "realm-emergence",
                h3 { "Realm Formation" }
                if realms.is_empty() && metrics.realms_available == 0 {
                    div { class: "realm-pending",
                        span { class: "realm-icon", "..." }
                        span { "Waiting for discovery" }
                    }
                } else {
                    div { class: "realm-formed",
                        span { class: "realm-icon", "" }
                        span { "Realm emerged: A+B" }
                    }
                }
            }

            // Metrics grid
            div { class: "metrics-grid discovery-metrics",
                MetricSection {
                    title: "Discovery",
                    cards: vec![
                        ("Peers Discovered", format!("{}", metrics.peers_discovered), ""),
                        ("Completeness", format!("{:.0}%", completeness), ""),
                        ("Failures", format!("{}", metrics.discovery_failures), if metrics.discovery_failures > 0 { "" } else { "" }),
                    ]
                }

                MetricSection {
                    title: "PQ Crypto",
                    cards: vec![
                        ("KEM Ops", format!("{}", metrics.kem_encapsulations), ""),
                        ("Signatures", format!("{}", metrics.pq_signatures_created), ""),
                        ("Key Completeness", format!("{:.0}%", metrics.pq_key_completeness * 100.0), ""),
                    ]
                }
            }

            // Event log
            DiscoveryEventLog { events: events }
        }
    }
}

// ============================================================================
// Peer Group Dashboard
// ============================================================================

/// Dashboard for discovery_peer_group.lua - Multi-peer realm formation
#[component]
pub fn PeerGroupDashboard(state: Signal<DiscoveryState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let discovery_matrix = state.read().discovery_matrix.clone();

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    let completeness = metrics.discovery_completeness * 100.0;
    let peer_count = if discovery_matrix.is_empty() { 4 } else { discovery_matrix.len() };
    // For N peers, max possible realms is 2^N - N - 1
    let max_realms = if peer_count > 1 {
        (1_u64 << peer_count) - (peer_count as u64) - 1
    } else {
        0
    };

    rsx! {
        div { class: "discovery-dashboard peer-group-dashboard",
            div { class: "dashboard-header",
                h2 { "Peer Group Discovery" }
                p { class: "dashboard-subtitle",
                    "Multi-peer discovery with overlapping realm formation"
                }
            }

            if running || metrics.current_tick > 0 {
                ProgressSection { current: metrics.current_tick, max: metrics.max_ticks, progress: progress }
            }

            // Completeness meter
            div { class: "convergence-meter",
                h3 { "Discovery Completeness" }
                div { class: "convergence-bar",
                    div {
                        class: "convergence-fill",
                        style: "width: {completeness}%",
                    }
                }
                span { class: "convergence-value", "{completeness:.1}%" }
            }

            // Discovery matrix
            DiscoveryMatrix { matrix: discovery_matrix.clone(), peer_count: peer_count }

            // Metrics
            div { class: "metrics-grid discovery-metrics",
                MetricSection {
                    title: "Peers",
                    cards: vec![
                        ("Total", format!("{}", peer_count), ""),
                        ("Discovered", format!("{}", metrics.peers_discovered), ""),
                        ("Failures", format!("{}", metrics.discovery_failures), ""),
                    ]
                }

                MetricSection {
                    title: "Realms",
                    cards: vec![
                        ("Formed", format!("{}", metrics.realms_available), ""),
                        ("Max Possible", format!("{}", max_realms), ""),
                        ("Active Joins", format!("{}", metrics.realm_joins), ""),
                    ]
                }
            }

            DiscoveryEventLog { events: events }
        }
    }
}

// ============================================================================
// Late Joiner Dashboard
// ============================================================================

/// Dashboard for discovery_late_joiner.lua - IntroductionRequest handling
#[component]
pub fn LateJoinerDashboard(state: Signal<DiscoveryState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let current_phase = state.read().current_phase.clone();
    let phase_number = state.read().current_phase_number;
    let total_phases = state.read().total_phases;

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    rsx! {
        div { class: "discovery-dashboard late-joiner-dashboard",
            div { class: "dashboard-header",
                h2 { "Late Joiner Discovery" }
                p { class: "dashboard-subtitle",
                    "Late joiner discovery via IntroductionRequest messages"
                }
            }

            if let Some(phase) = current_phase {
                div { class: "phase-indicator",
                    span { class: "phase-label", "Phase {phase_number}/{total_phases}: " }
                    span { class: "phase-name", "{phase}" }
                }
            }

            if running || metrics.current_tick > 0 {
                ProgressSection { current: metrics.current_tick, max: metrics.max_ticks, progress: progress }
            }

            // Introduction flow timeline
            div { class: "introduction-timeline",
                h3 { "Introduction Flow" }
                div { class: "timeline-stages",
                    TimelineStage { name: "Existing Peers Online", status: get_stage_status(1, phase_number) }
                    TimelineStage { name: "Late Joiner Connects", status: get_stage_status(2, phase_number) }
                    TimelineStage { name: "IntroRequest Sent", status: get_stage_status(3, phase_number) }
                    TimelineStage { name: "IntroResponse Received", status: get_stage_status(4, phase_number) }
                    TimelineStage { name: "Discovery Complete", status: get_stage_status(5, phase_number) }
                }
            }

            // Metrics
            div { class: "metrics-grid discovery-metrics",
                MetricSection {
                    title: "Introduction Messages",
                    cards: vec![
                        ("Requests Sent", format!("{}", metrics.introduction_requests_sent), ""),
                        ("Responses Received", format!("{}", metrics.introduction_responses_received), ""),
                        ("Pending", format!("{}", metrics.introduction_requests_sent.saturating_sub(metrics.introduction_responses_received)), ""),
                    ]
                }

                MetricSection {
                    title: "Discovery",
                    cards: vec![
                        ("Peers", format!("{}", metrics.peers_discovered), ""),
                        ("Realms Joined", format!("{}", metrics.realm_joins), ""),
                        ("Latency p99", format!("{} ticks", metrics.discovery_latency_p99_ticks), ""),
                    ]
                }
            }

            DiscoveryEventLog { events: events }
        }
    }
}

// ============================================================================
// Rate Limit Dashboard
// ============================================================================

/// Dashboard for discovery_rate_limit.lua - Rate limiting verification
#[component]
pub fn RateLimitDashboard(state: Signal<DiscoveryState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    let allowed = metrics.introduction_requests_sent.saturating_sub(metrics.rate_limited_count);
    let total_requests = metrics.introduction_requests_sent;
    let allowed_pct = if total_requests > 0 {
        (allowed as f64 / total_requests as f64) * 100.0
    } else {
        100.0
    };

    rsx! {
        div { class: "discovery-dashboard rate-limit-dashboard",
            div { class: "dashboard-header",
                h2 { "Rate Limit Verification" }
                p { class: "dashboard-subtitle",
                    "Testing rate limiting with 30-tick window enforcement"
                }
            }

            if running || metrics.current_tick > 0 {
                ProgressSection { current: metrics.current_tick, max: metrics.max_ticks, progress: progress }
            }

            // Rate limit meter
            div { class: "rate-limit-meter",
                h3 { "Request Breakdown" }
                div { class: "rate-bar",
                    div {
                        class: "rate-fill allowed",
                        style: "width: {allowed_pct}%",
                    }
                }
                div { class: "rate-legend",
                    span { class: "legend-allowed", "{allowed} Allowed" }
                    span { class: "legend-blocked", "{metrics.rate_limited_count} Rate-Limited" }
                }
            }

            // 30-tick window indicator
            div { class: "window-indicator",
                h3 { "Rate Limit Window" }
                div { class: "window-visual",
                    span { class: "window-value", "30" }
                    span { class: "window-unit", "ticks" }
                }
                p { class: "window-desc", "Maximum requests per window: 5" }
            }

            // Metrics
            div { class: "metrics-grid discovery-metrics",
                MetricSection {
                    title: "Requests",
                    cards: vec![
                        ("Total Sent", format!("{}", total_requests), ""),
                        ("Allowed", format!("{}", allowed), ""),
                        ("Blocked", format!("{}", metrics.rate_limited_count), ""),
                    ]
                }

                MetricSection {
                    title: "Violations",
                    cards: vec![
                        ("Window Violations", format!("{}", metrics.rate_limit_violations), if metrics.rate_limit_violations > 0 { "" } else { "" }),
                        ("Discovery OK", format!("{}", metrics.peers_discovered), ""),
                        ("Failures", format!("{}", metrics.discovery_failures), ""),
                    ]
                }
            }

            DiscoveryEventLog { events: events }
        }
    }
}

// ============================================================================
// Reconnect Dashboard
// ============================================================================

/// Dashboard for discovery_reconnect.lua - Disconnect/reconnect handling
#[component]
pub fn ReconnectDashboard(state: Signal<DiscoveryState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let current_phase = state.read().current_phase.clone();

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    let reconnect_success_rate = if metrics.churn_events > 0 {
        (metrics.reconnect_count as f64 / metrics.churn_events as f64) * 100.0
    } else {
        100.0
    };

    rsx! {
        div { class: "discovery-dashboard reconnect-dashboard",
            div { class: "dashboard-header",
                h2 { "Reconnect Handling" }
                p { class: "dashboard-subtitle",
                    "Testing peer disconnect and reconnect scenarios"
                }
            }

            if let Some(phase) = current_phase {
                div { class: "phase-indicator",
                    span { class: "phase-name", "{phase}" }
                }
            }

            if running || metrics.current_tick > 0 {
                ProgressSection { current: metrics.current_tick, max: metrics.max_ticks, progress: progress }
            }

            // Reconnect success meter
            div { class: "reconnect-meter",
                h3 { "Reconnect Success Rate" }
                div { class: "convergence-bar",
                    div {
                        class: if reconnect_success_rate >= 95.0 { "convergence-fill" }
                               else if reconnect_success_rate >= 80.0 { "delivery-fill warning" }
                               else { "delivery-fill critical" },
                        style: "width: {reconnect_success_rate}%",
                    }
                }
                span { class: "convergence-value", "{reconnect_success_rate:.1}%" }
            }

            // Peer status grid
            div { class: "peer-status-grid",
                h3 { "Peer Status" }
                div { class: "status-cards",
                    StatusCard { label: "Online", count: metrics.active_members, icon: "" }
                    StatusCard { label: "Disconnected", count: metrics.churn_events.saturating_sub(metrics.reconnect_count), icon: "" }
                    StatusCard { label: "Reconnected", count: metrics.reconnect_count, icon: "" }
                }
            }

            // Metrics
            div { class: "metrics-grid discovery-metrics",
                MetricSection {
                    title: "Churn Events",
                    cards: vec![
                        ("Total", format!("{}", metrics.churn_events), ""),
                        ("Reconnects", format!("{}", metrics.reconnect_count), ""),
                        ("Still Offline", format!("{}", metrics.churn_events.saturating_sub(metrics.reconnect_count)), ""),
                    ]
                }

                MetricSection {
                    title: "Discovery",
                    cards: vec![
                        ("Re-discovered", format!("{}", metrics.peers_discovered), ""),
                        ("Realms Restored", format!("{}", metrics.realms_available), ""),
                        ("Latency p99", format!("{} ticks", metrics.discovery_latency_p99_ticks), ""),
                    ]
                }
            }

            DiscoveryEventLog { events: events }
        }
    }
}

// ============================================================================
// PQ Keys Dashboard
// ============================================================================

/// Dashboard for discovery_pq_keys.lua - Post-quantum key exchange
#[component]
pub fn PQKeysDashboard(state: Signal<DiscoveryState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    let pq_completeness = metrics.pq_key_completeness * 100.0;

    rsx! {
        div { class: "discovery-dashboard pq-keys-dashboard",
            div { class: "dashboard-header",
                h2 { "PQ Key Exchange" }
                p { class: "dashboard-subtitle",
                    "Post-quantum key exchange using ML-KEM-768 and ML-DSA-65"
                }
            }

            if running || metrics.current_tick > 0 {
                ProgressSection { current: metrics.current_tick, max: metrics.max_ticks, progress: progress }
            }

            // PQ completeness meter
            div { class: "convergence-meter",
                h3 { "Key Exchange Completeness" }
                div { class: "convergence-bar",
                    div {
                        class: "convergence-fill",
                        style: "width: {pq_completeness}%",
                    }
                }
                span { class: "convergence-value", "{pq_completeness:.1}%" }
            }

            // PQ Key indicators
            div { class: "pq-key-status",
                h3 { "Algorithm Details" }
                div { class: "key-badges",
                    div { class: "key-badge ml-kem",
                        span { class: "badge-name", "ML-KEM-768" }
                        span { class: "badge-size", "1088 bytes ciphertext" }
                        span { class: if metrics.kem_encapsulations > 0 { "badge-status valid" } else { "badge-status pending" },
                            if metrics.kem_encapsulations > 0 { "" } else { "" }
                        }
                    }
                    div { class: "key-badge ml-dsa",
                        span { class: "badge-name", "ML-DSA-65" }
                        span { class: "badge-size", "1952 bytes public key" }
                        span { class: if metrics.pq_signatures_verified > 0 { "badge-status valid" } else { "badge-status pending" },
                            if metrics.pq_signatures_verified > 0 { "" } else { "" }
                        }
                    }
                }
            }

            // Metrics
            div { class: "metrics-grid discovery-metrics",
                MetricSection {
                    title: "KEM Operations",
                    cards: vec![
                        ("Encapsulations", format!("{}", metrics.kem_encapsulations), ""),
                        ("Decapsulations", format!("{}", metrics.kem_decapsulations), ""),
                        ("Failures", format!("{}", metrics.kem_failures), if metrics.kem_failures > 0 { "" } else { "" }),
                    ]
                }

                MetricSection {
                    title: "Signatures",
                    cards: vec![
                        ("Created", format!("{}", metrics.pq_signatures_created), ""),
                        ("Verified", format!("{}", metrics.pq_signatures_verified), ""),
                        ("Failures", format!("{}", metrics.pq_signature_failures), if metrics.pq_signature_failures > 0 { "" } else { "" }),
                    ]
                }

                MetricSection {
                    title: "Latency (us)",
                    cards: vec![
                        ("Sign", format!("{:.0}", metrics.avg_sign_latency_us), ""),
                        ("Verify", format!("{:.0}", metrics.avg_verify_latency_us), ""),
                        ("Encap", format!("{:.0}", metrics.avg_encap_latency_us), ""),
                    ]
                }
            }

            DiscoveryEventLog { events: events }
        }
    }
}

// ============================================================================
// Stress Dashboard
// ============================================================================

/// Dashboard for discovery_stress.lua - High-churn stress test
#[component]
pub fn StressDashboard(state: Signal<DiscoveryState>) -> Element {
    let metrics = state.read().metrics.clone();
    let events = state.read().events.clone();
    let running = state.read().running;
    let convergence_achieved = state.read().convergence_achieved;
    let convergence_tick = state.read().convergence_tick;

    let progress = if metrics.max_ticks > 0 {
        (metrics.current_tick as f64 / metrics.max_ticks as f64) * 100.0
    } else {
        0.0
    };

    let completeness = metrics.discovery_completeness * 100.0;

    // Calculate churn rate (events per tick)
    let churn_rate = if metrics.current_tick > 0 {
        metrics.churn_events as f64 / metrics.current_tick as f64
    } else {
        0.0
    };

    rsx! {
        div { class: "discovery-dashboard stress-dashboard",
            div { class: "dashboard-header",
                h2 { "Discovery Stress Test" }
                p { class: "dashboard-subtitle",
                    "High-churn stress test with convergence tracking"
                }
            }

            if running || metrics.current_tick > 0 {
                ProgressSection { current: metrics.current_tick, max: metrics.max_ticks, progress: progress }
            }

            // Throughput display
            if metrics.ops_per_second > 0.0 {
                div { class: "throughput-display",
                    span { class: "throughput-value", "{metrics.ops_per_second:.0}" }
                    span { class: "throughput-unit", " ops/sec" }
                }
            }

            // Convergence meter
            div { class: "convergence-meter",
                h3 { "Discovery Convergence" }
                div { class: "convergence-bar",
                    div {
                        class: "convergence-fill",
                        style: "width: {completeness}%",
                    }
                }
                div { class: "convergence-info",
                    span { class: "convergence-value", "{completeness:.1}%" }
                    if convergence_achieved {
                        if let Some(tick) = convergence_tick {
                            span { class: "convergence-tick", " (achieved at tick {tick})" }
                        }
                    }
                }
            }

            // Churn indicator
            div { class: "churn-indicator",
                h3 { "Churn Rate" }
                div { class: "churn-visual",
                    span { class: "churn-value", "{churn_rate:.2}" }
                    span { class: "churn-unit", " events/tick" }
                }
            }

            // Peer grid visualization
            div { class: "stress-peer-grid",
                h3 { "Peer Status Grid" }
                div { class: "peer-grid-visual",
                    for i in 0..16 {
                        {
                            let online = i < metrics.active_members as usize;
                            rsx! {
                                div {
                                    class: if online { "peer-cell online" } else { "peer-cell offline" },
                                    key: "{i}",
                                }
                            }
                        }
                    }
                }
                div { class: "grid-legend",
                    span { class: "legend-online", "{metrics.active_members} online" }
                    span { class: "legend-offline", "{16_u64.saturating_sub(metrics.active_members)} offline" }
                }
            }

            // Metrics
            div { class: "metrics-grid discovery-metrics",
                MetricSection {
                    title: "Churn",
                    cards: vec![
                        ("Total Events", format!("{}", metrics.churn_events), ""),
                        ("Reconnects", format!("{}", metrics.reconnect_count), ""),
                        ("Failures", format!("{}", metrics.discovery_failures), ""),
                    ]
                }

                MetricSection {
                    title: "Discovery",
                    cards: vec![
                        ("Peers Discovered", format!("{}", metrics.peers_discovered), ""),
                        ("Realms Available", format!("{}", metrics.realms_available), ""),
                        ("Latency p99", format!("{} ticks", metrics.discovery_latency_p99_ticks), ""),
                    ]
                }
            }

            DiscoveryEventLog { events: events }
        }
    }
}

// ============================================================================
// Shared Components
// ============================================================================

/// Progress section component
#[component]
fn ProgressSection(current: u64, max: u64, progress: f64) -> Element {
    rsx! {
        div { class: "progress-section",
            div { class: "progress-header",
                span { "Tick {current} / {max}" }
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
}

/// Metric section component
#[component]
fn MetricSection(title: &'static str, cards: Vec<(&'static str, String, &'static str)>) -> Element {
    rsx! {
        div { class: "metric-section",
            h3 { class: "section-title", "{title}" }
            div { class: "metric-row",
                for (card_title, value, icon) in cards {
                    MetricCard { title: card_title, value: value.clone(), icon: icon }
                }
            }
        }
    }
}

/// Metric card component
#[component]
fn MetricCard(title: &'static str, value: String, icon: &'static str) -> Element {
    rsx! {
        div { class: "metric-card",
            div { class: "metric-icon", "{icon}" }
            div { class: "metric-content",
                h4 { class: "metric-title", "{title}" }
                div { class: "metric-value", "{value}" }
            }
        }
    }
}

/// Peer card for two-peer visualization
#[component]
fn PeerCard(name: &'static str, online: bool, discovered: bool) -> Element {
    let status_class = if discovered {
        "peer-card discovered"
    } else if online {
        "peer-card online"
    } else {
        "peer-card offline"
    };

    rsx! {
        div { class: status_class,
            div { class: "peer-avatar",
                if discovered { "" } else if online { "" } else { "" }
            }
            div { class: "peer-name", "{name}" }
            div { class: "peer-status",
                if discovered { "Discovered" } else if online { "Online" } else { "Offline" }
            }
        }
    }
}

/// Timeline stage component
#[component]
fn TimelineStage(name: &'static str, status: StageStatus) -> Element {
    let (class, icon) = match status {
        StageStatus::Pending => ("stage pending", ""),
        StageStatus::Active => ("stage active", ""),
        StageStatus::Complete => ("stage complete", ""),
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

/// Discovery matrix visualization
#[component]
fn DiscoveryMatrix(
    matrix: std::collections::HashMap<String, std::collections::HashMap<String, bool>>,
    peer_count: usize,
) -> Element {
    let peers: Vec<String> = if matrix.is_empty() {
        (0..peer_count).map(|i| format!("Peer {}", (b'A' + i as u8) as char)).collect()
    } else {
        let mut p: Vec<String> = matrix.keys().cloned().collect();
        p.sort();
        p
    };

    rsx! {
        div { class: "discovery-matrix-container",
            h3 { "Discovery Matrix" }
            div {
                class: "discovery-matrix",
                style: "grid-template-columns: auto repeat({peer_count}, 1fr)",

                // Header row
                div { class: "matrix-header" }
                for peer in peers.iter() {
                    div { class: "matrix-header", "{peer}" }
                }

                // Data rows
                for from_peer in peers.iter() {
                    div { class: "matrix-row-header", "{from_peer}" }
                    for to_peer in peers.iter() {
                        {
                            let discovered = if from_peer == to_peer {
                                true // Self always "discovered"
                            } else {
                                matrix
                                    .get(from_peer)
                                    .and_then(|m| m.get(to_peer))
                                    .copied()
                                    .unwrap_or(false)
                            };
                            let class = if from_peer == to_peer {
                                "matrix-cell self"
                            } else if discovered {
                                "matrix-cell discovered"
                            } else {
                                "matrix-cell pending"
                            };
                            rsx! {
                                div {
                                    class: class,
                                    if from_peer == to_peer { "-" } else if discovered { "" } else { "" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Status card for reconnect dashboard
#[component]
fn StatusCard(label: &'static str, count: u64, icon: &'static str) -> Element {
    rsx! {
        div { class: "status-card",
            span { class: "status-icon", "{icon}" }
            span { class: "status-count", "{count}" }
            span { class: "status-label", "{label}" }
        }
    }
}

/// Discovery event log component
#[component]
fn DiscoveryEventLog(events: Vec<crate::state::SimEvent>) -> Element {
    rsx! {
        div { class: "discovery-event-log",
            h3 { "Event Log" }
            div { class: "event-list",
                if events.is_empty() {
                    div { class: "empty-state",
                        span { class: "empty-icon", "" }
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
