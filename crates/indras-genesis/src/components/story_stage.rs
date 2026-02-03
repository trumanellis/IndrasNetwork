//! Story stage component - all stages on a single scrollable page.

use dioxus::prelude::*;
use indras_crypto::StoryTemplate;

use crate::state::PassStoryState;

/// Per-slot placeholder hints that describe what kind of word to fill in.
/// These map 1:1 to the 23 slots across all 11 stages.
const SLOT_HINTS: [&str; 23] = [
    // Stage 1: The Ordinary World (2 slots)
    "a land or place",           // in the land of ___
    "a name or role",            // known as ___
    // Stage 2: The Call (2 slots)
    "a messenger or force",      // ___ came bearing
    "what they bore",            // bearing ___
    // Stage 3: Refusal (2 slots)
    "a bond or burden",          // bound by my ___
    "a shadow or memory",        // haunted by my ___
    // Stage 4: Crossing the Threshold (2 slots)
    "a gate or passage",         // crossed through the ___
    "an unknown realm",          // into the realm of ___
    // Stage 5: The Mentor (2 slots)
    "a guide or sage",           // a ___ unveiled
    "a hidden truth",            // the hidden ___
    // Stage 6: Tests and Allies (3 slots)
    "a craft or weapon",         // learned to forge ___
    "a raw material",            // from ___
    "another element",           // and ___
    // Stage 7: The Ordeal (2 slots)
    "something precious",        // my ___ was shattered
    "an opposing force",         // shattered against ___
    // Stage 8: The Reward (2 slots)
    "a treasure or relic",       // rose a ___
    "a secret or promise",       // whispered of ___
    // Stage 9: The Road Back (2 slots)
    "a prize or knowledge",      // bore the ___
    "a wilderness or trial",     // through the vast ___
    // Stage 10: Resurrection (2 slots)
    "your former nature",        // had been a ___
    "your new form",             // reborn as a ___
    // Stage 11: Return with the Elixir (2 slots)
    "a gift or power",           // carry ___
    "another boon",              // and ___
];

#[component]
pub fn StoryStage(
    mut story_state: Signal<PassStoryState>,
    on_next: EventHandler<()>,
    on_back: EventHandler<()>,
) -> Element {
    let template = StoryTemplate::default_template();
    let weak_slots = story_state.read().weak_slots.clone();
    let slots = story_state.read().slots.clone();

    // Count filled slots
    let filled = slots.iter().filter(|s| !s.trim().is_empty()).count();
    let total = 23;
    let all_filled = filled == total;

    rsx! {
        div {
            class: "story-single-page",

            // Header
            div {
                class: "story-page-header",
                h2 { class: "story-stage-name", "Your Pass Story" }
                p { class: "story-page-subtitle",
                    "Fill each blank with a word from your real life. Personal, rare words make the strongest keys."
                }
                div {
                    class: "story-page-progress",
                    span { class: "story-progress-count", "{filled}" }
                    span { class: "story-progress-sep", " / " }
                    span { class: "story-progress-total", "{total}" }
                    span { class: "story-progress-label", " slots filled" }
                }
            }

            // All stages in a scrollable container
            div {
                class: "story-stages-scroll",

                {render_all_stages(&template, story_state, &weak_slots)}
            }

            // Hints footer
            div {
                class: "story-page-hints",
                p { class: "story-hint",
                    "Use words that are specific to you — a street name, a nickname, a texture only you'd remember."
                }
                p { class: "story-hint",
                    "Common words like \"love\" or \"home\" are weak. Unusual words like \"cassiterite\" or \"hollyhock\" are strong."
                }
            }

            // Navigation
            div {
                class: "story-nav",

                button {
                    class: "genesis-btn-secondary",
                    onclick: move |_| on_back.call(()),
                    "Cancel"
                }

                button {
                    class: "genesis-btn-primary",
                    disabled: !all_filled,
                    onclick: move |_| on_next.call(()),
                    if all_filled { "Review Story" } else { "Fill all {total - filled} remaining" }
                }
            }
        }
    }
}

fn render_all_stages(
    template: &StoryTemplate,
    mut story_state: Signal<PassStoryState>,
    weak_slots: &[usize],
) -> Element {
    let mut global_slot = 0usize;

    rsx! {
        for (stage_idx, stage) in template.stages.iter().enumerate() {
            {
                let parts: Vec<String> = stage.template
                    .split("`_____`")
                    .map(|s| s.to_string())
                    .collect();
                let slot_count = stage.slot_count;
                let start_slot = global_slot;
                global_slot += slot_count;
                let stage_name = stage.name;
                let stage_desc = stage.description;

                rsx! {
                    div {
                        key: "{stage_idx}",
                        class: "story-stage-section",

                        div {
                            class: "story-stage-label",
                            span { class: "story-stage-number", "{stage_idx + 1}" }
                            span { class: "story-stage-name-small", "{stage_name}" }
                            span { class: "story-stage-desc-small", " — {stage_desc}" }
                        }

                        div {
                            class: "story-template",
                            for (i, part) in parts.iter().enumerate() {
                                span { class: "story-template-text", "{part}" }
                                if i < slot_count {
                                    {
                                        let slot_idx = start_slot + i;
                                        let weak = weak_slots.contains(&slot_idx);
                                        let hint = if slot_idx < SLOT_HINTS.len() {
                                            SLOT_HINTS[slot_idx]
                                        } else {
                                            "..."
                                        };
                                        let class_name = if weak {
                                            "story-blank story-blank-weak"
                                        } else {
                                            "story-blank"
                                        };
                                        rsx! {
                                            input {
                                                class: "{class_name}",
                                                r#type: "text",
                                                value: "{story_state.read().slots[slot_idx]}",
                                                placeholder: "{hint}",
                                                oninput: move |evt| {
                                                    story_state.write().slots[slot_idx] = evt.value();
                                                },
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
