//! Root application component.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_ui::{ThemedRoot, CURRENT_SKIN, Skin};

use crate::state::{AppState, AppStep};
use crate::vault_manager::VaultManager;

/// Root component — routes to the correct screen based on [`AppState::step`].
#[component]
pub fn App() -> Element {
    use_hook(|| {
        *CURRENT_SKIN.write() = Skin::Technical;
    });

    let mut state = use_signal(AppState::new);
    let mut network: Signal<Option<Arc<IndrasNetwork>>> = use_signal(|| None);
    let mut vault_manager: Signal<Option<Arc<VaultManager>>> = use_signal(|| None);

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

        // Skip if network was already created by the creation/restore flow.
        if network.read().is_some() {
            return;
        }

        let data_dir = crate::state::default_data_dir();
        spawn(async move {
            // Retry up to 3 times with backoff if the database is locked
            // (stale lock from a crash, or another instance starting simultaneously).
            const MAX_RETRIES: u32 = 3;
            let mut last_err = None;

            for attempt in 0..=MAX_RETRIES {
                if attempt > 0 {
                    let delay = std::time::Duration::from_millis(500 * u64::from(attempt));
                    tracing::info!("Database locked, retrying in {}ms (attempt {}/{})", delay.as_millis(), attempt, MAX_RETRIES);
                    tokio::time::sleep(delay).await;
                }

                match IndrasNetwork::new(&data_dir).await {
                    Ok(net) => {
                        if let Err(e) = net.start().await {
                            tracing::error!("Failed to start network: {e}");
                        }
                        if let Err(e) = net.join_contacts_realm().await {
                            tracing::warn!("Failed to join contacts realm: {e}");
                        }
                        // Prefer the synced ProfileIdentityDocument; fall back to
                        // the local network builder's display name.
                        let display_name = match crate::profile_bridge::load_profile_identity(&net).await {
                            Some(p) if !p.display_name.is_empty() => p.display_name,
                            _ => net.display_name().unwrap_or_default(),
                        };
                        if !display_name.is_empty() {
                            state.write().display_name = display_name.clone();
                        }
                        crate::profile_bridge::ensure_profile_artifacts(&net).await;
                        let _homepage = crate::profile_server::start_homepage_server(&net, &data_dir).await;
                        network.set(Some(net));
                        let data_dir = crate::state::default_data_dir();
                        match VaultManager::new(data_dir).await {
                            Ok(vm) => {
                                let vault_path = vm.start_private_vault(&display_name).await;
                                crate::vault_bridge::ensure_vault_ready(&vault_path);
                                state.write().vault_path = vault_path;
                                vault_manager.set(Some(Arc::new(vm)));
                            }
                            Err(e) => tracing::error!("Failed to start vault manager: {e}"),
                        }
                        state.write().sync_status = crate::state::SyncStatus::Synced;
                        last_err = None;
                        break;
                    }
                    Err(e) => {
                        if e.is_locked() && attempt < MAX_RETRIES {
                            last_err = Some(e);
                            continue;
                        }
                        last_err = Some(e);
                        break;
                    }
                }
            }

            if let Some(e) = last_err {
                tracing::error!("Failed to load network: {e}");
                let message = if e.is_locked() {
                    "Another instance is already running. Please close it first.".to_string()
                } else {
                    e.to_string()
                };
                state.write().sync_status = crate::state::SyncStatus::Error(message);
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
                    rsx! { super::loading::Loading { state, network, vault_manager } }
                },
                AppStep::HomeVault => rsx! { super::home_vault::HomeVault { state, network, vault_manager } },
            }
        }
    }
}
