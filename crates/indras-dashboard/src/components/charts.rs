//! Pure SVG chart components for the dashboard
//!
//! These components render directly as SVG elements within Dioxus RSX,
//! using CSS variables for theming and reactive signals for data.

use dioxus::prelude::*;
use crate::state::DataPoint;

/// Line chart component for time-series data
///
/// Renders a line chart with optional area fill.
#[component]
pub fn LineChart(
    /// Data points to render
    data: Vec<DataPoint>,
    /// Chart width in pixels
    #[props(default = 400)]
    width: u32,
    /// Chart height in pixels
    #[props(default = 150)]
    height: u32,
    /// Y-axis minimum value (auto if None)
    y_min: Option<f64>,
    /// Y-axis maximum value (auto if None)
    y_max: Option<f64>,
    /// Whether to fill the area under the line
    #[props(default = false)]
    fill: bool,
    /// CSS color for the line (defaults to accent-primary)
    #[props(default = "var(--accent-primary)".to_string())]
    color: String,
    /// Label for the chart
    #[props(default = "".to_string())]
    label: String,
) -> Element {
    if data.is_empty() {
        return rsx! {
            div {
                class: "chart-container chart-empty",
                style: "width: {width}px; height: {height}px;",
                "No data"
            }
        };
    }

    // Calculate bounds
    let x_min = data.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let x_max = data.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let auto_y_min = data.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let auto_y_max = data.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);

    let y_min = y_min.unwrap_or(auto_y_min.min(0.0));
    let y_max = y_max.unwrap_or(auto_y_max * 1.1).max(y_min + 0.001);
    let x_range = (x_max - x_min).max(1.0);
    let y_range = y_max - y_min;

    // Padding for labels
    let padding_left = 45.0;
    let padding_right = 10.0;
    let padding_top = 20.0;
    let padding_bottom = 25.0;

    let plot_width = width as f64 - padding_left - padding_right;
    let plot_height = height as f64 - padding_top - padding_bottom;

    // Scale functions
    let scale_x = |x: f64| padding_left + ((x - x_min) / x_range) * plot_width;
    let scale_y = |y: f64| padding_top + (1.0 - (y - y_min) / y_range) * plot_height;

    // Build path
    let mut path_d = String::new();
    let mut area_d = String::new();

    for (i, point) in data.iter().enumerate() {
        let x = scale_x(point.x);
        let y = scale_y(point.y);

        if i == 0 {
            path_d.push_str(&format!("M {:.1} {:.1}", x, y));
            if fill {
                area_d.push_str(&format!("M {:.1} {:.1}", x, scale_y(y_min)));
                area_d.push_str(&format!(" L {:.1} {:.1}", x, y));
            }
        } else {
            path_d.push_str(&format!(" L {:.1} {:.1}", x, y));
            if fill {
                area_d.push_str(&format!(" L {:.1} {:.1}", x, y));
            }
        }
    }

    // Close area path
    if fill && !data.is_empty() {
        let last_x = scale_x(data.last().unwrap().x);
        area_d.push_str(&format!(" L {:.1} {:.1}", last_x, scale_y(y_min)));
        area_d.push_str(" Z");
    }

    // Grid lines
    let grid_lines_y = 4;
    let y_grid_step = y_range / grid_lines_y as f64;

    rsx! {
        div {
            class: "chart-container",
            style: "width: {width}px; height: {height}px;",

            // Label
            if !label.is_empty() {
                div {
                    style: "position: absolute; top: 2px; left: {padding_left}px; font-size: 0.75rem; color: var(--text-muted);",
                    "{label}"
                }
            }

            svg {
                width: "{width}",
                height: "{height}",
                view_box: "0 0 {width} {height}",

                // Grid lines
                for i in 0..=grid_lines_y {
                    {
                        let y_val = y_min + (i as f64) * y_grid_step;
                        let y_pos = scale_y(y_val);
                        rsx! {
                            line {
                                x1: "{padding_left}",
                                y1: "{y_pos:.1}",
                                x2: "{width as f64 - padding_right}",
                                y2: "{y_pos:.1}",
                                stroke: "var(--border-color)",
                                stroke_dasharray: "2,2",
                                stroke_width: "1",
                            }
                            text {
                                x: "{padding_left - 5.0}",
                                y: "{y_pos:.1}",
                                text_anchor: "end",
                                dominant_baseline: "middle",
                                font_size: "10",
                                fill: "var(--text-muted)",
                                "{y_val:.0}"
                            }
                        }
                    }
                }

                // Area fill
                if fill && !area_d.is_empty() {
                    path {
                        d: "{area_d}",
                        fill: "{color}",
                        fill_opacity: "0.15",
                    }
                }

                // Line
                if !path_d.is_empty() {
                    path {
                        d: "{path_d}",
                        fill: "none",
                        stroke: "{color}",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                    }
                }

                // X-axis labels (first and last)
                if data.len() >= 2 {
                    text {
                        x: "{scale_x(x_min):.1}",
                        y: "{height as f64 - 5.0}",
                        text_anchor: "start",
                        font_size: "10",
                        fill: "var(--text-muted)",
                        "{x_min:.0}"
                    }
                    text {
                        x: "{scale_x(x_max):.1}",
                        y: "{height as f64 - 5.0}",
                        text_anchor: "end",
                        font_size: "10",
                        fill: "var(--text-muted)",
                        "{x_max:.0}"
                    }
                }
            }
        }
    }
}

