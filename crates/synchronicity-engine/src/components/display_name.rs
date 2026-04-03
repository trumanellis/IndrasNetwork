//! Display name entry screen (create flow).

use dioxus::prelude::*;

use crate::state::{AppState, AppStep};

/// Screen to collect the user's display name before pass story creation.
#[component]
pub fn DisplayName(mut state: Signal<AppState>) -> Element {
    let mut name = use_signal(String::new);

    rsx! {
        div {
            class: "onboarding-screen",

            h2 { "What should we call you?" }

            p {
                class: "onboarding-subtitle",
                "This is how you'll appear in your vault."
            }

            input {
                class: "text-input",
                r#type: "text",
                placeholder: "Your name",
                value: "{name}",
                oninput: move |e| *name.write() = e.value(),
            }

            button {
                class: "se-btn-glow",
                disabled: name.read().trim().is_empty(),
                onclick: move |_| {
                    let n = name.read().trim().to_string();
                    if !n.is_empty() {
                        state.write().display_name = n;
                        state.write().step = AppStep::PassStory;
                    }
                },
                "Continue"
            }

            button {
                class: "se-btn-back",
                onclick: move |_| state.write().step = AppStep::Welcome,
                "\u{2190} Back"
            }
        }
    }
}
