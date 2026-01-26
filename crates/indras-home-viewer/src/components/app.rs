//! Root application component for the home realm viewer.

use dioxus::prelude::*;

use crate::playback;
use crate::state::AppState;
use crate::theme::ThemedRoot;

use super::{ActivityFeed, ArtifactsGallery, NotesPanel, QuestsPanel, Sidebar};

/// Root application component.
#[component]
pub fn App(state: Signal<AppState>) -> Element {
    rsx! {
        ThemedRoot {
            div {
                class: "home-viewer",

                // Header
                Header { state }

                // Main content area - 3 panel layout
                main {
                    class: "main-content",

                    // Left panel - Sidebar with notes/files lists
                    Sidebar { state }

                    // Center panel - Main content area
                    div {
                        class: "center-panel",

                        // Notes section
                        NotesPanel { state }

                        // Artifacts gallery
                        ArtifactsGallery { state }
                    }

                    // Right panel - Quests and activity
                    div {
                        class: "right-panel",

                        QuestsPanel { state }
                        ActivityFeed { state }
                    }
                }

                // Floating playback controls
                PlaybackControls { state }
            }
        }
    }
}

/// Header component with title and status indicators.
#[component]
fn Header(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let member = state_read
        .selected_member
        .as_deref()
        .unwrap_or("Unknown");
    let session_active = state_read.is_session_active();
    let sync_healthy = state_read.is_sync_healthy();

    rsx! {
        header {
            class: "header",

            div {
                class: "header-left",
                h1 {
                    class: "header-title",
                    "My Home Realm"
                }
                span {
                    class: "header-member",
                    "{member}"
                }
            }

            div {
                class: "header-right",

                // Session status
                div {
                    class: if session_active { "status-indicator status-active" } else { "status-indicator status-inactive" },
                    span {
                        class: "status-dot",
                    }
                    span {
                        class: "status-text",
                        "Session"
                    }
                }

                // Sync status
                div {
                    class: if sync_healthy { "status-indicator status-synced" } else { "status-indicator status-conflict" },
                    span {
                        class: "status-icon",
                        if sync_healthy { "✓" } else { "!" }
                    }
                    span {
                        class: "status-text",
                        "Sync"
                    }
                }
            }
        }
    }
}

/// Floating playback controls.
#[component]
fn PlaybackControls(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let state_read = state.read();

    let is_paused = state_read.playback.paused;
    let speed = state_read.playback.speed;
    let tick = state_read.tick;
    let total_events = state_read.total_events;

    rsx! {
        div {
            class: "playback-controls",

            // Reset button
            button {
                class: "control-btn",
                onclick: move |_| {
                    playback::request_reset();
                    state_write.write().reset();
                },
                "⟲"
            }

            // Play/Pause button
            button {
                class: "control-btn control-btn-primary",
                onclick: move |_| {
                    let new_paused = playback::toggle_paused();
                    state_write.write().playback.paused = new_paused;
                },
                if is_paused { "▶" } else { "⏸" }
            }

            // Step button
            button {
                class: "control-btn",
                onclick: move |_| {
                    playback::request_step();
                },
                "⏭"
            }

            // Speed control
            div {
                class: "speed-control",
                label {
                    class: "speed-label",
                    "Speed:"
                }
                select {
                    class: "speed-select",
                    value: "{speed}",
                    onchange: move |evt| {
                        if let Ok(new_speed) = evt.value().parse::<f32>() {
                            playback::set_speed(new_speed);
                            state_write.write().playback.speed = new_speed;
                        }
                    },
                    option { value: "0.5", "0.5x" }
                    option { value: "1", "1x" }
                    option { value: "2", "2x" }
                    option { value: "5", "5x" }
                    option { value: "10", "10x" }
                }
            }

            // Tick display
            div {
                class: "tick-display",
                span {
                    class: "tick-label",
                    "Tick:"
                }
                span {
                    class: "tick-value",
                    "{tick}"
                }
            }

            // Events count
            div {
                class: "events-display",
                span {
                    class: "events-label",
                    "Events:"
                }
                span {
                    class: "events-value",
                    "{total_events}"
                }
            }
        }
    }
}
