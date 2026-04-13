//! Top info bar showing breadcrumb path, Open Vault Folder button, and sync status.

use dioxus::prelude::*;

use crate::state::{AppState, SyncStatus};

/// Top bar with breadcrumb path (left), Open Vault Folder button (center),
/// and sync status + device count (right).
#[component]
pub fn VaultInfoBar(state: Signal<AppState>) -> Element {
    let vault_path = state.read().vault_path.clone();
    let device_count = state.read().device_count;
    let sync_status = state.read().sync_status.clone();

    let (sync_label, sync_dot_class) = match &sync_status {
        SyncStatus::Synced => ("Synced".to_string(), "sync-dot synced"),
        SyncStatus::Syncing => ("Syncing...".to_string(), "sync-dot syncing"),
        SyncStatus::Offline => ("Offline".to_string(), "sync-dot offline"),
        SyncStatus::Error(e) => (format!("Error: {}", e), "sync-dot error"),
    };

    let focused_column = state.read().selection.focused_column;
    let selected_realm = state.read().selection.selected_realm;
    let selected_file = state.read().selection.selected_file.clone();
    let realm_name = selected_realm.and_then(|rid| {
        state.read().realms.iter()
            .find(|r| r.id == rid)
            .map(|r| r.display_name.clone())
    });

    rsx! {
        div { class: "vault-info-bar",
            // Left: breadcrumb path
            div { class: "vault-info-left",
                div { class: "breadcrumb-bar",
                    span { class: "breadcrumb-segment",
                        match focused_column {
                            0 => "Private",
                            1 => "Direct",
                            2 => "Groups",
                            3 => "World",
                            _ => "Private",
                        }
                    }
                    if let Some(name) = realm_name {
                        span { class: "breadcrumb-sep", "\u{203A}" }
                        span { class: "breadcrumb-segment", "{name}" }
                    }
                    if let Some(ref file) = selected_file {
                        span { class: "breadcrumb-sep", "\u{203A}" }
                        span { class: "breadcrumb-segment active", "{file}" }
                    }
                }
            }
            // Center: Open in Obsidian
            div { class: "vault-info-center",
                button {
                    class: "se-btn-outline se-btn-sm",
                    onclick: move |_| {
                        let _ = open::that(vault_path.parent().unwrap_or(&*vault_path));
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
