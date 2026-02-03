//! Story review component - shows completed narrative for confirmation.

use dioxus::prelude::*;
use indras_crypto::StoryTemplate;

use crate::state::{AsyncStatus, PassStoryState};

#[component]
pub fn StoryReview(
    mut story_state: Signal<PassStoryState>,
    on_confirm: EventHandler<()>,
    on_edit_stage: EventHandler<usize>,
) -> Element {
    let template = StoryTemplate::default_template();
    let slots = story_state.read().slots.clone();
    let weak_slots = story_state.read().weak_slots.clone();
    let status = story_state.read().status.clone();
    let has_weak = !weak_slots.is_empty();
    let is_loading = matches!(status, AsyncStatus::Loading);

    // Build rendered narrative
    let mut slot_idx = 0usize;
    let mut stage_narratives: Vec<(String, Vec<(String, bool)>)> = Vec::new();

    for stage in &template.stages {
        let mut words: Vec<(String, bool)> = Vec::new();
        let parts: Vec<&str> = stage.template.split("`_____`").collect();

        for (i, part) in parts.iter().enumerate() {
            if !part.is_empty() {
                words.push((part.to_string(), false));
            }
            if i < stage.slot_count && slot_idx < 23 {
                let is_weak = weak_slots.contains(&slot_idx);
                let val = if slots[slot_idx].is_empty() {
                    "_____".to_string()
                } else {
                    slots[slot_idx].clone()
                };
                words.push((val, is_weak));
                slot_idx += 1;
            }
        }

        stage_narratives.push((stage.name.to_string(), words));
    }

    rsx! {
        div {
            class: "story-review",

            h2 {
                class: "story-review-title",
                "Your Story"
            }

            div {
                class: "story-review-narrative",

                for (stage_idx, (_stage_name, words)) in stage_narratives.iter().enumerate() {
                    div {
                        key: "{stage_idx}",
                        class: "story-review-stage",

                        p {
                            class: "story-review-paragraph",
                            for (word_idx, (text, is_weak)) in words.iter().enumerate() {
                                if *is_weak {
                                    span {
                                        key: "{word_idx}",
                                        class: "story-highlight",
                                        onclick: move |_| on_edit_stage.call(stage_idx),
                                        "{text}"
                                    }
                                } else {
                                    span {
                                        key: "{word_idx}",
                                        "{text}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if has_weak {
                div {
                    class: "story-review-warning",
                    p { "This doesn't sound like a story only you would tell." }
                    p { class: "story-review-warning-hint", "Click highlighted words to strengthen them." }
                }
            } else {
                div {
                    class: "story-review-success",
                    p { "Your story is uniquely yours." }
                }
            }

            if let AsyncStatus::Error(ref e) = status {
                p {
                    class: "genesis-error",
                    "{e}"
                }
            }

            div {
                class: "story-nav",

                button {
                    class: "genesis-btn-secondary",
                    onclick: move |_| {
                        // Go back to the single-page form
                        story_state.write().current_stage = 0;
                    },
                    "Edit Story"
                }

                if !has_weak {
                    button {
                        class: "genesis-btn-primary",
                        disabled: is_loading,
                        onclick: move |_| on_confirm.call(()),
                        if is_loading {
                            "Protecting identity..."
                        } else {
                            "Confirm & Protect Identity"
                        }
                    }
                }
            }
        }
    }
}
