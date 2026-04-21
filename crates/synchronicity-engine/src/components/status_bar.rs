//! Bottom status bar showing file count, total size, and last sync time.

use dioxus::prelude::*;

use crate::state::AppState;

/// Bottom bar with watch status (left), last sync time (center),
/// and file count + total size (right).
#[component]
pub fn StatusBar(mut state: Signal<AppState>) -> Element {
    let files = state.read().private_files.clone();
    let relay_count = state.read().relay_config.servers.len();
    let connected = 0_usize; // live count not yet wired
    let dot_class = if connected > 0 { "relay-chip-dot connected" } else { "relay-chip-dot" };
    // Count private + all realm files
    let realm_file_count: usize = state.read().realms.iter().map(|r| r.files.len()).sum();
    let file_count = files.len() + realm_file_count;
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    let size_label = format_size(total_bytes);

    // Use the most-recently-modified file's time as "last sync".
    let last_sync = files
        .first()
        .map(|f| f.modified.clone())
        .unwrap_or_else(|| "never".to_string());

    rsx! {
        div { class: "status-bar",
            span {
                class: "status-left muted status-relay-link",
                onclick: move |_| {
                    let cur = state.read().show_relay_settings;
                    state.write().show_relay_settings = !cur;
                },
                span { class: "{dot_class}", "\u{25cf}" }
                " Relay: {connected}/{relay_count} connected"
            }
            span {
                class: "status-left muted status-relay-link",
                onclick: move |_| {
                    let cur = state.read().show_recovery_setup;
                    state.write().show_recovery_setup = !cur;
                },
                title: if state.read().held_backups_count > 0 {
                    "You're holding backup pieces for other friends — click to manage your own backup"
                } else {
                    "Set up backup friends who can help you recover if you lose access"
                },
                " · Backup plan"
                if state.read().held_backups_count > 0 {
                    span { class: "status-held-badge",
                        " · holding {state.read().held_backups_count} for friends"
                    }
                }
            }
            span {
                class: "status-left muted status-relay-link",
                onclick: move |_| {
                    let cur = state.read().show_recovery_use;
                    state.write().show_recovery_use = !cur;
                },
                title: "Use the pieces your friends gave you to recover access",
                " · Use backup"
            }
            span {
                class: "status-left muted status-relay-link",
                onclick: move |_| {
                    let cur = state.read().show_steward_inbox;
                    state.write().show_steward_inbox = !cur;
                },
                title: "Requests from friends asking you to be their backup",
                " · Requests"
                if state.read().steward_inbox_pending > 0 {
                    span { class: "status-inbox-badge",
                        " · {state.read().steward_inbox_pending} waiting"
                    }
                }
            }
            span { class: "status-center muted", "Last sync: {last_sync}" }
            span { class: "status-right muted", "{file_count} file(s) · {size_label}" }
        }
    }
}

/// Format a byte count as a human-readable string (B, KB, MB).
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
