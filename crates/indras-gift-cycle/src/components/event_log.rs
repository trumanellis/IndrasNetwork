//! Fixed footer panel showing real-time P2P events.

use dioxus::prelude::*;

use crate::data::{time_ago, P2pLogEntry};

/// Fixed footer panel displaying P2P event log entries.
#[component]
pub fn EventLogPanel(entries: Vec<P2pLogEntry>) -> Element {
    rsx! {
        div { class: "event-log",
            div { class: "event-log-header",
                span { "P2P Log" }
                span { "{entries.len()} events" }
            }
            for entry in entries.iter().rev() {
                div { class: "event-log-entry",
                    span { class: "event-log-time", "{time_ago(entry.timestamp as i64)}" }
                    span { "{entry.message}" }
                }
            }
        }
    }
}
