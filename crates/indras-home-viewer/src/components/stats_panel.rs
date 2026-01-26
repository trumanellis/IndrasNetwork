//! Stats panel component showing quick statistics.

use dioxus::prelude::*;

use crate::state::AppState;

/// Stats panel showing quick statistics.
#[component]
pub fn StatsPanel(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    let notes_count = state_read.notes.active_count();
    let quests_active = state_read.quests.active_count();
    let quests_completed = state_read.quests.completed_count();
    let artifacts_count = state_read.artifacts.count();
    let total_size = state_read.artifacts.total_size();

    rsx! {
        section {
            class: "stats-panel",

            div {
                class: "panel-header",
                h2 {
                    class: "panel-title",
                    "Stats"
                }
            }

            div {
                class: "stats-grid",

                // Notes stat
                StatItem {
                    label: "Notes".to_string(),
                    value: notes_count.to_string(),
                    accent: "gold".to_string(),
                }

                // Active quests stat
                StatItem {
                    label: "Active Quests".to_string(),
                    value: quests_active.to_string(),
                    accent: "cyan".to_string(),
                }

                // Completed quests stat
                StatItem {
                    label: "Completed".to_string(),
                    value: quests_completed.to_string(),
                    accent: "moss".to_string(),
                }

                // Files stat
                StatItem {
                    label: "Files".to_string(),
                    value: artifacts_count.to_string(),
                    accent: "cyan".to_string(),
                }

                // Storage stat
                StatItem {
                    label: "Storage".to_string(),
                    value: format_size(total_size),
                    accent: "gold".to_string(),
                }
            }
        }
    }
}

/// A single stat item.
#[component]
fn StatItem(label: String, value: String, accent: String) -> Element {
    rsx! {
        div {
            class: "stat-item stat-{accent}",

            span {
                class: "stat-value",
                "{value}"
            }

            span {
                class: "stat-label",
                "{label}"
            }
        }
    }
}

/// Formats a byte size as a human-readable string.
fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        "0 B".to_string()
    } else if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