/// Health gauge component (semicircle gauge for rates 0-100%)
#[component]
pub fn HealthGauge(
    /// Current value (0.0 - 1.0)
    value: f64,
    /// Size in pixels (diameter)
    #[props(default = 120)]
    size: u32,
    /// Label text
    #[props(default = "".to_string())]
    label: String,
    /// Threshold for warning (0.0 - 1.0)
    #[props(default = 0.7)]
    warning_threshold: f64,
    /// Threshold for danger (0.0 - 1.0)
    #[props(default = 0.5)]
    danger_threshold: f64,
    /// Whether to invert colors (high value = bad)
    #[props(default = false)]
    invert: bool,
) -> Element {
    let clamped_value = value.clamp(0.0, 1.0);
    let percentage = (clamped_value * 100.0).round() as i32;

    // Calculate arc
    let cx = size as f64 / 2.0;
    let cy = size as f64 * 0.6;
    let radius = size as f64 * 0.4;
    let stroke_width = size as f64 * 0.08;

    // Arc goes from -180 to 0 degrees (bottom semicircle)
    let start_angle = std::f64::consts::PI;
    let end_angle = std::f64::consts::PI * (1.0 - clamped_value);

    let start_x = cx + radius * start_angle.cos();
    let start_y = cy + radius * start_angle.sin();
    let end_x = cx + radius * end_angle.cos();
    let end_y = cy + radius * end_angle.sin();

    // Large arc flag
    let large_arc = if clamped_value > 0.5 { 1 } else { 0 };

    let arc_path = format!(
        "M {:.1} {:.1} A {:.1} {:.1} 0 {} 1 {:.1} {:.1}",
        start_x, start_y, radius, radius, large_arc, end_x, end_y
    );

    // Background arc (full semicircle)
    let bg_end_x = cx + radius * 0.0_f64.cos();
    let bg_end_y = cy + radius * 0.0_f64.sin();
    let bg_path = format!(
        "M {:.1} {:.1} A {:.1} {:.1} 0 1 1 {:.1} {:.1}",
        start_x, start_y, radius, radius, bg_end_x, bg_end_y
    );

    // Determine color based on value and thresholds
    let color = if invert {
        if clamped_value >= warning_threshold {
            "var(--accent-error)"
        } else if clamped_value >= danger_threshold {
            "var(--accent-warning)"
        } else {
            "var(--accent-success)"
        }
    } else {
        if clamped_value >= warning_threshold {
            "var(--accent-success)"
        } else if clamped_value >= danger_threshold {
            "var(--accent-warning)"
        } else {
            "var(--accent-error)"
        }
    };

    rsx! {
        div {
            class: "gauge-container",
            style: "width: {size}px; height: {size as f64 * 0.7}px; text-align: center;",

            svg {
                width: "{size}",
                height: "{size as f64 * 0.65}",
                view_box: "0 0 {size} {size as f64 * 0.65}",

                // Background arc
                path {
                    d: "{bg_path}",
                    fill: "none",
                    stroke: "var(--bg-tertiary)",
                    stroke_width: "{stroke_width}",
                    stroke_linecap: "round",
                }

                // Value arc
                if clamped_value > 0.01 {
                    path {
                        d: "{arc_path}",
                        fill: "none",
                        stroke: "{color}",
                        stroke_width: "{stroke_width}",
                        stroke_linecap: "round",
                    }
                }

                // Value text
                text {
                    x: "{cx}",
                    y: "{cy - 5.0}",
                    text_anchor: "middle",
                    dominant_baseline: "middle",
                    font_size: "{size as f64 * 0.2}",
                    font_weight: "bold",
                    fill: "{color}",
                    "{percentage}%"
                }
            }

            // Label
            if !label.is_empty() {
                div {
                    style: "font-size: 0.75rem; color: var(--text-muted); margin-top: -5px;",
                    "{label}"
                }
            }
        }
    }
}

