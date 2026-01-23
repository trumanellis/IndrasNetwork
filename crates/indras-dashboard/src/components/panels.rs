//! Test-specific metric panels
//!
//! Adaptive panels that display the most relevant metrics based on test type.

use dioxus::prelude::*;
use crate::state::{SimMetrics, TestCategory, MetricsHistory, DataPoint};
use super::charts::{LineChart, HealthGauge, LatencyBars, PhaseTimeline};

/// Adaptive metrics panel that shows relevant content based on test type
#[component]
pub fn AdaptiveMetricsPanel(
    /// Current metrics
    metrics: SimMetrics,
    /// Historical metrics for charts
    history: MetricsHistory,
    /// Detected test category
    category: TestCategory,
    /// Phase markers for integration tests
    #[props(default = vec![])]
    phases: Vec<(String, u64, u64)>,
) -> Element {
    rsx! {
        div {
            class: "adaptive-panel",

            // Category badge
            div {
                class: "category-badge",
                style: "margin-bottom: 16px;",
                span {
                    style: "background: var(--bg-hover); padding: 4px 12px; border-radius: 12px; font-size: 0.75rem; color: var(--accent-primary);",
                    "{category.display_name()}"
                }
            }

            // Render appropriate panel based on category
            match category {
                TestCategory::PQCrypto => rsx! {
                    PQCryptoPanel { metrics: metrics, history: history }
                },
                TestCategory::Routing => rsx! {
                    RoutingPanel { metrics: metrics, history: history }
                },
                TestCategory::Transport => rsx! {
                    TransportPanel { metrics: metrics, history: history }
                },
                TestCategory::Sync => rsx! {
                    SyncPanel { metrics: metrics, history: history, phases: phases }
                },
                TestCategory::Integration => rsx! {
                    IntegrationPanel { metrics: metrics, history: history, phases: phases }
                },
                TestCategory::Unknown => rsx! {
                    GenericMetricsPanel { metrics: metrics, history: history }
                },
            }
        }
    }
}

