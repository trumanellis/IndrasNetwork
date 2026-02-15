//! Heat visualization components — dots and bars.

use dioxus::prelude::*;

/// Maps a heat float (0.0-1.0) to a level 0-5 for CSS classes.
pub fn heat_level(heat: f32) -> u8 {
    match heat {
        h if h < 0.05 => 0,
        h if h < 0.2 => 1,
        h if h < 0.4 => 2,
        h if h < 0.6 => 3,
        h if h < 0.8 => 4,
        _ => 5,
    }
}

/// Small colored dot indicating heat level (0-5).
#[component]
pub fn HeatDot(level: u8) -> Element {
    let class = format!("heat-dot heat-{}", level.min(5));
    rsx! {
        div { class: "{class}" }
    }
}

/// Labeled heat bar with fill percentage.
#[component]
pub fn HeatBar(
    label: String,
    value: f32,
    color: Option<String>,
) -> Element {
    let pct = (value.clamp(0.0, 1.0) * 100.0) as u32;
    let fill_color = color.unwrap_or_else(|| "var(--accent-gold)".to_string());
    let display_val = format!("{:.0}%", value * 100.0);

    rsx! {
        div {
            class: "heat-bar-row",
            span { class: "heat-bar-label", "{label}" }
            div {
                class: "heat-bar-track",
                div {
                    class: "heat-bar-fill",
                    style: "width: {pct}%; background: {fill_color}",
                }
            }
            span { class: "heat-bar-value", "{display_val}" }
        }
    }
}
