use dioxus::prelude::*;
use crate::state::{ExchangeState, Screen};

#[component]
pub fn IntentionCreator(state: Signal<ExchangeState>) -> Element {
    let title = state.read().draft_title.clone();
    let description = state.read().draft_description.clone();
    let location = state.read().draft_location.clone();

    let can_proceed = !title.is_empty();

    rsx! {
        div {
            class: "screen-content",

            div {
                class: "status-bar",
                span {
                    class: "back-button",
                    onclick: move |_| {
                        state.write().screen = Screen::Dashboard;
                    },
                    "<-"
                }
                span { "New Intention" }
            }

            div { class: "screen-title", "New Intention" }

            div {
                class: "input-group",
                label { class: "input-label", "What do you seek?" }
                input {
                    class: "text-input",
                    r#type: "text",
                    placeholder: "e.g., Delicious cheesecake",
                    value: "{title}",
                    oninput: move |evt: Event<FormData>| {
                        state.write().draft_title = evt.value();
                    },
                }
            }

            div {
                class: "input-group",
                label { class: "input-label", "Details (optional)" }
                textarea {
                    class: "text-input textarea",
                    placeholder: "Add any specific requests or context...",
                    value: "{description}",
                    oninput: move |evt: Event<FormData>| {
                        state.write().draft_description = evt.value();
                    },
                }
            }

            div {
                class: "input-group",
                label { class: "input-label", "Location" }
                input {
                    class: "text-input",
                    r#type: "text",
                    placeholder: "e.g., Community Market - Stall 7",
                    value: "{location}",
                    oninput: move |evt: Event<FormData>| {
                        state.write().draft_location = evt.value();
                    },
                }
            }

            button {
                class: if can_proceed { "button-primary" } else { "button-primary disabled" },
                disabled: !can_proceed,
                onclick: move |_| {
                    if can_proceed {
                        state.write().screen = Screen::AttachTokens;
                    }
                },
                "Next: Attach Tokens"
            }
        }
    }
}
