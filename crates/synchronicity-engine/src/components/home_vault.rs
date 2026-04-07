//! Main vault view — file list, preview, and info bar.
//!
//! Uses `notify` filesystem watcher for instant file change detection.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::state::AppState;
use crate::vault_bridge::scan_vault;

/// Rescan the vault and update state only if files changed.
fn rescan(state: &mut Signal<AppState>, vault_path: &std::path::Path) {
    let files = scan_vault(vault_path);

    // Only update state if the file list actually changed (avoids unnecessary re-renders)
    let current_names: Vec<String> = state.read().files.iter().map(|f| f.name.clone()).collect();
    let new_names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    let list_changed = current_names.iter().map(|s| s.as_str()).collect::<Vec<_>>() != new_names;

    if list_changed {
        let should_auto_select = state.read().selected_file.is_none() && !files.is_empty();
        let first_path = files.first().map(|f| f.path.clone());
        state.write().files = files;
        if should_auto_select {
            if let Some(path) = first_path {
                state.write().selected_file = Some(path);
            }
        }
    }
}

/// Main vault view orchestrator. Three-zone layout: info bar (top),
/// file list + preview (body), status bar (bottom).
#[component]
pub fn HomeVault(mut state: Signal<AppState>) -> Element {
    // Initial scan + filesystem watcher for instant updates.
    use_effect(move || {
        let vault_path = state.read().vault_path.clone();

        // Initial scan
        rescan(&mut state, &vault_path);

        // Start filesystem watcher + polling fallback
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

            // Poll the atomic flag + periodic rescan as fallback
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if changed.swap(false, Ordering::Relaxed) {
                    // Small extra debounce for rapid events
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    changed.store(false, Ordering::Relaxed);
                    rescan(&mut state, &vault_path);
                }
            }
        });
    });

    rsx! {
        div { class: "home-vault",
            super::vault_info_bar::VaultInfoBar { state }
            div { class: "vault-body",
                super::file_list::FileList { state }
                super::file_preview::FilePreview { state }
            }
            super::status_bar::StatusBar { state }
        }
    }
}
