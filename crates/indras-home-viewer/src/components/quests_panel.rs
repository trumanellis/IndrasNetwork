//! Quests panel component showing personal tasks.

use dioxus::prelude::*;

use crate::state::{AppState, HomeQuest, QuestStatus};

/// Quests panel showing personal tasks.
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
                    "Quests"
                }
                span {
                    class: "panel-count",
                    "{state_read.quests.active_count()} active"
                }
            }

            div {
                class: "quests-list",

                if state_read.quests.quests.is_empty() {
                    div {
                        class: "quests-empty",
                        p { "No quests yet." }
                    }
                } else {
                    // Active quests first
                    for quest in state_read.quests.active_quests().iter().take(5) {
                        QuestItem {
                            key: "{quest.id}",
                            quest: (*quest).clone(),
                        }
                    }

                    // Then completed quests
                    CompletedQuestsSection { state }
                }
            }
        }
    }
}

/// Completed quests section.
#[component]
fn CompletedQuestsSection(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let completed: Vec<_> = state_read
        .quests
        .quests_by_recency()
        .iter()
        .filter(|q| q.status == QuestStatus::Completed)
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
        for quest in completed {
            QuestItem {
                key: "{quest.id}",
                quest: quest.clone(),
            }
        }
    }
}

/// A single quest item.
#[component]
fn QuestItem(quest: HomeQuest) -> Element {
    let is_completed = quest.status == QuestStatus::Completed;

    rsx! {
        div {
            class: if is_completed { "quest-item quest-item-completed" } else { "quest-item" },

            // Checkbox indicator
            span {
                class: if is_completed { "quest-checkbox quest-checkbox-checked" } else { "quest-checkbox" },
                if is_completed { "✓" } else { "" }
            }

            // Quest title
            span {
                class: "quest-title",
                if is_completed {
                    del { "{quest.title}" }
                } else {
                    "{quest.title}"
                }
            }
        }
    }
}
