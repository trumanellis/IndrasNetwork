//! Context menu shown on right-click over a file.

use dioxus::prelude::*;
use crate::state::{AppState, ModalFile};

/// Context menu popup positioned at click coordinates.
#[component]
pub fn ContextMenu(mut state: Signal<AppState>) -> Element {
    let menu = state.read().context_menu.clone();
    let Some(menu) = menu else {
        return rsx! {};
    };

    let vault_path = state.read().vault_path.clone();
    let file_path = menu.file_path.clone();
    let realm_id = menu.realm_id;

    let close = move |_| {
        state.write().context_menu = None;
    };

    rsx! {
        div {
            class: "context-menu-overlay",
            onclick: close,

            div {
                class: "context-menu",
                style: "left: {menu.x}px; top: {menu.y}px;",
                onclick: move |e| e.stop_propagation(),

                // Open
                div {
                    class: "context-menu-item",
                    onclick: {
                        let fp = file_path.clone();
                        move |_| {
                            state.write().modal_file = Some(ModalFile {
                                realm_id,
                                file_path: fp.clone(),
                            });
                            state.write().context_menu = None;
                        }
                    },
                    span { class: "context-menu-item-icon", "📄" }
                    span { "Open" }
                    span { class: "context-menu-item-shortcut", "Space" }
                }

                // Rename
                div {
                    class: "context-menu-item",
                    onclick: {
                        let fp = file_path.clone();
                        move |_| {
                            state.write().renaming_file = Some(fp.clone());
                            state.write().context_menu = None;
                        }
                    },
                    span { class: "context-menu-item-icon", "✏️" }
                    span { "Rename" }
                    span { class: "context-menu-item-shortcut", "F2" }
                }

                // Duplicate (private only)
                if realm_id.is_none() {
                    div {
                        class: "context-menu-item",
                        onclick: {
                            let fp = file_path.clone();
                            let vp = vault_path.clone();
                            move |_| {
                                let src = vp.join(&fp);
                                if let Ok(content) = std::fs::read_to_string(&src) {
                                    let new_name = format!("{} copy.md",
                                        fp.strip_suffix(".md").unwrap_or(&fp));
                                    let dest = vp.join(&new_name);
                                    let _ = std::fs::write(&dest, &content);
                                }
                                state.write().context_menu = None;
                            }
                        },
                        span { class: "context-menu-item-icon", "📋" }
                        span { "Duplicate" }
                    }
                }

                div { class: "context-menu-divider" }

                // Delete
                if realm_id.is_none() {
                    div {
                        class: "context-menu-item danger",
                        onclick: {
                            let fp = file_path.clone();
                            let vp = vault_path.clone();
                            move |_| {
                                let path = vp.join(&fp);
                                let _ = std::fs::remove_file(&path);
                                // Clear selection if this was the selected file
                                if state.read().selection.selected_file.as_deref() == Some(fp.as_str()) {
                                    state.write().selection.selected_file = None;
                                }
                                state.write().context_menu = None;
                            }
                        },
                        span { class: "context-menu-item-icon", "🗑" }
                        span { "Delete" }
                        span { class: "context-menu-item-shortcut", "⌘⌫" }
                    }
                }
            }
        }
    }
}
