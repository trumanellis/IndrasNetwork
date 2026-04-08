//! Main vault view — 4-column realm layout with file modal.
//!
//! Uses `notify` filesystem watcher for instant private vault file detection.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::state::AppState;
use crate::vault_bridge::scan_vault;

/// Rescan the private vault and update state only if files changed.
fn rescan_private(state: &mut Signal<AppState>, vault_path: &std::path::Path) {
    let files = scan_vault(vault_path);

    let current_names: Vec<String> = state.read().private_files.iter().map(|f| f.name.clone()).collect();
    let new_names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    let list_changed = current_names.iter().map(|s| s.as_str()).collect::<Vec<_>>() != new_names;

    if list_changed {
        state.write().private_files = files;
    }
}

/// Main vault view: info bar, 4-column realm layout, file modal, status bar.
#[component]
pub fn HomeVault(mut state: Signal<AppState>) -> Element {
    // Initial scan + filesystem watcher for private vault
    use_effect(move || {
        let vault_path = state.read().vault_path.clone();

        rescan_private(&mut state, &vault_path);

        let watch_path = vault_path.clone();
        spawn(async move {
            use notify::{Watcher, RecursiveMode, Config};

            let changed = Arc::new(AtomicBool::new(false));
            let changed_writer = changed.clone();

            let _watcher = notify::RecommendedWatcher::new(
                move |_res: Result<notify::Event, notify::Error>| {
                    changed_writer.store(true, Ordering::Relaxed);
                },
                Config::default(),
            ).ok().and_then(|mut w| {
                w.watch(&watch_path, RecursiveMode::NonRecursive).ok()?;
                tracing::info!("Watching vault: {}", watch_path.display());
                Some(w)
            });

            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if changed.swap(false, Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    changed.store(false, Ordering::Relaxed);
                    rescan_private(&mut state, &vault_path);
                }
            }
        });
    });

    rsx! {
        div { class: "home-vault",
            super::vault_info_bar::VaultInfoBar { state }
            super::vault_columns::VaultColumns { state }
            super::status_bar::StatusBar { state }
            super::file_modal::FileModal { state }
        }
    }
}
