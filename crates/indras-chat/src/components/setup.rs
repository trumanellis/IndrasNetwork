//! Setup flow — multi-step onboarding: Welcome → DisplayName → PassStory → Creating.

use dioxus::prelude::*;
use indras_crypto::StoryTemplate;

/// Current step of the setup flow.
#[derive(Clone, Debug, PartialEq)]
pub enum SetupStep {
    Welcome,
    DisplayName,
    PassStory,
    Creating,
}

/// Phase of the PassStory sub-flow.
#[derive(Clone, Debug, PartialEq)]
enum StoryPhase {
    Fill,
    Review,
}

/// Full setup flow component.
#[component]
pub fn SetupView(
    on_create: EventHandler<(String, Option<[String; 23]>)>,
    error: Option<String>,
    loading: bool,
) -> Element {
    let mut step = use_signal(|| SetupStep::Welcome);
    let mut name = use_signal(String::new);

    // PassStory state
    let mut slots: Signal<Vec<String>> = use_signal(|| vec![String::new(); 23]);
    let mut story_phase = use_signal(|| StoryPhase::Fill);
    let mut weak_slots = use_signal(Vec::<usize>::new);
    let mut story_error = use_signal(|| None::<String>);

    // Auto-advance Welcome after 1.5s
    let step_clone = step.clone();
    use_effect(move || {
        if *step_clone.read() == SetupStep::Welcome {
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                step.set(SetupStep::DisplayName);
            });
        }
    });

    let current_step = step.read().clone();

    match current_step {
        SetupStep::Welcome => rsx! {
            div { class: "setup-container",
                div { class: "setup-card",
                    div { class: "setup-logo", "I" }
                    div { class: "setup-title setup-title-large", "Indras Network" }
                    div { class: "setup-subtitle", "Synchronicity Engine" }
                    div { class: "setup-fade-hint", "Your sovereign identity awaits" }
                }
            }
        },

        SetupStep::DisplayName => {
            let can_submit = !name.read().trim().is_empty();
            rsx! {
                div { class: "setup-container",
                    div { class: "setup-card",
                        div { class: "setup-logo", "I" }
                        div { class: "setup-title", "Choose Your Name" }
                        div { class: "setup-subtitle", "This is how others will see you on the network" }

                        input {
                            class: "setup-input",
                            r#type: "text",
                            placeholder: "Display name",
                            value: "{name}",
                            autofocus: true,
                            oninput: move |evt| name.set(evt.value()),
                            onkeydown: move |evt: KeyboardEvent| {
                                if evt.key() == Key::Enter && can_submit {
                                    step.set(SetupStep::PassStory);
                                }
                            },
                        }

                        button {
                            class: "setup-button",
                            disabled: !can_submit,
                            onclick: move |_| {
                                if can_submit {
                                    step.set(SetupStep::PassStory);
                                }
                            },
                            "Continue"
                        }
                    }
                }
            }
        },

        SetupStep::PassStory => {
            let template = StoryTemplate::default_template();
            let boundaries = template.stage_boundaries();
            let current_story_phase = story_phase.read().clone();
            let display_name = name.read().clone();

            match current_story_phase {
                StoryPhase::Fill => rsx! {
                    div { class: "setup-container setup-container-wide",
                        div { class: "setup-card setup-card-wide",
                            div { class: "setup-step-indicator", "Step 2 of 2" }
                            div { class: "setup-title", "Write Your PassStory" }
                            div { class: "setup-subtitle",
                                "Your story becomes the key that protects your identity. "
                                "Fill in each blank with a memorable word or phrase."
                            }

                            div { class: "pass-story-stages",
                                for (stage_idx, stage) in template.stages.iter().enumerate() {
                                    {
                                        let (start, end) = boundaries[stage_idx];
                                        let weak_list = weak_slots.read().clone();
                                        rsx! {
                                            div { class: "pass-story-stage",
                                                div { class: "pass-story-stage-name", "{stage.name}" }
                                                div { class: "pass-story-stage-desc", "{stage.description}" }
                                                div { class: "pass-story-stage-template", "{stage.template}" }

                                                div { class: "pass-story-slots-row",
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
                                }
                            }

                            // Progress counter
                            {
                                let filled = slots.read().iter().filter(|s| !s.trim().is_empty()).count();
                                rsx! {
                                    div { class: "pass-story-progress", "{filled} / 23 slots filled" }
                                }
                            }

                            if let Some(ref err) = *story_error.read() {
                                div { class: "setup-error", "{err}" }
                            }

                            div { class: "setup-actions",
                                button {
                                    class: "setup-button-secondary",
                                    onclick: move |_| {
                                        // Skip PassStory — create identity without protection
                                        let n = display_name.clone();
                                        on_create.call((n, None));
                                    },
                                    "Skip for now"
                                }
                                button {
                                    class: "setup-button",
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
                                                story_error.set(None);
                                                story_phase.set(StoryPhase::Review);
                                            }
                                            Err(e) => {
                                                let err_str = format!("{}", e);
                                                story_error.set(Some(err_str));
                                            }
                                        }
                                    },
                                    "Review Story"
                                }
                            }
                        }
                    }
                },
                StoryPhase::Review => {
                    let display_name_for_confirm = display_name.clone();
                    rsx! {
                        div { class: "setup-container setup-container-wide",
                            div { class: "setup-card setup-card-wide",
                                div { class: "setup-title", "Review Your Story" }
                                div { class: "setup-subtitle",
                                    "Read through your complete narrative. Make sure you can remember it."
                                }

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

                                if let Some(ref err) = error {
                                    div { class: "setup-error", "{err}" }
                                }

                                div { class: "setup-actions",
                                    button {
                                        class: "setup-button-secondary",
                                        onclick: move |_| story_phase.set(StoryPhase::Fill),
                                        "Edit Story"
                                    }
                                    button {
                                        class: "setup-button",
                                        disabled: loading,
                                        onclick: move |_| {
                                            let current_slots = slots.read().clone();
                                            let arr: [String; 23] = match current_slots.try_into() {
                                                Ok(a) => a,
                                                Err(_) => return,
                                            };
                                            let n = display_name_for_confirm.clone();
                                            on_create.call((n, Some(arr)));
                                        },
                                        if loading { "Creating identity..." } else { "Confirm & Create Identity" }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        },

        SetupStep::Creating => rsx! {
            div { class: "setup-container",
                div { class: "setup-card",
                    div { class: "setup-logo", "I" }
                    div { class: "setup-title", "Creating Your Identity..." }
                    div { class: "setup-subtitle", "Generating cryptographic keys and protecting your story" }
                }
            }
        },
    }
}
