use dioxus::prelude::*;
use indras_artifacts::ArtifactId;
use crate::engine;
use crate::state::{ExchangeState, Screen};

#[component]
pub fn ReleaseReview(state: Signal<ExchangeState>, exchange_id: ArtifactId) -> Element {
    // Find exchange in active_exchanges
    let exchange_view = state.read().active_exchanges.iter()
        .find(|e| e.id == exchange_id)
        .cloned();

    let Some(ex) = exchange_view else {
        return rsx! {
            div { class: "screen-content",
                div { class: "screen-title", "Exchange not found" }
                button {
                    class: "button-primary",
                    onclick: move |_| { state.write().screen = Screen::Dashboard; },
                    "Back to Dashboard"
                }
            }
        };
    };

    let provider_name = ex.provider_name.clone();
    let proof_title = ex.proof_title.clone().unwrap_or_else(|| "Service provided".to_string());
    let proof_desc = ex.proof_description.clone().unwrap_or_default();
    let token_name = ex.token_name.clone();
    let exchange_id_for_release = exchange_id.clone();

    rsx! {
        div {
            class: "screen-content",

            div {
                class: "status-bar",
                span { "Intention Exchange" }
                span { "" }
            }

            div { class: "screen-title", "Intention Fulfilled!" }

            // Notification
            div {
                class: "notification",
                div {
                    class: "notification-title",
                    span { "check " }
                    span { "Proof of Service Received" }
                }
                div {
                    class: "notification-body",
                    "{provider_name} has fulfilled your intention and submitted proof. Review and release your token to complete the exchange."
                }
            }

            // Proof details card
            div {
                class: "intention-card",
                div { class: "intention-title", "{proof_title}" }
                if !proof_desc.is_empty() {
                    div { class: "intention-description", "{proof_desc}" }
                }

                // Provider info
                div {
                    class: "provider-info",
                    div { class: "provider-label", "Provider" }
                    div {
                        class: "provider-row",
                        div { class: "avatar-circle small", "P" }
                        div { class: "provider-name", "{provider_name}" }
                    }
                }

                // Token to release
                div {
                    class: "token-attached",
                    div { class: "token-attached-label", "Ready to Release" }
                    div {
                        class: "token-attached-row",
                        div { class: "token-attached-name", "{token_name}" }
                    }
                }
            }

            button {
                class: "button-primary",
                onclick: move |_| {
                    let now = chrono::Utc::now().timestamp();
                    let mut s = state.write();
                    match engine::release_token(&mut s.vault, &exchange_id_for_release, now) {
                        Ok(()) => {
                            // Move from active to completed
                            if let Some(pos) = s.active_exchanges.iter().position(|e| e.id == exchange_id_for_release) {
                                let mut completed = s.active_exchanges.remove(pos);
                                completed.completed = true;
                                s.completed_exchanges.push(completed);
                            }
                            s.screen = Screen::ExchangeComplete(exchange_id_for_release.clone());
                        }
                        Err(e) => {
                            tracing::error!("Failed to release token: {}", e);
                            s.status_message = Some(format!("Error: {}", e));
                        }
                    }
                },
                "Release Token to {provider_name}"
            }
        }
    }
}
