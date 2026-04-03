//! Pass story entry and review screens.
//!
//! Handles three AppStep values: PassStory, StoryReview, RestoreStory.
//! Implements the 23-slot hero's journey form and the review/confirm screen.

use dioxus::prelude::*;
use indras_crypto::{StoryTemplate, entropy};
use indras_crypto::error::CryptoError;

use crate::state::{AppState, AppStep};

/// Per-slot placeholder hints mapping 1:1 to the 23 slots across all 11 stages.
const SLOT_HINTS: [&str; 23] = [
    "a land or place",
    "a name or role",
    "a messenger or force",
    "what they bore",
    "a bond or burden",
    "a shadow or memory",
    "a gate or passage",
    "an unknown realm",
    "a guide or sage",
    "a hidden truth",
    "a craft or weapon",
    "a raw material",
    "another element",
    "something precious",
    "an opposing force",
    "a treasure or relic",
    "a secret or promise",
    "a prize or knowledge",
    "a wilderness or trial",
    "your former nature",
    "your new form",
    "a gift or power",
    "another boon",
];

/// Local state for the pass story flow.
#[derive(Debug, Clone)]
struct PassStoryLocal {
    slots: Vec<String>,
    weak_slots: Vec<usize>,
    error: Option<String>,
}

impl Default for PassStoryLocal {
    fn default() -> Self {
        Self {
            slots: vec![String::new(); 23],
            weak_slots: Vec::new(),
            error: None,
        }
    }
}

/// Pass story entry and review component.
///
/// Routes to fill mode (PassStory/RestoreStory) or review mode (StoryReview)
/// based on the current AppStep.
#[component]
pub fn PassStory(mut state: Signal<AppState>) -> Element {
    let step = state.read().step.clone();

    // Seed local state from AppState if re-entering after review
    let initial = {
        let stored = state.read().pass_story_slots.clone();
        if stored.len() == 23 {
            PassStoryLocal {
                slots: stored,
                weak_slots: Vec::new(),
                error: None,
            }
        } else {
            PassStoryLocal::default()
        }
    };

    let local = use_signal(move || initial);

    match step {
        AppStep::StoryReview => rsx! {
            ReviewScreen { state, local }
        },
        AppStep::PassStory | AppStep::RestoreStory | _ => rsx! {
            FillScreen { state, local }
        },
    }
}

// ── Fill screen ────────────────────────────────────────────────────────────

