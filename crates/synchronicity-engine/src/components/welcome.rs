//! Welcome screen — name input, Create Account or Sign In.

use dioxus::prelude::*;

use crate::state::{AppState, AppStep};

/// Welcome splash screen with inline name input and Create/Sign In actions.
#[component]
pub fn Welcome(mut state: Signal<AppState>) -> Element {
    let mut name = use_signal(String::new);

    rsx! {
        div {
            class: "welcome-screen",

            h1 {
                class: "welcome-title",
                "THE SYNCHRONICITY ENGINE"
            }

            p {
                class: "welcome-subtitle",
                "Your sovereign vault, synced everywhere"
            }

            div {
                class: "welcome-actions",

                input {
                    class: "text-input",
                    r#type: "text",
                    placeholder: "What should we call you?",
                    value: "{name}",
                    oninput: move |e| *name.write() = e.value(),
                }

                button {
                    class: "se-btn-glow",
                    disabled: name.read().trim().is_empty(),
                    onclick: move |_| {
                        state.write().display_name = name.read().trim().to_string();
                        state.write().step = AppStep::Creating;
                    },
                    "Create Account"
                }

                button {
                    class: "se-btn-outline",
                    onclick: move |_| state.write().step = AppStep::RestoreStory,
                    "Sign In"
                }
            }

            p {
                class: "welcome-hint",
                "Already have an account on another device? Sign in with your story."
            }
        }
    }
}
