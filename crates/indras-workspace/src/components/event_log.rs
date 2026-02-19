//! Network activity event log component (chat-style, newest at bottom).

use dioxus::prelude::*;
use crate::state::workspace::{EventLogEntry, EventDirection};

#[component]
pub fn EventLogView(event_log: Vec<EventLogEntry>) -> Element {
    let entry_count = event_log.len();

    // Auto-scroll to bottom when new entries arrive
    use_effect(move || {
        let _count = entry_count;
        document::eval(
            "var el = document.getElementById('event-log-scroll'); if(el) el.scrollTop = el.scrollHeight;"
        );
    });

    rsx! {
        div { class: "view active event-log-view",
            div { class: "event-log-header",
                div { class: "event-log-title", "Network Activity" }
                div { class: "event-log-subtitle", "P2P events and system messages" }
            }
            div { class: "event-log-scroll", id: "event-log-scroll",
                if event_log.is_empty() {
                    div { class: "event-log-empty",
                        div { class: "event-log-empty-icon", "\u{25C9}" }
                        div { "No network activity yet" }
                        div { class: "event-log-hint", "Events will appear here as you interact with the network" }
                    }
                } else {
                    div { class: "event-log-content",
                        // Render oldest-first (vec is newest-first, so reverse)
                        for entry in event_log.iter().rev() {
                            {
                                let (arrow, arrow_class) = match entry.direction {
                                    EventDirection::Sent => ("\u{2192}", "sent"),
                                    EventDirection::Received => ("\u{2190}", "received"),
                                    EventDirection::System => ("\u{00B7}", "system"),
                                };
                                rsx! {
                                    {
                                        let entry_class = if entry.highlighted { "event-log-entry highlighted" } else { "event-log-entry" };
                                        rsx! {
                                            div { class: "{entry_class}",
                                                span { class: "event-log-time", "{entry.timestamp}" }
                                                span { class: "event-log-arrow {arrow_class}", "{arrow}" }
                                                span { class: "event-log-msg", "{entry.message}" }
                                                if let Some(action) = &entry.action_label {
                                                    button {
                                                        class: "event-log-action",
                                                        "{action}"
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
            }
        }
    }
}
