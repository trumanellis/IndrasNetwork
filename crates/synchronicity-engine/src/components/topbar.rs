//! Top app bar — brand · breadcrumb · view toggle · sync action.
//!
//! Matches the layout of `design/braid-prototype.html` `.topbar`. The
//! view toggle is purely visual for now (Loom view isn't built yet);
//! clicking Loom sets a local mode so the visual state feels real.

use std::sync::LazyLock;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use dioxus::prelude::*;

use crate::state::{AppState, SyncStatus};

const LOGO_PNG: &[u8] = include_bytes!("../../../../assets/Logo_black.png");

static LOGO_DATA_URL: LazyLock<String> =
    LazyLock::new(|| format!("data:image/png;base64,{}", B64.encode(LOGO_PNG)));

/// Visual view mode — Dashboard is the 4-column layout; Loom is a
/// future full-viewport braid. Only Dashboard renders content today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Dashboard,
    Loom,
}

/// Top bar: brand on the left, breadcrumb next, then a Dashboard/Loom
/// toggle and an informative sync action button on the right.
#[component]
pub fn Topbar(mut state: Signal<AppState>, display_name: String) -> Element {
    let mut view_mode = use_signal(|| ViewMode::Dashboard);

    // Resolve breadcrumb: vaults / <self> / <realm-if-selected>
    let selected_realm = state.read().selection.selected_realm;
    let realm_name = selected_realm.and_then(|rid| {
        state
            .read()
            .realms
            .iter()
            .find(|r| r.id == rid)
            .map(|r| r.display_name.clone())
    });

    // Sync-button sub-label (short, monospace).
    let sync_sub = match state.read().sync_status.clone() {
        SyncStatus::Synced => "up to date".to_string(),
        SyncStatus::Syncing => "syncing…".to_string(),
        SyncStatus::Offline => "offline".to_string(),
        SyncStatus::Error(_) => "error".to_string(),
    };
    let sync_active = matches!(state.read().sync_status, SyncStatus::Syncing);

    let show_name = if display_name.trim().is_empty() {
        None
    } else {
        Some(display_name.clone())
    };

    rsx! {
        div { class: "topbar",
            span { class: "brand",
                img { class: "brand-logo", src: "{&*LOGO_DATA_URL}", alt: "" }
                "Synchronicity Engine"
            }
            div { class: "breadcrumb",
                span { "vaults" }
                span { class: "crumb-sep", "/" }
                if let Some(name) = show_name {
                    span {
                        class: if realm_name.is_some() { "" } else { "crumb-active" },
                        "{name}"
                    }
                }
                if let Some(name) = realm_name {
                    span { class: "crumb-sep", "/" }
                    span { class: "crumb-active", "{name}" }
                }
            }
            div { class: "topbar-spacer" }
            div { class: "view-toggle",
                button {
                    class: if *view_mode.read() == ViewMode::Dashboard { "active" } else { "" },
                    onclick: move |_| view_mode.set(ViewMode::Dashboard),
                    "Dashboard"
                }
                button {
                    class: if *view_mode.read() == ViewMode::Loom { "active" } else { "" },
                    title: "Loom view — coming soon",
                    onclick: move |_| view_mode.set(ViewMode::Loom),
                    "Loom"
                }
            }
            button {
                class: if sync_active { "sync-action active" } else { "sync-action" },
                title: "Open sync panel",
                onclick: move |_| {
                    state.write().show_sync = true;
                },
                span { class: "sync-icon" }
                span { "Sync" }
                span { class: "sync-sub", "{sync_sub}" }
            }
        }
    }
}
