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
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !name.read().trim().is_empty() {
                            state.write().display_name = name.read().trim().to_string();
                            state.write().step = AppStep::Creating;
                        }
                    },
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
                    "Sign in with a pass story"
                }
            }

            p {
                class: "welcome-hint",
                "Lost your device? Create a new account first, then pick "
                b { "Use backup" }
                " from the status bar — your backup friends will help you back in."
            }
        }
    }
}
