//! Activity feed component showing recent events.

use dioxus::prelude::*;

use crate::state::{ActivityEvent, ActivityEventType, AppState};

/// Activity feed showing recent events.
#[component]
pub fn ActivityFeed(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    rsx! {
        section {
            class: "activity-panel",

            div {
                class: "panel-header",
                h2 {
                    class: "panel-title",
                    "Activity"
                }
            }

            div {
                class: "activity-list",

                if state_read.activity_log.is_empty() {
                    div {
                        class: "activity-empty",
                        p { "No activity yet." }
                    }
                } else {
                    for (i, event) in state_read.activity_log.iter().take(15).enumerate() {
                        ActivityItem {
                            key: "{i}",
                            event: event.clone(),
                        }
                    }
                }
            }
        }
    }
}

/// A single activity item.
#[component]
fn ActivityItem(event: ActivityEvent) -> Element {
    let type_class = match event.event_type {
        ActivityEventType::Note => "activity-note",
        ActivityEventType::Quest => "activity-quest",
        ActivityEventType::Artifact => "activity-artifact",
        ActivityEventType::Session => "activity-session",
        ActivityEventType::Sync => "activity-sync",
        ActivityEventType::Info => "activity-info",
    };

    let icon = match event.event_type {
        ActivityEventType::Note => "📝",
        ActivityEventType::Quest => "✓",
        ActivityEventType::Artifact => "📁",
        ActivityEventType::Session => "●",
        ActivityEventType::Sync => "↻",
        ActivityEventType::Info => "i",
    };

    rsx! {
        div {
            class: "activity-item {type_class}",

            span {
                class: "activity-icon",
                "{icon}"
            }

            span {
                class: "activity-description",
                "{event.description}"
            }

            span {
                class: "activity-tick",
                "{event.tick}"
            }
        }
    }
}