/// PQ Crypto specific panel
#[component]
pub fn PQCryptoPanel(
    metrics: SimMetrics,
    history: MetricsHistory,
) -> Element {
    let signature_failure_rate = if metrics.pq_signatures_created > 0 {
        metrics.pq_signature_failures as f64 / metrics.pq_signatures_created as f64
    } else {
        0.0
    };

    let kem_failure_rate = if metrics.kem_encapsulations > 0 {
        metrics.kem_failures as f64 / metrics.kem_encapsulations as f64
    } else {
        0.0
    };

    rsx! {
        div {
            class: "pq-crypto-panel",

            // Top row: Key metrics
            div {
                class: "metrics-row",
                style: "display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-bottom: 24px;",

                div {
                    class: "metric-card",
                    h3 { "Signatures Created" }
                    div { class: "metric-value large", "{metrics.pq_signatures_created}" }
                }

                div {
                    class: "metric-card",
                    h3 { "Signatures Verified" }
                    div { class: "metric-value large", "{metrics.pq_signatures_verified}" }
                }

                div {
                    class: "metric-card",
                    h3 { "KEM Encapsulations" }
                    div { class: "metric-value large", "{metrics.kem_encapsulations}" }
                }

                div {
                    class: "metric-card",
                    h3 { "Throughput" }
                    div { class: "metric-value large", "{metrics.ops_per_second:.0}/s" }
                }
            }

            // Middle row: Charts and gauges
            div {
                class: "charts-row",
                style: "display: grid; grid-template-columns: 2fr 1fr 1fr; gap: 24px; margin-bottom: 24px;",

                // Throughput chart
                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Operations/Second" }
                    LineChart {
                        data: history.ops_per_second.clone(),
                        width: 350,
                        height: 150,
                        fill: true,
                        y_min: Some(0.0),
                    }
                }

                // Signature failure gauge
                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px; text-align: center;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Signature Failure Rate" }
                    HealthGauge {
                        value: 1.0 - signature_failure_rate,
                        size: 120,
                        label: "".to_string(),
                        warning_threshold: 0.95,
                        danger_threshold: 0.9,
                    }
                    div {
                        style: "font-size: 0.75rem; color: var(--text-secondary); margin-top: 8px;",
                        "{metrics.pq_signature_failures} failures"
                    }
                }

                // KEM failure gauge
                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px; text-align: center;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "KEM Failure Rate" }
                    HealthGauge {
                        value: 1.0 - kem_failure_rate,
                        size: 120,
                        label: "".to_string(),
                        warning_threshold: 0.95,
                        danger_threshold: 0.9,
                    }
                    div {
                        style: "font-size: 0.75rem; color: var(--text-secondary); margin-top: 8px;",
                        "{metrics.kem_failures} failures"
                    }
                }
            }

            // Bottom row: Latency percentiles
            div {
                class: "latency-row",
                style: "display: grid; grid-template-columns: 1fr 1fr; gap: 24px;",

                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Signature Latency" }

                    div {
                        style: "display: flex; justify-content: space-between; align-items: center;",

                        div {
                            style: "font-size: 0.875rem;",
                            span { style: "color: var(--text-muted);", "Sign: " }
                            span { style: "color: var(--accent-primary);", "{metrics.avg_sign_latency_us:.0} us" }
                        }

                        div {
                            style: "font-size: 0.875rem;",
                            span { style: "color: var(--text-muted);", "Verify: " }
                            span { style: "color: var(--accent-primary);", "{metrics.avg_verify_latency_us:.0} us" }
                        }
                    }
                }

                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "KEM Latency" }

                    div {
                        style: "display: flex; justify-content: space-between; align-items: center;",

                        div {
                            style: "font-size: 0.875rem;",
                            span { style: "color: var(--text-muted);", "Encap: " }
                            span { style: "color: var(--accent-primary);", "{metrics.avg_encap_latency_us:.0} us" }
                        }

                        div {
                            style: "font-size: 0.875rem;",
                            span { style: "color: var(--text-muted);", "Decap: " }
                            span { style: "color: var(--accent-primary);", "{metrics.avg_decap_latency_us:.0} us" }
                        }
                    }
                }
            }
        }
    }
}

/// Routing specific panel
#[component]
pub fn RoutingPanel(
    metrics: SimMetrics,
    history: MetricsHistory,
) -> Element {
    let delivery_rate = if metrics.messages_sent > 0 {
        metrics.messages_delivered as f64 / metrics.messages_sent as f64
    } else {
        0.0
    };

    rsx! {
        div {
            class: "routing-panel",

            // Top row: Primary gauge and key stats
            div {
                style: "display: grid; grid-template-columns: 200px 1fr; gap: 24px; margin-bottom: 24px;",

                // Large delivery rate gauge
                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 24px; text-align: center;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 16px;", "Delivery Rate" }
                    HealthGauge {
                        value: delivery_rate,
                        size: 160,
                        warning_threshold: 0.8,
                        danger_threshold: 0.6,
                    }
                }

                // Stats grid
                div {
                    style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px;",

                    div {
                        class: "metric-card",
                        h3 { "Sent" }
                        div { class: "metric-value large", "{metrics.messages_sent}" }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Delivered" }
                        div { class: "metric-value large", style: "color: var(--accent-success);", "{metrics.messages_delivered}" }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Dropped" }
                        div {
                            class: "metric-value large",
                            style: if metrics.messages_dropped > 0 { "color: var(--accent-error);" } else { "" },
                            "{metrics.messages_dropped}"
                        }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Avg Latency" }
                        div { class: "metric-value", "{metrics.avg_latency:.1} ticks" }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Avg Hops" }
                        div { class: "metric-value", "{metrics.avg_hops:.2}" }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Backprops" }
                        div { class: "metric-value", "{metrics.backprops_completed}" }
                    }
                }
            }

            // Bottom row: Charts
            div {
                style: "display: grid; grid-template-columns: 1fr 1fr; gap: 24px;",

                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Delivery Rate Over Time" }
                    LineChart {
                        data: history.delivery_rate.clone(),
                        width: 400,
                        height: 150,
                        fill: true,
                        y_min: Some(0.0),
                        y_max: Some(1.0),
                        color: "var(--accent-success)".to_string(),
                    }
                }

                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Average Latency Over Time" }
                    LineChart {
                        data: history.avg_latency.clone(),
                        width: 400,
                        height: 150,
                        fill: true,
                        y_min: Some(0.0),
                        color: "var(--accent-warning)".to_string(),
                    }
                }
            }
        }
    }
}

