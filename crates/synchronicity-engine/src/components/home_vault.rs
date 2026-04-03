//! Main vault view — file list, preview, and info bar.

use dioxus::prelude::*;

use crate::state::AppState;
use crate::vault_bridge::scan_vault;

/// Main vault view orchestrator. Three-zone layout: info bar (top),
/// file list + preview (body), status bar (bottom).
#[component]
pub fn HomeVault(mut state: Signal<AppState>) -> Element {
    // On mount: start periodic vault scan every 2 seconds.
    use_effect(move || {
        let vault_path = state.read().vault_path.clone();
        spawn(async move {
            loop {
                let files = scan_vault(&vault_path);
                // Auto-select first file on initial load if nothing is selected.
                let should_auto_select = state.read().selected_file.is_none() && !files.is_empty();
                let first_path = files.first().map(|f| f.path.clone());
                state.write().files = files;
                if should_auto_select {
                    if let Some(path) = first_path {
                        state.write().selected_file = Some(path);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    });

    // When selected_file changes, read content from disk and render to HTML.
    let selected = state.read().selected_file.clone();
    use_effect(move || {
        let selected = state.read().selected_file.clone();
        if let Some(ref path) = selected {
            let vault_path = state.read().vault_path.clone();
            let full_path = vault_path.join(path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let html = indras_ui::render_markdown_to_html(&content);
                state.write().selected_content = Some(html);
            }
        } else {
            state.write().selected_content = None;
        }
    });

    // Suppress unused variable warning — selected is used as use_effect dependency.
    let _ = selected;

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
