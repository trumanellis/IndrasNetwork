use dioxus::prelude::*;
use indras_artifacts::ArtifactId;
use crate::engine;
use crate::state::{ExchangeState, Screen};

#[component]
pub fn ProofForm(state: Signal<ExchangeState>, request_id: ArtifactId) -> Element {
    let proof_title = state.read().draft_proof_title.clone();
    let proof_description = state.read().draft_proof_description.clone();
    let can_submit = !proof_title.is_empty();

    let request_id_for_submit = request_id.clone();

    rsx! {
        div {
            class: "screen-content",

            div {
                class: "status-bar",
                span {
                    class: "back-button",
                    onclick: {
                        let rid = request_id.clone();
                        move |_| { state.write().screen = Screen::ProviderView(rid.clone()); }
                    },
                    "<-"
                }
                span { "Post Proof" }
            }

            div { class: "screen-title", "Post Proof of Service" }

            div {
                class: "proof-form",

                div {
                    class: "input-group",
                    label { class: "input-label", "What did you provide?" }
                    input {
                        class: "text-input",
                        r#type: "text",
                        placeholder: "e.g., Maracuja Cheesecake Slice",
                        value: "{proof_title}",
                        oninput: move |evt: Event<FormData>| {
                            state.write().draft_proof_title = evt.value();
                        },
                    }
                }

                div {
                    class: "input-group",
                    label { class: "input-label", "Description" }
                    textarea {
                        class: "text-input textarea",
                        placeholder: "Describe what you provided...",
                        value: "{proof_description}",
                        oninput: move |evt: Event<FormData>| {
                            state.write().draft_proof_description = evt.value();
                        },
                    }
                }

                div {
                    class: "input-group",
                    label { class: "input-label", "Photo (optional)" }
                    div {
                        class: "image-upload",
                        div { class: "upload-icon", "camera" }
                        div { class: "upload-text", "Tap to add photo" }
                    }
                }
            }

            button {
                class: if can_submit { "button-primary" } else { "button-primary disabled" },
                disabled: !can_submit,
                onclick: move |_| {
                    let now = chrono::Utc::now().timestamp();
                    let mut s = state.write();
                    let pt = s.draft_proof_title.clone();
                    let pd = s.draft_proof_description.clone();
                    let provider_id = s.player_id;

                    match engine::submit_proof_and_propose(
                        &mut s.vault,
                        &request_id_for_submit,
                        &pt,
                        &pd,
                        provider_id,
                        now,
                    ) {
                        Ok(exchange_id) => {
                            // Build exchange view
                            if let Ok(view) = engine::build_exchange_view(
                                &s.vault,
                                &exchange_id,
                                &s.display_name,
                            ) {
                                s.active_exchanges.push(view);
                            }
                            s.draft_proof_title.clear();
                            s.draft_proof_description.clear();
                            s.screen = Screen::ReleaseReview(exchange_id);
                        }
                        Err(e) => {
                            tracing::error!("Failed to submit proof: {}", e);
                            s.status_message = Some(format!("Error: {}", e));
                        }
                    }
                },
                "Submit Proof of Service"
            }
        }
    }
}
