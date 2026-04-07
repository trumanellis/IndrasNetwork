//! Intentions panel component showing personal tasks.

use dioxus::prelude::*;

use crate::state::{AppState, HomeIntention, IntentionStatus};

/// Intentions panel showing personal tasks.
#[component]
pub fn QuestsPanel(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    rsx! {
        section {
            class: "quests-panel",

            div {
                class: "panel-header",
                h2 {
                    class: "panel-title",
                    "Intentions"
                }
                span {
                    class: "panel-count",
                    "{state_read.intentions.active_count()} active"
                }
            }

            div {
                class: "quests-list",

                if state_read.intentions.intentions.is_empty() {
                    div {
                        class: "quests-empty",
                        p { "No intentions yet." }
                    }
                } else {
                    // Active intentions first
                    for intention in state_read.intentions.active_intentions().iter().take(5) {
                        IntentionItem {
                            key: "{intention.id}",
                            intention: (*intention).clone(),
                        }
                    }

                    // Then completed intentions
                    CompletedIntentionsSection { state }
                }
            }
        }
    }
}

/// Completed intentions section.
#[component]
fn CompletedIntentionsSection(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let completed: Vec<_> = state_read
        .intentions
        .intentions_by_recency()
        .iter()
        .filter(|i| i.status == IntentionStatus::Completed)
        .take(3)
        .cloned()
        .collect();

    if completed.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "quests-divider",
            "Completed"
        }
        for intention in completed {
            IntentionItem {
                key: "{intention.id}",
                intention: intention.clone(),
            }
        }
    }
}

/// A single intention item.
#[component]
fn IntentionItem(intention: HomeIntention) -> Element {
    let is_completed = intention.status == IntentionStatus::Completed;

    rsx! {
        div {
            class: if is_completed { "quest-item quest-item-completed" } else { "quest-item" },

            // Checkbox indicator
            span {
                class: if is_completed { "quest-checkbox quest-checkbox-checked" } else { "quest-checkbox" },
                if is_completed { "✓" } else { "" }
            }

            // Intention title
            span {
                class: "quest-title",
                if is_completed {
                    del { "{intention.title}" }
                } else {
                    "{intention.title}"
                }
            }
        }
    }
}
