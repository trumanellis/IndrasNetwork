//! Pass story entry and review screens (restore/sign-in flow).
//!
//! Handles RestoreStory and StoryReview AppStep values.
//! Implements the 23-slot hero's journey form and the review/confirm screen.

use dioxus::prelude::*;
use indras_crypto::{StoryTemplate, entropy};
use indras_crypto::error::CryptoError;

use crate::state::{AppState, AppStep};

/// Per-slot placeholder hints mapping 1:1 to the 23 slots across all 11 stages.
const SLOT_HINTS: [&str; 23] = [
    "a land or place you knew",
    "a name you were called",
    "a messenger or force",
    "what they carried",
    "a bond that held you",
    "a shadow that followed",
    "a gate or passage",
    "an unknown realm",
    "a guide who appeared",
    "a hidden truth",
    "something you forged",
    "a raw material",
    "another element",
    "something precious, broken",
    "an opposing force",
    "what rose from silence",
    "what it whispered of",
    "what you carried home",
    "a vast wilderness",
    "your former self",
    "who you became",
    "a gift you keep",
    "another boon you hold",
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

/// Pass story entry and review component (restore flow only).
///
/// Routes to fill mode (RestoreStory) or review mode (StoryReview).
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
        _ => rsx! {
            FillScreen { state, local }
        },
    }
}

// ── Fill screen ────────────────────────────────────────────────────────────

#[component]
fn FillScreen(mut state: Signal<AppState>, mut local: Signal<PassStoryLocal>) -> Element {
    let template = StoryTemplate::default_template();
    let slots = local.read().slots.clone();
    let weak_slots = local.read().weak_slots.clone();

    let filled = slots.iter().filter(|s| !s.trim().is_empty()).count();
    let total: usize = 23;
    let all_filled = filled == total;
    let progress_pct = (filled as f64 / total as f64) * 100.0;

    rsx! {
        div {
            class: "story-single-page",

            // Header
            div {
                class: "story-page-header",

                h2 { class: "story-manuscript-title", "Enter Your Pass Story" }

                p {
                    class: "story-page-subtitle",
                    "Type the same words you used when creating your account."
                }

                div {
                    class: "story-progress-bar",
                    div {
                        class: "story-progress-fill",
                        width: "{progress_pct:.0}%",
                    }
                }
            }

            // Flowing manuscript
            div {
                class: "story-manuscript",
                {render_all_stages(&template, local, &weak_slots)}
            }

            // Footnote
            p {
                class: "story-manuscript-footnote",
                "Unusual words are strongest \u{2014} a street name, a texture, a word only you would think to use."
            }

            // Navigation
            div {
                class: "story-nav",

                button {
                    class: "se-btn-back",
                    onclick: move |_| state.write().step = AppStep::Welcome,
                    "\u{2190} Back"
                }

                button {
                    class: "se-btn-glow",
                    disabled: !all_filled,
                    onclick: move |_| {
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

                rsx! {
                    div {
                        key: "{stage_idx}",
                        class: "story-manuscript-stage",

                        span {
                            class: "story-stage-annotation",
                            "{stage_name}"
                        }

                        p {
                            class: "story-manuscript-paragraph",
                            for (i, part) in parts.iter().enumerate() {
                                span { class: "story-prose", "{part}" }
                                if i < slot_count {
                                    {
                                        let slot_idx = start_slot + i;
                                        let is_weak = weak_slots.contains(&slot_idx);
                                        let hint = SLOT_HINTS.get(slot_idx).copied().unwrap_or("...");
                                        let class_name = if is_weak {
                                            "story-manuscript-blank story-manuscript-blank-weak"
                                        } else {
                                            "story-manuscript-blank"
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
    let slots = local.read().slots.clone();
    let weak_slots = local.read().weak_slots.clone();
    let error = local.read().error.clone();
    let has_weak = !weak_slots.is_empty();

    let template = StoryTemplate::default_template();

    // Build rendered narrative: Vec<(text, is_slot, is_weak)> per stage
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

    rsx! {
        div {
            class: "story-review-manuscript",

            p {
                class: "story-review-epigraph",
                "Read your story once more. If it sounds like you, it is ready."
            }

            div {
                class: "story-review-prose",

                for (stage_idx, words) in stage_narratives.iter().enumerate() {
                    p {
                        key: "{stage_idx}",
                        class: "story-review-paragraph",
                        for (word_idx, (text, is_slot, is_weak)) in words.iter().enumerate() {
                            if *is_slot {
                                span {
                                    key: "{word_idx}",
                                    class: if *is_weak { "story-word-weak" } else { "story-word-luminous" },
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

            div {
                class: "story-review-colophon",
                "These words are the key to who you are. Spoken on any device, they will open the same door. Guard them as you would a secret name."
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
                        state.write().step = AppStep::RestoreStory;
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
                                    state.write().pass_story_slots = slot_arr.to_vec();
                                    state.write().step = AppStep::Restoring;
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
                    "Seal This Story"
                }
            }
        }
    }
}
