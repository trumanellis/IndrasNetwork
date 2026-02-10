use dioxus::prelude::*;
use indras_artifacts::ArtifactId;
use crate::state::{ExchangeState, Screen};

#[component]
pub fn ExchangeComplete(state: Signal<ExchangeState>, exchange_id: ArtifactId) -> Element {
    // Find in completed exchanges
    let exchange_view = state.read().completed_exchanges.iter()
        .find(|e| e.id == exchange_id)
        .cloned();

    let provider_name = exchange_view.as_ref()
        .map(|e| e.provider_name.clone())
        .unwrap_or_else(|| "Provider".to_string());

    rsx! {
        div {
            class: "screen-content",

            div {
                class: "status-bar",
                span { "Intention Exchange" }
                span { "" }
            }

            // Success message
            div {
                class: "success-message",
                div { class: "success-icon", "check" }
                div { class: "success-title", "Exchange Complete!" }
                div {
                    class: "success-body",
                    "Your token has been released to {provider_name}. You're now connected through the Intention Exchange."
                }
            }

            // New connection card
            div {
                class: "intention-card",
                style: "margin-top: 2rem;",

                div {
                    class: "connection-badge-container",
                    div {
                        class: "connection-badge",
                        span { "handshake " }
                        span { "New Connection" }
                    }
                }

                div {
                    class: "new-connection-profile",
                    div { class: "avatar-circle", "P" }
                    div {
                        class: "connection-info",
                        div { class: "connection-name", "{provider_name}" }
                    }
                }

                div {
                    class: "connection-description",
                    "You can now make special requests and coordinate future exchanges directly with {provider_name}."
                }
            }

            button {
                class: "button-primary",
                style: "margin-top: 2rem;",
                onclick: move |_| {
                    state.write().screen = Screen::Dashboard;
                },
                "Back to Dashboard"
            }
        }
    }
}