#[component]
fn FillScreen(mut state: Signal<AppState>, mut local: Signal<PassStoryLocal>) -> Element {
    let step = state.read().step.clone();
    let is_restore = step == AppStep::RestoreStory;

    let template = StoryTemplate::default_template();
    let slots = local.read().slots.clone();
    let weak_slots = local.read().weak_slots.clone();

    let filled = slots.iter().filter(|s| !s.trim().is_empty()).count();
    let total: usize = 23;
    let all_filled = filled == total;

    let (heading, subtitle) = if is_restore {
        (
            "Enter Your Pass Story",
            "Type the same words you used when creating your account.",
        )
    } else {
        (
            "Your Pass Story",
            "Fill each blank with a word from your real life.",
        )
    };

    let back_step = if is_restore {
        AppStep::Welcome
    } else {
        AppStep::DisplayName
    };

    rsx! {
        div {
            class: "story-single-page",

            // Header
            div {
                class: "story-page-header",

                h2 { class: "story-stage-name", "{heading}" }

                p {
                    class: "story-page-subtitle",
                    "{subtitle}"
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
                {render_all_stages(&template, local, &weak_slots)}
            }

            // Hints footer
            div {
                class: "story-page-hints",
                p {
                    class: "story-hint",
                    "Use words that are specific to you — a street name, a nickname, a texture only you'd remember."
                }
                p {
                    class: "story-hint",
                    "Common words like \"love\" or \"home\" are weak. Unusual words like \"cassiterite\" or \"hollyhock\" are strong."
                }
            }

            // Navigation
            div {
                class: "story-nav",

                button {
                    class: "se-btn-back",
                    onclick: move |_| state.write().step = back_step.clone(),
                    "\u{2190} Back"
                }

                button {
                    class: "se-btn-glow",
                    disabled: !all_filled,
                    onclick: move |_| {
                        // Persist slots into AppState and advance to review
                        let slots = local.read().slots.clone();
                        state.write().pass_story_slots = slots;
                        state.write().step = AppStep::StoryReview;
                    },
                    if all_filled {
                        "Continue to Review"
                    } else {
                        "Fill all {total - filled} remaining"
                    }
                }
            }
        }
    }
}

fn render_all_stages(
    template: &StoryTemplate,
    mut local: Signal<PassStoryLocal>,
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
                            span { class: "story-stage-name-small", " {stage_name}" }
                            span { class: "story-stage-desc-small", " — {stage_desc}" }
                        }

                        div {
                            class: "story-template",
                            for (i, part) in parts.iter().enumerate() {
                                span { class: "story-template-text", "{part}" }
                                if i < slot_count {
                                    {
                                        let slot_idx = start_slot + i;
                                        let is_weak = weak_slots.contains(&slot_idx);
                                        let hint = SLOT_HINTS.get(slot_idx).copied().unwrap_or("...");
                                        let class_name = if is_weak {
                                            "story-blank story-blank-weak"
                                        } else {
                                            "story-blank"
                                        };
                                        let current_val = local.read().slots.get(slot_idx).cloned().unwrap_or_default();
                                        rsx! {
                                            input {
                                                class: "{class_name}",
                                                r#type: "text",
                                                value: "{current_val}",
                                                placeholder: "{hint}",
                                                oninput: move |evt| {
                                                    if let Some(slot) = local.write().slots.get_mut(slot_idx) {
                                                        *slot = evt.value();
                                                    }
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

// ── Review screen ──────────────────────────────────────────────────────────

#[component]
fn ReviewScreen(mut state: Signal<AppState>, mut local: Signal<PassStoryLocal>) -> Element {
    let step = state.read().step.clone();
    let is_restore = step == AppStep::RestoreStory;

    let slots = local.read().slots.clone();
    let weak_slots = local.read().weak_slots.clone();
    let error = local.read().error.clone();
    let has_weak = !weak_slots.is_empty();

    let template = StoryTemplate::default_template();

    // Build rendered narrative: (stage_name, Vec<(text, is_slot, is_weak)>)
    let mut stage_narratives: Vec<Vec<(String, bool, bool)>> = Vec::new();
    let mut slot_idx = 0usize;

    for stage in &template.stages {
        let mut words: Vec<(String, bool, bool)> = Vec::new();
        let parts: Vec<&str> = stage.template.split("`_____`").collect();

        for (i, part) in parts.iter().enumerate() {
            if !part.is_empty() {
                words.push((part.to_string(), false, false));
            }
            if i < stage.slot_count && slot_idx < 23 {
                let is_weak = weak_slots.contains(&slot_idx);
                let val = if slots.get(slot_idx).map(|s| s.is_empty()).unwrap_or(true) {
                    "_____".to_string()
                } else {
                    slots[slot_idx].clone()
                };
                words.push((val, true, is_weak));
                slot_idx += 1;
            }
        }

        stage_narratives.push(words);
    }

    let back_step = if is_restore {
        AppStep::RestoreStory
    } else {
        AppStep::PassStory
    };

    rsx! {
        div {
            class: "story-review",

            h2 {
                class: "story-review-title",
                "Review Your Story"
            }

            div {
                class: "story-review-narrative",

                for (stage_idx, words) in stage_narratives.iter().enumerate() {
                    div {
                        key: "{stage_idx}",
                        class: "story-review-stage",

                        p {
                            class: "story-review-paragraph",
                            for (word_idx, (text, is_slot, is_weak)) in words.iter().enumerate() {
                                if *is_slot {
                                    span {
                                        key: "{word_idx}",
                                        class: if *is_weak { "story-slot-weak" } else { "story-slot-filled" },
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

            div {
                class: "story-review-warning-box",
                p {
                    class: "story-review-warning",
                    "This story is your identity. The same words on any device unlock the same vault. Keep it safe — it cannot be recovered."
                }
            }

            if let Some(ref err) = error {
                p {
                    class: "se-error",
                    "{err}"
                }
            }

            div {
                class: "story-nav",

                button {
                    class: "se-btn-back",
                    onclick: move |_| {
                        local.write().error = None;
                        state.write().step = back_step.clone();
                    },
                    "\u{2190} Edit"
                }

                button {
                    class: "se-btn-glow",
                    disabled: has_weak,
                    onclick: move |_| {
                        let slots = local.read().slots.clone();
                        let mut local = local;
                        let mut state = state;

                        spawn(async move {
                            // Run entropy gate
                            let slot_arr: [String; 23] = match slots.try_into() {
                                Ok(a) => a,
                                Err(_) => {
                                    local.write().error = Some("Invalid slot count.".to_string());
                                    return;
                                }
                            };

                            match entropy::entropy_gate(&slot_arr) {
                                Ok(()) => {
                                    // Store slots and advance to Creating/Restoring
                                    state.write().pass_story_slots = slot_arr.to_vec();
                                    let next = if state.read().step == AppStep::StoryReview {
                                        AppStep::Creating
                                    } else {
                                        AppStep::Restoring
                                    };
                                    state.write().step = next;
                                }
                                Err(e) => {
                                    if let CryptoError::EntropyBelowThreshold { weak_slots, .. } = &e {
                                        local.write().weak_slots = weak_slots.clone();
                                    }
                                    local.write().error = Some(
                                        "Story needs stronger words. Edit the highlighted slots.".to_string()
                                    );
                                }
                            }
                        });
                    },
                    "Confirm & Create"
                }
            }
        }
    }
}
