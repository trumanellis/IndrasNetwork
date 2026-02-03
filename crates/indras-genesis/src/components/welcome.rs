//! Welcome screen with branding and auto-advance.

use dioxus::prelude::*;
use tokio::time::{sleep, Duration};

use crate::state::{GenesisState, GenesisStep};

#[component]
pub fn WelcomeScreen(mut state: Signal<GenesisState>) -> Element {
    // Auto-advance after 1.5 seconds
    use_effect(move || {
        spawn(async move {
            sleep(Duration::from_millis(1500)).await;
            let current = state.read().step.clone();
            if current == GenesisStep::Welcome {
                state.write().step = GenesisStep::DisplayName;
            }
        });
    });

    rsx! {
        div {
            class: "genesis-screen genesis-welcome",

            h1 {
                class: "genesis-brand-title",
                "Synchronicity Engine"
            }
            p {
                class: "genesis-brand-subtitle",
                "Your sovereign identity awaits"
            }
        }
    }
}