/// Latency bars component for showing percentiles (p50, p95, p99)
#[component]
pub fn LatencyBars(
    /// P50 latency value
    p50: f64,
    /// P95 latency value
    p95: f64,
    /// P99 latency value
    p99: f64,
    /// Unit label (e.g., "us", "ms", "ticks")
    #[props(default = "us".to_string())]
    unit: String,
    /// Maximum value for the scale (auto if 0)
    #[props(default = 0.0)]
    max_value: f64,
    /// Width in pixels
    #[props(default = 200)]
    width: u32,
) -> Element {
    let max_val = if max_value > 0.0 { max_value } else { p99 * 1.2 }.max(1.0);

    let bar_height = 16;
    let label_width = 35;
    let value_width = 60;
    let bar_width = width as i32 - label_width - value_width - 10;

    let scale = |v: f64| -> i32 {
        ((v / max_val) * bar_width as f64).round() as i32
    };

    rsx! {
        div {
            class: "latency-bars",
            style: "width: {width}px;",

            // P50 bar
            div {
                class: "latency-bar-row",
                style: "display: flex; align-items: center; margin-bottom: 4px;",

                span {
                    style: "width: {label_width}px; font-size: 0.75rem; color: var(--text-muted);",
                    "p50"
                }

                div {
                    style: "flex: 1; height: {bar_height}px; background: var(--bg-tertiary); border-radius: 2px; overflow: hidden;",
                    div {
                        style: "width: {scale(p50)}px; height: 100%; background: var(--accent-success); border-radius: 2px;",
                    }
                }

                span {
                    style: "width: {value_width}px; text-align: right; font-size: 0.75rem; color: var(--text-secondary);",
                    "{p50:.0} {unit}"
                }
            }

            // P95 bar
            div {
                class: "latency-bar-row",
                style: "display: flex; align-items: center; margin-bottom: 4px;",

                span {
                    style: "width: {label_width}px; font-size: 0.75rem; color: var(--text-muted);",
                    "p95"
                }

                div {
                    style: "flex: 1; height: {bar_height}px; background: var(--bg-tertiary); border-radius: 2px; overflow: hidden;",
                    div {
                        style: "width: {scale(p95)}px; height: 100%; background: var(--accent-warning); border-radius: 2px;",
                    }
                }

                span {
                    style: "width: {value_width}px; text-align: right; font-size: 0.75rem; color: var(--text-secondary);",
                    "{p95:.0} {unit}"
                }
            }

            // P99 bar
            div {
                class: "latency-bar-row",
                style: "display: flex; align-items: center;",

                span {
                    style: "width: {label_width}px; font-size: 0.75rem; color: var(--text-muted);",
                    "p99"
                }

                div {
                    style: "flex: 1; height: {bar_height}px; background: var(--bg-tertiary); border-radius: 2px; overflow: hidden;",
                    div {
                        style: "width: {scale(p99)}px; height: 100%; background: var(--accent-error); border-radius: 2px;",
                    }
                }

                span {
                    style: "width: {value_width}px; text-align: right; font-size: 0.75rem; color: var(--text-secondary);",
                    "{p99:.0} {unit}"
                }
            }
        }
    }
}

