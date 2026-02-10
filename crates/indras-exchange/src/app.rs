use dioxus::prelude::*;
use indras_ui::ThemedRoot;

use crate::engine;
use crate::state::ExchangeState;
use crate::components::*;

/// Root application component.
#[component]
pub fn App() -> Element {
    use_hook(|| {
        *indras_ui::CURRENT_THEME.write() = indras_ui::Theme::MinimalTerminal;
    });

    let state = use_signal(|| {
        let mut s = ExchangeState::new();
        // Seed demo tokens
        let now = chrono::Utc::now().timestamp();
        match engine::seed_demo_tokens(&mut s.vault, now) {
            Ok(tokens) => s.tokens = tokens,
            Err(e) => tracing::error!("Failed to seed tokens: {}", e),
        }
        s.display_name = "Zephyr".to_string();
        s
    });

    let current_screen = state.read().screen.clone();

    rsx! {
        ThemedRoot {
            div {
                class: "exchange-app",
                match current_screen {
                    crate::state::Screen::Dashboard => rsx! {
                        Dashboard { state }
                    },
                    crate::state::Screen::CreateIntention => rsx! {
                        IntentionCreator { state }
                    },
                    crate::state::Screen::AttachTokens => rsx! {
                        TokenSelector { state }
                    },
                    crate::state::Screen::ShareIntention(ref id) => rsx! {
                        IntentionCard { state, request_id: id.clone() }
                    },
                    crate::state::Screen::EnterCode => rsx! {
                        CodeEntry { state }
                    },
                    crate::state::Screen::ProviderView(ref id) => rsx! {
                        ProviderView { state, request_id: id.clone() }
                    },
                    crate::state::Screen::FulfillForm(ref id) => rsx! {
                        ProofForm { state, request_id: id.clone() }
                    },
                    crate::state::Screen::ReleaseReview(ref id) => rsx! {
                        ReleaseReview { state, exchange_id: id.clone() }
                    },
                    crate::state::Screen::ExchangeComplete(ref id) => rsx! {
                        ExchangeComplete { state, exchange_id: id.clone() }
                    },
                }
            }
        }
    }
}
