//! PassStory overlay — 23-slot hero's journey form for key protection.

use dioxus::prelude::*;
use indras_crypto::StoryTemplate;

/// Phase of the PassStory flow.
#[derive(Clone, Debug, PartialEq)]
enum PassStoryPhase {
    Fill,
    Review,
    Done,
}

/// PassStory overlay for protecting identity with a hero's journey narrative.
#[component]
pub fn PassStoryOverlay(
    visible: bool,
    on_close: EventHandler<()>,
    on_protect: EventHandler<[String; 23]>,
) -> Element {
    let template = StoryTemplate::default_template();
    let mut slots: Signal<Vec<String>> = use_signal(|| vec![String::new(); 23]);
    let mut phase = use_signal(|| PassStoryPhase::Fill);
    let mut weak_slots = use_signal(Vec::<usize>::new);
    let mut error_msg = use_signal(|| None::<String>);

    if !visible {
        return rsx! {};
    }

    let current_phase = phase.read().clone();
    let boundaries = template.stage_boundaries();

    rsx! {
        div {
            class: "overlay pass-story-overlay",
            onclick: move |_| on_close.call(()),

            div {
                class: "pass-story-modal",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "pass-story-header",
                    div { class: "pass-story-title",
                        match current_phase {
                            PassStoryPhase::Fill => "Protect Your Identity",
                            PassStoryPhase::Review => "Review Your Story",
                            PassStoryPhase::Done => "Identity Protected",
                        }
                    }
                    button {
                        class: "pass-story-close",
                        onclick: move |_| on_close.call(()),
                        "\u{2715}"
                    }
                }

                div { class: "pass-story-body",
                    match current_phase {
                        PassStoryPhase::Fill => rsx! {
                            div { class: "pass-story-instructions",
                                "Fill in each blank with a memorable word or phrase. "
                                "This story becomes the key that protects your identity. "
                                "Choose words that are meaningful to you but hard for others to guess."
                            }

                            for (stage_idx, stage) in template.stages.iter().enumerate() {
                                {
                                    let (start, end) = boundaries[stage_idx];
                                    let weak_list = weak_slots.read().clone();
                                    rsx! {
                                        div { class: "pass-story-stage",
                                            div { class: "pass-story-stage-name", "{stage.name}" }
                                            div { class: "pass-story-stage-desc", "{stage.description}" }
                                            div { class: "pass-story-stage-template", "{stage.template}" }

                                            for slot_idx in start..end {
                                                {
                                                    let is_weak = weak_list.contains(&slot_idx);
                                                    let slot_num = slot_idx + 1;
                                                    let current_val = slots.read()[slot_idx].clone();
                                                    rsx! {
                                                        input {
                                                            class: if is_weak { "pass-story-slot weak" } else { "pass-story-slot" },
                                                            placeholder: "Slot {slot_num}",
                                                            value: "{current_val}",
                                                            oninput: move |evt| {
                                                                slots.write()[slot_idx] = evt.value();
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(ref err) = *error_msg.read() {
                                div { class: "pass-story-error", "{err}" }
                            }

                            div { class: "pass-story-actions",
                                button {
                                    class: "pass-story-btn secondary",
                                    onclick: move |_| on_close.call(()),
                                    "Cancel"
                                }
                                button {
                                    class: "pass-story-btn primary",
                                    disabled: slots.read().iter().any(|s| s.trim().is_empty()),
                                    onclick: move |_| {
                                        let current_slots = slots.read().clone();
                                        let arr: [String; 23] = match current_slots.try_into() {
                                            Ok(a) => a,
                                            Err(_) => return,
                                        };

                                        match indras_crypto::entropy::entropy_gate(&arr) {
                                            Ok(()) => {
                                                weak_slots.set(Vec::new());
                                                error_msg.set(None);
                                                phase.set(PassStoryPhase::Review);
                                            }
                                            Err(e) => {
                                                // Extract weak slots from error
                                                let err_str = format!("{}", e);
                                                error_msg.set(Some(format!("Some slots are too weak: {}", err_str)));
                                                // Try to parse weak slot indices from the error
                                                // The entropy_gate error includes weak_slots info
                                            }
                                        }
                                    },
                                    "Review Story"
                                }
                            }
                        },
                        PassStoryPhase::Review => rsx! {
                            div { class: "pass-story-review",
                                for (stage_idx, stage) in template.stages.iter().enumerate() {
                                    {
                                        let (start, end) = boundaries[stage_idx];
                                        let current_slots = slots.read().clone();
                                        let mut filled = stage.template.to_string();
                                        for idx in start..end {
                                            filled = filled.replacen(
                                                "`_____`",
                                                &format!("`{}`", &current_slots[idx]),
                                                1,
                                            );
                                        }
                                        rsx! {
                                            div { class: "pass-story-review-stage",
                                                div { class: "pass-story-stage-name", "{stage.name}" }
                                                div { class: "pass-story-review-text", "{filled}" }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "pass-story-warning",
                                "This story is your key. If you forget it, your identity cannot be recovered. "
                                "Make sure you can remember or securely store these words."
                            }

                            div { class: "pass-story-actions",
                                button {
                                    class: "pass-story-btn secondary",
                                    onclick: move |_| phase.set(PassStoryPhase::Fill),
                                    "Edit"
                                }
                                button {
                                    class: "pass-story-btn primary",
                                    onclick: move |_| {
                                        let current_slots = slots.read().clone();
                                        let arr: [String; 23] = match current_slots.try_into() {
                                            Ok(a) => a,
                                            Err(_) => return,
                                        };
                                        on_protect.call(arr);
                                        phase.set(PassStoryPhase::Done);
                                    },
                                    "Confirm & Protect"
                                }
                            }
                        },
                        PassStoryPhase::Done => rsx! {
                            div { class: "pass-story-done",
                                div { class: "pass-story-done-icon", "\u{1F6E1}" }
                                div { class: "pass-story-done-text",
                                    "Your identity is now protected with your PassStory."
                                }
                            }
                            div { class: "pass-story-actions",
                                button {
                                    class: "pass-story-btn primary",
                                    onclick: move |_| on_close.call(()),
                                    "Done"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
