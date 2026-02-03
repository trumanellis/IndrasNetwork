//! Pass story flow orchestrator - manages the lazy story setup overlay.
//!
//! Two screens:
//! 1. Single-page form with all 11 stages and inline hints
//! 2. Review screen showing the full narrative, then confirm

use std::sync::Arc;

use dioxus::prelude::*;
use indras_crypto::{CryptoError, PassStory, StoryTemplate, entropy};
use indras_network::{IndrasNetwork, StoryAuth};

use crate::state::{AsyncStatus, GenesisState, PassStoryState};

use super::app::default_data_dir;
use super::story_review::StoryReview;
use super::story_stage::StoryStage;

#[component]
pub fn PassStoryFlow(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    let mut story_state = use_signal(PassStoryState::default);

    let template = StoryTemplate::default_template();
    let total_stages = template.stages.len();
    let current_stage = story_state.read().current_stage;
    let submitted = story_state.read().submitted;

    // current_stage >= total_stages means "review mode"
    let show_review = current_stage >= total_stages && !submitted;

    rsx! {
        div {
            class: "story-overlay",

            // Close button
            button {
                class: "story-close",
                onclick: move |_| {
                    state.write().pass_story_active = false;
                },
                "\u{2715}"
            }

            if submitted {
                // Success state
                div {
                    class: "story-success",
                    h2 { class: "story-stage-name", "Identity Protected" }
                    p { class: "story-hint", "Your keys are now encrypted with your story." }
                    button {
                        class: "genesis-btn-primary",
                        onclick: move |_| {
                            state.write().pass_story_active = false;
                        },
                        "Return to Home Realm"
                    }
                }
            } else if show_review {
                StoryReview {
                    story_state,
                    on_confirm: move |_| {
                        let mut ss = story_state;
                        let network = network;
                        let mut state = state;
                        spawn(async move {
                            // Validate entropy first to get weak_slots
                            let slots = ss.read().slots.clone();
                            match entropy::entropy_gate(&slots) {
                                Ok(()) => {
                                    ss.write().status = AsyncStatus::Loading;

                                    // Build PassStory from slots
                                    let slot_refs: Vec<&str> = slots.iter().map(|s| s.as_str()).collect();
                                    let slot_array: [&str; 23] = match slot_refs.try_into() {
                                        Ok(a) => a,
                                        Err(_) => {
                                            ss.write().status = AsyncStatus::Error("Invalid slot count".into());
                                            return;
                                        }
                                    };

                                    let story = match PassStory::from_raw(&slot_array) {
                                        Ok(s) => s,
                                        Err(e) => {
                                            ss.write().status = AsyncStatus::Error(format!("Story error: {}", e));
                                            return;
                                        }
                                    };

                                    // Get user ID and timestamp
                                    let net = network.read();
                                    let net = match net.as_ref() {
                                        Some(n) => n,
                                        None => {
                                            ss.write().status = AsyncStatus::Error("Network not ready".into());
                                            return;
                                        }
                                    };
                                    let user_id = net.id();
                                    let timestamp = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();

                                    // Create story-based keystore
                                    let data_dir = default_data_dir();
                                    match StoryAuth::create_account(
                                        &data_dir,
                                        &story,
                                        &user_id,
                                        timestamp,
                                    ) {
                                        Ok(_auth) => {
                                            tracing::info!("Story auth account created successfully");
                                            ss.write().submitted = true;
                                            ss.write().status = AsyncStatus::Idle;

                                            // Refresh home realm data to update quest checklist
                                            if let Ok(home) = net.home_realm().await {
                                                if let Ok(doc) = home.quests().await {
                                                    let data = doc.read().await;
                                                    let quests: Vec<crate::state::QuestView> = data.quests.iter().map(|q| {
                                                        crate::state::QuestView {
                                                            id: q.id.iter().map(|b| format!("{:02x}", b)).collect(),
                                                            title: q.title.clone(),
                                                            description: q.description.clone(),
                                                            is_complete: q.completed_at_millis.is_some(),
                                                        }
                                                    }).collect();
                                                    drop(data);
                                                    state.write().quests = quests;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("Story auth failed: {}", e);
                                            ss.write().status = AsyncStatus::Error(
                                                format!("Failed to protect identity: {}", e)
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    // Extract weak slots from CryptoError
                                    if let CryptoError::EntropyBelowThreshold { weak_slots, .. } = &e {
                                        ss.write().weak_slots = weak_slots.clone();
                                    }
                                    tracing::warn!("Entropy gate failed: {}", e);
                                    ss.write().status = AsyncStatus::Error(
                                        "Story needs stronger words. Edit highlighted slots.".to_string()
                                    );
                                }
                            }
                        });
                    },
                    on_edit_stage: move |_stage_idx: usize| {
                        // Go back to the single-page form (stage 0 means "editing")
                        story_state.write().current_stage = 0;
                    },
                }
            } else {
                // Single-page form with all stages
                StoryStage {
                    story_state,
                    on_next: move |_| {
                        // Advance to review mode
                        story_state.write().current_stage = total_stages;
                    },
                    on_back: move |_| {
                        // Close the overlay
                        state.write().pass_story_active = false;
                    },
                }
            }
        }
    }
}
