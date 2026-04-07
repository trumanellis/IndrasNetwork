//! Top info bar showing vault path, Obsidian button, and sync status.

use dioxus::prelude::*;

use crate::state::{AppState, SyncStatus};

/// Top bar with vault path (left), Open in Obsidian button (center),
/// and sync status + device count (right).
#[component]
pub fn VaultInfoBar(state: Signal<AppState>) -> Element {
    let vault_path = state.read().vault_path.clone();
    let vault_path_str = vault_path.display().to_string();
    let device_count = state.read().device_count;
    let sync_status = state.read().sync_status.clone();

    let (sync_label, sync_dot_class) = match &sync_status {
        SyncStatus::Synced => ("Synced".to_string(), "sync-dot synced"),
        SyncStatus::Syncing => ("Syncing...".to_string(), "sync-dot syncing"),
        SyncStatus::Offline => ("Offline".to_string(), "sync-dot offline"),
        SyncStatus::Error(e) => (format!("Error: {}", e), "sync-dot error"),
    };

    rsx! {
        div { class: "vault-info-bar",
            // Left: vault path
            div { class: "vault-info-left",
                span { class: "vault-path-icon", "📁" }
                span {
                    class: "vault-path mono",
                    title: "{vault_path_str}",
                    "{vault_path_str}"
                }
            }
            // Center: Open in Obsidian
            div { class: "vault-info-center",
                button {
                    class: "se-btn-outline se-btn-sm",
                    onclick: move |_| {
                        let _ = open::that(&*vault_path);
                    },
                    "Open Vault Folder"
                }
            }
            // Right: sync status + device count
            div { class: "vault-info-right",
                span { class: sync_dot_class }
                span { class: "sync-label", "{sync_label}" }
                span { class: "device-count muted", " · {device_count} device(s)" }
            }
        }
    }
}
