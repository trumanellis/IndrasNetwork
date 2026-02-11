use dioxus::prelude::*;
use crate::state::{ExchangeState, Screen};

#[component]
pub fn CodeEntry(state: Signal<ExchangeState>) -> Element {
    let code_input = state.read().code_input.clone();
    let has_input = !code_input.trim().is_empty();

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
                span { "Enter Code" }
            }

            div { class: "screen-title", "Enter Intention Code" }

            div {
                class: "code-entry-description",
                "Enter the magic code shown on the requester's screen to view their intention."
            }

            div {
                class: "input-group",
                label { class: "input-label", "Magic Code" }
                input {
                    class: "text-input magic-code-input",
                    r#type: "text",
                    placeholder: "INDRA-XXXX-XXXX",
                    value: "{code_input}",
                    oninput: move |evt: Event<FormData>| {
                        state.write().code_input = evt.value().to_uppercase();
                    },
                }
            }

            if let Some(ref msg) = state.read().status_message {
                div {
                    class: "status-message error",
                    "{msg}"
                }
            }

            button {
                class: if has_input { "button-primary" } else { "button-primary disabled" },
                disabled: !has_input,
                onclick: move |_| {
                    // For demo: find matching request by scanning active_requests for matching magic code
                    let s = state.read();
                    let code = s.code_input.trim().to_string();
                    let found = s.active_requests.iter().find(|r| {
                        r.magic_code.as_deref() == Some(&code)
                    }).map(|r| r.id.clone());
                    drop(s);

                    match found {
                        Some(id) => {
                            state.write().code_input.clear();
                            state.write().status_message = None;
                            state.write().screen = Screen::ProviderView(id);
                        }
                        None => {
                            state.write().status_message = Some("No matching intention found. Check the code and try again.".to_string());
                        }
                    }
                },
                "Find Intention"
            }
        }
    }
}