/// Transport specific panel
#[component]
pub fn TransportPanel(
    metrics: SimMetrics,
    history: MetricsHistory,
) -> Element {
    let delivery_rate = if metrics.messages_sent > 0 {
        metrics.messages_delivered as f64 / metrics.messages_sent as f64
    } else {
        0.0
    };

    rsx! {
        div {
            class: "transport-panel",

            // Stats row
            div {
                style: "display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-bottom: 24px;",

                div {
                    class: "metric-card",
                    h3 { "Messages Sent" }
                    div { class: "metric-value large", "{metrics.messages_sent}" }
                }

                div {
                    class: "metric-card",
                    h3 { "Messages Delivered" }
                    div { class: "metric-value large", "{metrics.messages_delivered}" }
                }

                div {
                    class: "metric-card",
                    h3 { "Delivery Rate" }
                    div { class: "metric-value large", "{delivery_rate * 100.0:.1}%" }
                }

                div {
                    class: "metric-card",
                    h3 { "Avg Latency" }
                    div { class: "metric-value large", "{metrics.avg_latency:.1}" }
                }
            }

            // Chart
            div {
                style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Delivery Rate Over Time" }
                LineChart {
                    data: history.delivery_rate.clone(),
                    width: 600,
                    height: 200,
                    fill: true,
                    y_min: Some(0.0),
                    y_max: Some(1.0),
                }
            }
        }
    }
}

/// Sync specific panel
#[component]
pub fn SyncPanel(
    metrics: SimMetrics,
    history: MetricsHistory,
    phases: Vec<(String, u64, u64)>,
) -> Element {
    let delivery_rate = if metrics.messages_sent > 0 {
        metrics.messages_delivered as f64 / metrics.messages_sent as f64
    } else {
        0.0
    };

    rsx! {
        div {
            class: "sync-panel",

            // Phase timeline if present
            if !phases.is_empty() {
                div {
                    style: "margin-bottom: 24px;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Sync Progress" }
                    PhaseTimeline {
                        phases: phases,
                        current_tick: metrics.current_tick,
                        total_ticks: metrics.max_ticks,
                        width: 600,
                    }
                }
            }

            // Stats
            div {
                style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-bottom: 24px;",

                div {
                    class: "metric-card",
                    h3 { "Sync Messages" }
                    div { class: "metric-value large", "{metrics.messages_sent}" }
                }

                div {
                    class: "metric-card",
                    h3 { "Delivery Rate" }
                    HealthGauge {
                        value: delivery_rate,
                        size: 100,
                    }
                }

                div {
                    class: "metric-card",
                    h3 { "Avg Latency" }
                    div { class: "metric-value large", "{metrics.avg_latency:.1} ticks" }
                }
            }

            // Chart
            div {
                style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Sync Progress Over Time" }
                LineChart {
                    data: history.delivery_rate.clone(),
                    width: 600,
                    height: 180,
                    fill: true,
                    y_min: Some(0.0),
                    y_max: Some(1.0),
                    color: "var(--accent-secondary)".to_string(),
                }
            }
        }
    }
}

