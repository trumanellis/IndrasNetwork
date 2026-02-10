use dioxus::prelude::*;
use indras_artifacts::ArtifactId;
use crate::engine;
use crate::state::{ExchangeState, Screen};

#[component]
pub fn ProviderView(state: Signal<ExchangeState>, request_id: ArtifactId) -> Element {
    // Load request view from vault
    let request_view = {
        let s = state.read();
        // Try active_requests first (for demo flow where we are both requester and provider)
        s.active_requests.iter().find(|r| r.id == request_id).cloned()
            .or_else(|| s.incoming_request.clone())
            .or_else(|| {
                engine::receive_request_as_provider(&s.vault, &request_id).ok().flatten()
            })
    };

    let Some(req) = request_view else {
        return rsx! {
            div { class: "screen-content",
                div { class: "screen-title", "Request not found" }
                button {
                    class: "button-primary",
                    onclick: move |_| { state.write().screen = Screen::Dashboard; },
                    "Back to Dashboard"
                }
            }
        };
    };

    let title = req.title.clone();
    let description = req.description.clone();
    let location = req.location.clone();
    let token_name = req.token_name.clone().unwrap_or_else(|| "Token".to_string());
    let requester_name = state.read().display_name.clone();
    let request_id_for_fulfill = request_id.clone();

    rsx! {
        div {
            class: "screen-content",

            div {
                class: "status-bar",
                span {
                    class: "back-button",
                    onclick: move |_| { state.write().screen = Screen::Dashboard; },
                    "<-"
                }
                span { "Intention Request" }
            }

            div { class: "screen-title", "Intention Request" }

            // Requester profile
            div {
                class: "requester-profile",
                div { class: "avatar-circle", "Z" }
                div {
                    class: "requester-info",
                    div { class: "requester-name", "{requester_name}" }
                    div { class: "requester-meta", "2 connections" }
                }
            }

            // Intention card
            div {
                class: "intention-card",
                div { class: "intention-title", "{title}" }
                div { class: "intention-description", "{description}" }
                if !location.is_empty() {
                    div { class: "intention-location", "Location: {location}" }
                }
                div {
                    class: "token-attached",
                    div { class: "token-attached-label", "Token Offered" }
                    div {
                        class: "token-attached-row",
                        div { class: "token-attached-name", "{token_name}" }
                    }
                }
            }

            // Action buttons
            button {
                class: "button-primary",
                onclick: move |_| {
                    state.write().screen = Screen::FulfillForm(request_id_for_fulfill.clone());
                },
                "Fulfill Intention"
            }
            button {
                class: "button-secondary decline",
                onclick: move |_| { state.write().screen = Screen::Dashboard; },
                "Decline"
            }
        }
    }
}
