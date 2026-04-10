//! Loading screen shown during vault creation and restore operations.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AppState, AppStep, LoadingStage};
use crate::vault_bridge;
use crate::vault_manager::VaultManager;

/// Loading screen with stage-by-stage progress display.
#[component]
pub fn Loading(
    state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
) -> Element {
    let step = state.read().step.clone();

    // Trigger account creation/restore once on mount.
    let mut started = use_signal(|| false);
    use_effect(move || {
        if *started.read() {
            return;
        }

        let is_creating = state.read().step == AppStep::Creating;
        let is_restoring = state.read().step == AppStep::Restoring;

        if is_creating {
            started.set(true);
            spawn(async move {
                vault_bridge::create_account(state, network, vault_manager).await;
            });
        } else if is_restoring {
            started.set(true);
            spawn(async move {
                vault_bridge::restore_account(state, network, vault_manager).await;
            });
        }
    });

    let stages = state.read().loading_stages.clone();
    let error = state.read().error.clone();

    let title = match step {
        AppStep::Creating => "Creating your vault...",
        AppStep::Restoring => "Connecting to your vault...",
        _ => "Loading...",
    };

    rsx! {
        div {
            class: "loading-screen",

            div { class: "spinner" }

            h2 {
                class: "loading-title",
                "{title}"
            }

            div {
                class: "loading-stages",

                for (idx, stage) in stages.iter().enumerate() {
                    div {
                        key: "{idx}",
                        class: "loading-stage",

                        match stage {
                            LoadingStage::Done(msg) => rsx! {
                                span { class: "stage-icon stage-done", "\u{2713}" }
                                span { class: "stage-label stage-label-done", "{msg}" }
                            },
                            LoadingStage::InProgress(msg) => rsx! {
                                span { class: "stage-icon stage-progress", "\u{25CF}" }
                                span { class: "stage-label stage-label-progress", "{msg}" }
                            },
                            LoadingStage::Failed(msg) => rsx! {
                                span { class: "stage-icon stage-failed", "\u{25CB}" }
                                span { class: "stage-label stage-label-failed", "{msg}" }
                            },
                        }
                    }
                }
            }

            if let Some(ref err) = error {
                div {
                    class: "se-error loading-error",
                    "{err}"
                }
            }
        }
    }
}
