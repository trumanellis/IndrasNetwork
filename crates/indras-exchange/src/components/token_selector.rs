use dioxus::prelude::*;
use crate::engine;
use crate::state::{ExchangeState, Screen};
use super::token_card::TokenCardWidget;

#[component]
pub fn TokenSelector(state: Signal<ExchangeState>) -> Element {
    let tokens = state.read().tokens.clone();
    let selected_ids = state.read().selected_token_ids.clone();
    let has_selection = !selected_ids.is_empty();

    rsx! {
        div {
            class: "screen-content",

            div {
                class: "status-bar",
                span {
                    class: "back-button",
                    onclick: move |_| {
                        state.write().screen = Screen::CreateIntention;
                    },
                    "<-"
                }
                span { "Attach Tokens" }
            }

            div { class: "screen-title", "Attach Tokens" }

            div {
                class: "input-label",
                style: "margin-bottom: 1rem;",
                "Select tokens to offer in exchange:"
            }

            div {
                class: "token-list",
                for token in tokens.iter() {
                    {
                        let token_id = token.id.clone();
                        let is_selected = selected_ids.contains(&token_id);
                        rsx! {
                            TokenCardWidget {
                                key: "{token.name}",
                                name: token.name.clone(),
                                description: token.description.clone(),
                                hours: token.hours.clone(),
                                earned_date: token.earned_date.clone(),
                                selected: is_selected,
                                on_click: {
                                    let token_id = token_id.clone();
                                    move |_| {
                                        let mut s = state.write();
                                        if let Some(pos) = s.selected_token_ids.iter().position(|id| *id == token_id) {
                                            s.selected_token_ids.remove(pos);
                                        } else {
                                            s.selected_token_ids.push(token_id.clone());
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }

            button {
                class: if has_selection { "button-primary" } else { "button-primary disabled" },
                disabled: !has_selection,
                onclick: move |_| {
                    if has_selection {
                        let now = chrono::Utc::now().timestamp();
                        let mut s = state.write();
                        let title = s.draft_title.clone();
                        let desc = s.draft_description.clone();
                        let loc = s.draft_location.clone();
                        let token_ids = s.selected_token_ids.clone();

                        match engine::create_intention(
                            &mut s.vault,
                            &title,
                            &desc,
                            &loc,
                            &token_ids,
                            now,
                        ) {
                            Ok(request_view) => {
                                let request_id = request_view.id.clone();
                                s.active_requests.push(request_view);
                                // Clear draft
                                s.draft_title.clear();
                                s.draft_description.clear();
                                s.draft_location.clear();
                                s.selected_token_ids.clear();
                                s.screen = Screen::ShareIntention(request_id);
                            }
                            Err(e) => {
                                tracing::error!("Failed to create intention: {}", e);
                                s.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                },
                "Create Intention Card"
            }
        }
    }
}