/// Phase timeline component for multi-phase tests
#[component]
pub fn PhaseTimeline(
    /// Phase definitions (name, tick range)
    phases: Vec<(String, u64, u64)>,
    /// Current tick
    current_tick: u64,
    /// Total ticks
    total_ticks: u64,
    /// Width in pixels
    #[props(default = 400)]
    width: u32,
) -> Element {
    if phases.is_empty() || total_ticks == 0 {
        return rsx! {
            div { class: "phase-timeline-empty", "No phases defined" }
        };
    }

    let bar_height = 24;
    let label_height = 20;

    rsx! {
        div {
            class: "phase-timeline",
            style: "width: {width}px;",

            // Phase labels
            div {
                style: "display: flex; margin-bottom: 4px;",

                for (name, start, end) in &phases {
                    {
                        let phase_width = ((*end - *start) as f64 / total_ticks as f64 * width as f64) as u32;
                        let is_current = current_tick >= *start && current_tick < *end;
                        let is_complete = current_tick >= *end;
                        let color = if is_current {
                            "var(--accent-primary)"
                        } else if is_complete {
                            "var(--accent-success)"
                        } else {
                            "var(--text-muted)"
                        };

                        rsx! {
                            div {
                                style: "width: {phase_width}px; text-align: center; font-size: 0.7rem; color: {color};",
                                "{name}"
                            }
                        }
                    }
                }
            }

            // Progress bar
            svg {
                width: "{width}",
                height: "{bar_height}",

                // Background
                rect {
                    x: "0",
                    y: "0",
                    width: "{width}",
                    height: "{bar_height}",
                    fill: "var(--bg-tertiary)",
                    rx: "4",
                }

                // Phase sections
                for (i, (_name, start, end)) in phases.iter().enumerate() {
                    {
                        let x = (*start as f64 / total_ticks as f64 * width as f64) as u32;
                        let phase_width = ((*end - *start) as f64 / total_ticks as f64 * width as f64) as u32;
                        let is_complete = current_tick >= *end;
                        let is_current = current_tick >= *start && current_tick < *end;

                        let color = if is_complete {
                            "var(--accent-success)"
                        } else if is_current {
                            "var(--accent-primary)"
                        } else {
                            "var(--bg-hover)"
                        };

                        // Calculate fill width for current phase
                        let fill_width = if is_complete {
                            phase_width
                        } else if is_current {
                            let progress = (current_tick - *start) as f64 / (*end - *start) as f64;
                            (progress * phase_width as f64) as u32
                        } else {
                            0
                        };

                        rsx! {
                            // Phase background
                            rect {
                                x: "{x}",
                                y: "2",
                                width: "{phase_width}",
                                height: "{bar_height - 4}",
                                fill: "var(--bg-hover)",
                                rx: "2",
                            }

                            // Phase progress
                            if fill_width > 0 {
                                rect {
                                    x: "{x}",
                                    y: "2",
                                    width: "{fill_width}",
                                    height: "{bar_height - 4}",
                                    fill: "{color}",
                                    rx: "2",
                                }
                            }

                            // Phase divider (except for last)
                            if i < phases.len() - 1 {
                                line {
                                    x1: "{x + phase_width}",
                                    y1: "0",
                                    x2: "{x + phase_width}",
                                    y2: "{bar_height}",
                                    stroke: "var(--bg-primary)",
                                    stroke_width: "2",
                                }
                            }
                        }
                    }
                }

                // Current position marker
                {
                    let marker_x = (current_tick as f64 / total_ticks as f64 * width as f64) as u32;
                    rsx! {
                        line {
                            x1: "{marker_x}",
                            y1: "0",
                            x2: "{marker_x}",
                            y2: "{bar_height}",
                            stroke: "var(--accent-primary)",
                            stroke_width: "2",
                        }
                    }
                }
            }

            // Tick labels
            div {
                style: "display: flex; justify-content: space-between; margin-top: 4px; font-size: 0.65rem; color: var(--text-muted);",
                span { "0" }
                span { "{current_tick}/{total_ticks}" }
            }
        }
    }
}

/// Mini sparkline for inline display
#[component]
pub fn Sparkline(
    /// Data values
    data: Vec<f64>,
    /// Width in pixels
    #[props(default = 80)]
    width: u32,
    /// Height in pixels
    #[props(default = 24)]
    height: u32,
    /// Line color
    #[props(default = "var(--accent-primary)".to_string())]
    color: String,
) -> Element {
    if data.is_empty() {
        return rsx! { span { class: "sparkline-empty" } };
    }

    let min_val = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = (max_val - min_val).max(0.001);

    let padding = 2.0;
    let plot_width = width as f64 - 2.0 * padding;
    let plot_height = height as f64 - 2.0 * padding;

    let mut path_d = String::new();

    for (i, &value) in data.iter().enumerate() {
        let x = padding + (i as f64 / (data.len() - 1).max(1) as f64) * plot_width;
        let y = padding + (1.0 - (value - min_val) / range) * plot_height;

        if i == 0 {
            path_d.push_str(&format!("M {:.1} {:.1}", x, y));
        } else {
            path_d.push_str(&format!(" L {:.1} {:.1}", x, y));
        }
    }

    rsx! {
        svg {
            class: "sparkline",
            width: "{width}",
            height: "{height}",
            view_box: "0 0 {width} {height}",

            path {
                d: "{path_d}",
                fill: "none",
                stroke: "{color}",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}
