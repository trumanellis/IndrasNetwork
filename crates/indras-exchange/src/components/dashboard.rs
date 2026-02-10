use dioxus::prelude::*;
use crate::state::{ExchangeState, Screen};
use super::token_card::TokenCardWidget;

#[component]
pub fn Dashboard(state: Signal<ExchangeState>) -> Element {
    let tokens = state.read().tokens.clone();
    let active_requests = state.read().active_requests.clone();
    let completed_exchanges = state.read().completed_exchanges.clone();
    let display_name = state.read().display_name.clone();

    rsx! {
        div {
            class: "screen-content dashboard",

            // Status bar
            div {
                class: "status-bar",
                span { "Intention Exchange" }
                span { "{display_name}" }
            }

            // Screen title
            div {
                class: "screen-title",
                "Dashboard"
            }

            // Action buttons
            div {
                class: "dashboard-actions",
                button {
                    class: "button-primary",
                    onclick: move |_| {
                        state.write().screen = Screen::CreateIntention;
                    },
                    "New Intention"
                }
                button {
                    class: "button-secondary",
                    onclick: move |_| {
                        state.write().screen = Screen::EnterCode;
                    },
                    "Enter Code"
                }
            }

            // My Tokens section
            if !tokens.is_empty() {
                div {
                    class: "section",
                    div { class: "section-title", "My Tokens" }
                    div {
                        class: "token-list",
                        for token in tokens.iter() {
                            TokenCardWidget {
                                key: "{token.name}",
                                name: token.name.clone(),
                                description: token.description.clone(),
                                hours: token.hours.clone(),
                                earned_date: token.earned_date.clone(),
                                selected: false,
                                on_click: move |_| {},
                            }
                        }
                    }
                }
            }

            // Active Intentions section
            if !active_requests.is_empty() {
                div {
                    class: "section",
                    div { class: "section-title", "Active Intentions" }
                    for req in active_requests.iter() {
                        div {
                            class: "intention-card",
                            onclick: {
                                let id = req.id.clone();
                                move |_| {
                                    state.write().screen = Screen::ShareIntention(id.clone());
                                }
                            },
                            div { class: "intention-title", "{req.title}" }
                            div { class: "intention-description", "{req.description}" }
                            if !req.location.is_empty() {
                                div {
                                    class: "intention-location",
                                    "Location: {req.location}"
                                }
                            }
                            if let Some(ref code) = req.magic_code {
                                div {
                                    class: "magic-code-small",
                                    "{code}"
                                }
                            }
                        }
                    }
                }
            }

            // Completed Exchanges section
            if !completed_exchanges.is_empty() {
                div {
                    class: "section",
                    div { class: "section-title", "Completed Exchanges" }
                    for ex in completed_exchanges.iter() {
                        div {
                            class: "exchange-card completed",
                            div { class: "exchange-title", "{ex.request_title}" }
                            div { class: "exchange-provider", "with {ex.provider_name}" }
                        }
                    }
                }
            }

            // Empty state
            if tokens.is_empty() && active_requests.is_empty() {
                div {
                    class: "empty-state",
                    div { class: "empty-icon", "+" }
                    div { class: "empty-text", "Create your first intention to get started" }
                }
            }
        }
    }
}
