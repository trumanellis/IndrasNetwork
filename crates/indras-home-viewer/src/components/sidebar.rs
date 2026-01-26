//! Sidebar component with session info.

use dioxus::prelude::*;

use crate::state::{short_id, AppState};

/// Sidebar component showing session information.
#[component]
pub fn Sidebar(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    rsx! {
        aside {
            class: "sidebar",

            // Session info section
            section {
                class: "sidebar-section",

                h2 {
                    class: "sidebar-section-title",
                    "Session"
                }

                div {
                    class: "session-info",

                    if let Some(realm_id) = &state_read.session.realm_id {
                        div {
                            class: "session-item",
                            span { class: "session-label", "Realm" }
                            span { class: "session-value session-realm", "{short_id(realm_id)}" }
                        }
                    }

                    div {
                        class: "session-item",
                        span { class: "session-label", "Status" }
                        span {
                            class: "session-value session-status",
                            "{state_read.session.status_display()}"
                        }
                    }

                    if state_read.session.devices_synced > 0 {
                        div {
                            class: "session-item",
                            span { class: "session-label", "Devices" }
                            span { class: "session-value", "{state_read.session.devices_synced}" }
                        }
                    }
                }
            }

            // Quick summary section
            section {
                class: "sidebar-section",

                h2 {
                    class: "sidebar-section-title",
                    "Summary"
                }

                div {
                    class: "session-info",

                    div {
                        class: "session-item",
                        span { class: "session-label", "Notes" }
                        span { class: "session-value", "{state_read.notes.active_count()}" }
                    }

                    div {
                        class: "session-item",
                        span { class: "session-label", "Quests" }
                        span { class: "session-value", "{state_read.quests.active_count()}" }
                    }

                    div {
                        class: "session-item",
                        span { class: "session-label", "Files" }
                        span { class: "session-value", "{state_read.artifacts.count()}" }
                    }

                    div {
                        class: "session-item",
                        span { class: "session-label", "Storage" }
                        span { class: "session-value", "{state_read.artifacts.total_size_display()}" }
                    }
                }
            }
        }
    }
}
