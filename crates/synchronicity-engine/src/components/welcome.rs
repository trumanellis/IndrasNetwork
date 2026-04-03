//! Welcome screen — Create Account or Sign In choice.

use dioxus::prelude::*;

use crate::state::{AppState, AppStep};

/// Welcome splash screen with Create Account and Sign In actions.
#[component]
pub fn Welcome(mut state: Signal<AppState>) -> Element {
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

                button {
                    class: "se-btn-glow",
                    onclick: move |_| state.write().step = AppStep::DisplayName,
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