/// Integration test panel
#[component]
pub fn IntegrationPanel(
    metrics: SimMetrics,
    history: MetricsHistory,
    phases: Vec<(String, u64, u64)>,
) -> Element {
    let delivery_rate = if metrics.messages_sent > 0 {
        metrics.messages_delivered as f64 / metrics.messages_sent as f64
    } else {
        0.0
    };

    let crypto_success_rate = if metrics.pq_signatures_created > 0 {
        1.0 - (metrics.pq_signature_failures as f64 / metrics.pq_signatures_created as f64)
    } else {
        1.0
    };

    rsx! {
        div {
            class: "integration-panel",

            // Phase timeline
            div {
                style: "margin-bottom: 24px;",
                h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Test Phases" }
                PhaseTimeline {
                    phases: if phases.is_empty() {
                        vec![
                            ("Setup".to_string(), 0, metrics.max_ticks / 4),
                            ("Normal".to_string(), metrics.max_ticks / 4, metrics.max_ticks / 2),
                            ("Partition".to_string(), metrics.max_ticks / 2, metrics.max_ticks * 3 / 4),
                            ("Recovery".to_string(), metrics.max_ticks * 3 / 4, metrics.max_ticks),
                        ]
                    } else {
                        phases.clone()
                    },
                    current_tick: metrics.current_tick,
                    total_ticks: metrics.max_ticks,
                    width: 700,
                }
            }

            // Gauges row
            div {
                style: "display: grid; grid-template-columns: repeat(2, 1fr); gap: 24px; margin-bottom: 24px;",

                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 24px; text-align: center;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 16px;", "Message Delivery" }
                    HealthGauge {
                        value: delivery_rate,
                        size: 140,
                        warning_threshold: 0.8,
                        danger_threshold: 0.6,
                    }
                }

                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 24px; text-align: center;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 16px;", "Crypto Success" }
                    HealthGauge {
                        value: crypto_success_rate,
                        size: 140,
                        warning_threshold: 0.95,
                        danger_threshold: 0.9,
                    }
                }
            }

            // Combined chart
            div {
                style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Delivery Rate Through Phases" }
                LineChart {
                    data: history.delivery_rate.clone(),
                    width: 700,
                    height: 200,
                    fill: true,
                    y_min: Some(0.0),
                    y_max: Some(1.0),
                }
            }
        }
    }
}

/// Generic metrics panel (fallback)
#[component]
pub fn GenericMetricsPanel(
    metrics: SimMetrics,
    history: MetricsHistory,
) -> Element {
    let delivery_rate = if metrics.messages_sent > 0 {
        metrics.messages_delivered as f64 / metrics.messages_sent as f64
    } else {
        0.0
    };

    rsx! {
        div {
            class: "generic-panel",

            // Stats grid
            div {
                class: "metrics-grid",
                style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 16px; margin-bottom: 24px;",

                if metrics.messages_sent > 0 {
                    div {
                        class: "metric-card",
                        h3 { "Messages Sent" }
                        div { class: "metric-value", "{metrics.messages_sent}" }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Delivered" }
                        div { class: "metric-value", "{metrics.messages_delivered}" }
                    }

                    div {
                        class: "metric-card",
                        h3 { "Delivery Rate" }
                        div { class: "metric-value", "{delivery_rate * 100.0:.1}%" }
                    }
                }

                if metrics.pq_signatures_created > 0 {
                    div {
                        class: "metric-card",
                        h3 { "Signatures" }
                        div { class: "metric-value", "{metrics.pq_signatures_created}" }
                    }
                }

                if metrics.kem_encapsulations > 0 {
                    div {
                        class: "metric-card",
                        h3 { "KEMs" }
                        div { class: "metric-value", "{metrics.kem_encapsulations}" }
                    }
                }

                if metrics.ops_per_second > 0.0 {
                    div {
                        class: "metric-card",
                        h3 { "Throughput" }
                        div { class: "metric-value", "{metrics.ops_per_second:.0}/s" }
                    }
                }
            }

            // Chart if we have history
            if !history.delivery_rate.is_empty() {
                div {
                    style: "background: var(--bg-tertiary); border-radius: 8px; padding: 16px;",
                    h4 { style: "font-size: 0.75rem; color: var(--text-muted); margin-bottom: 12px;", "Metrics Over Time" }
                    LineChart {
                        data: history.delivery_rate.clone(),
                        width: 500,
                        height: 150,
                        fill: true,
                        y_min: Some(0.0),
                    }
                }
            }
        }
    }
}
