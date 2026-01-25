//! Unified Control Bar component
//!
//! A Spotify-style fixed bottom bar for simulation/test controls.
//! Uses CSS variables from themes.css for consistent theming.

use crate::state::UnifiedPlaybackState;
use dioxus::prelude::*;

/// Unified control bar that appears at the bottom of the screen
#[component]
pub fn UnifiedControlBar(
    playback_state: UnifiedPlaybackState,
    on_step: EventHandler<()>,
    on_play_pause: EventHandler<()>,
    on_reset: EventHandler<()>,
    on_speed_change: EventHandler<f64>,
    on_level_change: EventHandler<String>,
) -> Element {
    let progress = playback_state.progress_percent();
    let status = playback_state.status_text();

    // Compute disabled states
    let step_disabled = !playback_state.can_step;
    let play_disabled = !playback_state.can_play;
    let reset_disabled = !playback_state.can_reset;

    rsx! {
        div {
            class: "unified-control-bar",

            // Progress bar across the top
            div { class: "control-bar-progress",
                div {
                    class: "control-bar-progress-fill",
                    style: "width: {progress}%;",
                }
            }

            // Main control bar content
            div { class: "control-bar-content",

                // Left: Context info + Stress level
                div { class: "control-bar-context",
                    span { class: "context-icon", "{playback_state.context.icon()}" }
                    div { class: "context-info",
                        span { class: "context-name", "{playback_state.context_name}" }
                        span { class: "context-type", "{playback_state.context.display_name()}" }
                    }

                    // Stress level selector (between name and transport)
                    if playback_state.has_stress_control {
                        div { class: "stress-selector",
                            button {
                                class: if playback_state.stress_level == "quick" { "stress-btn active" } else { "stress-btn" },
                                disabled: playback_state.is_running,
                                onclick: move |_| on_level_change.call("quick".to_string()),
                                "Quick"
                            }
                            button {
                                class: if playback_state.stress_level == "medium" { "stress-btn active" } else { "stress-btn" },
                                disabled: playback_state.is_running,
                                onclick: move |_| on_level_change.call("medium".to_string()),
                                "Medium"
                            }
                            button {
                                class: if playback_state.stress_level == "full" { "stress-btn active" } else { "stress-btn" },
                                disabled: playback_state.is_running,
                                onclick: move |_| on_level_change.call("full".to_string()),
                                "Full"
                            }
                        }
                    }
                }

                // Center: Transport controls
                div { class: "control-bar-transport",

                    // Step button
                    button {
                        class: if step_disabled { "transport-btn secondary disabled" } else { "transport-btn secondary" },
                        disabled: step_disabled,
                        onclick: move |_| on_step.call(()),
                        title: "Step forward one tick",
                        span { class: "btn-icon", "▶▶" }
                        span { class: "btn-label", "Step" }
                    }

                    // Play/Pause button (primary)
                    button {
                        class: if play_disabled {
                            "transport-btn primary disabled"
                        } else if playback_state.is_running {
                            "transport-btn primary running"
                        } else {
                            "transport-btn primary"
                        },
                        disabled: play_disabled,
                        onclick: move |_| on_play_pause.call(()),
                        title: if playback_state.is_running { "Stop" } else { "Run" },
                        if playback_state.is_running { "■" } else { "▶" }
                    }

                    // Reset button
                    button {
                        class: if reset_disabled { "transport-btn secondary disabled" } else { "transport-btn secondary" },
                        disabled: reset_disabled,
                        onclick: move |_| on_reset.call(()),
                        title: "Reset to initial state",
                        span { class: "btn-icon", "↺" }
                        span { class: "btn-label", "Reset" }
                    }
                }

                // Right: Status and speed control
                div { class: "control-bar-status",

                    // Tick/Progress display
                    div { class: "tick-display",
                        span { class: "tick-current", "{playback_state.current_tick}" }
                        span { class: "tick-separator", "/" }
                        span { class: "tick-max", "{playback_state.max_ticks}" }
                    }

                    // Status badge
                    span {
                        class: format!("status-badge status-{}", status.to_lowercase()),
                        "{status}"
                    }

                    // Speed control (only for tabs that support it)
                    if playback_state.has_speed_control {
                        div { class: "speed-control",
                            label { class: "speed-label", "{playback_state.playback_speed:.1}×" }
                            input {
                                r#type: "range",
                                class: "speed-slider",
                                min: "0.5",
                                max: "10",
                                step: "0.5",
                                value: "{playback_state.playback_speed}",
                                onchange: move |e| {
                                    if let Ok(speed) = e.value().parse::<f64>() {
                                        on_speed_change.call(speed);
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
