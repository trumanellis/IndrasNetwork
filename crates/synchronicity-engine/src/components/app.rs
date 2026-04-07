//! Root application component.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_ui::{ThemedRoot, CURRENT_SKIN, Skin};

use crate::state::{AppState, AppStep};

/// Root component — routes to the correct screen based on [`AppState::step`].
#[component]
pub fn App() -> Element {
    use_hook(|| {
        *CURRENT_SKIN.write() = Skin::Technical;
    });

    let mut state = use_signal(AppState::new);
    let mut network: Signal<Option<Arc<IndrasNetwork>>> = use_signal(|| None);

    // One-shot guard: only attempt network load once for returning users.
    let mut network_loaded = use_signal(|| false);

    use_effect(move || {
        // Only fire once, and only for returning users who start at HomeVault.
        if *network_loaded.read() {
            return;
        }
        if state.read().step != AppStep::HomeVault {
            return;
        }
        network_loaded.set(true);

        // Ensure vault dir exists and seed HelloWorld.md for returning users.
        let vault_path = state.read().vault_path.clone();
        crate::vault_bridge::ensure_vault_ready(&vault_path);

        let data_dir = crate::state::default_data_dir();
        spawn(async move {
            match IndrasNetwork::new(&data_dir).await {
                Ok(net) => {
                    if let Err(e) = net.start().await {
                        tracing::error!("Failed to start network: {e}");
                    }
                    network.set(Some(net));
                    state.write().sync_status = crate::state::SyncStatus::Synced;
                }
                Err(e) => {
                    tracing::error!("Failed to load network: {e}");
                    state.write().sync_status =
                        crate::state::SyncStatus::Error(e.to_string());
                }
            }
        });
    });

    // On shutdown: stop network.
    let network_for_cleanup = network;
    use_drop(move || {
        if let Some(net) = network_for_cleanup.read().as_ref() {
            let net = net.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = net.stop().await {
                        tracing::error!("Failed to stop network: {e}");
                    }
                });
            });
        }
    });

    rsx! {
        ThemedRoot {
            match state.read().step.clone() {
                AppStep::Welcome => rsx! { super::welcome::Welcome { state } },
                AppStep::RestoreStory | AppStep::StoryReview => {
                    rsx! { super::pass_story::PassStory { state } }
                },
                AppStep::Creating | AppStep::Restoring => {
                    rsx! { super::loading::Loading { state, network } }
                },
                AppStep::HomeVault => rsx! { super::home_vault::HomeVault { state } },
            }
        }
    }
}
