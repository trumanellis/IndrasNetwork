//! Private column — the user's personal vault files.

use dioxus::prelude::*;

use crate::state::{AppState, ContextMenu, DragPayload, ModalFile};
use super::file_item::FileItem;

/// Column 1: private vault files with "+ New" button.
#[component]
pub fn PrivateColumn(mut state: Signal<AppState>) -> Element {
    let files = state.read().private_files.clone();
    let selected = state.read().selection.selected_file.clone();
    let is_private_selected = state.read().selection.selected_realm.is_none();
    let vault_path = state.read().vault_path.clone();
    let display_name = state.read().display_name.clone();
    let header_label = if display_name.trim().is_empty() {
        "PRIVATE".to_string()
    } else {
        display_name.clone()
    };

    rsx! {
        div { class: "vault-column",
            div { class: "column-header",
                span {
                    class: "column-header-label glow-private",
                    title: "Edit profile",
                    onclick: move |_| {
                        state.write().show_profile = true;
                    },
                    "{header_label}"
                }
                button {
                    class: "column-header-folder glow-private",
                    title: "Open vault folder",
                    onclick: move |_| {
                        let vault = state.read().vault_path.clone();
                        let _ = open::that(vault.parent().unwrap_or(&vault));
                    },
                    "\u{1F4C1}"
                }
                button {
                    class: "column-header-add glow-private",
                    title: "New File",
                    onclick: move |_| {
                        let vault_path = state.read().vault_path.clone();
                        let existing: Vec<String> = state.read().private_files.iter().map(|f| f.name.clone()).collect();
                        let name = unique_untitled_name(&existing);
                        let full_path = vault_path.join(&name);
                        if std::fs::write(&full_path, "").is_ok() {
                            state.write().selection.selected_realm = None;
                            state.write().selection.selected_file = Some(name.clone());
                            state.write().modal_file = Some(ModalFile {
                                realm_id: None,
                                file_path: name,
                            });
                        }
                    },
                    "+"
                }
            }
            div { class: "vault-column-body",
                if files.is_empty() {
                    div { class: "column-empty",
                        div { class: "column-empty-icon", "🏠" }
                        div { class: "column-empty-text", "Your private vault is empty" }
                    }
                } else {
                    for file in files {
                        {
                            let path = file.path.clone();
                            let is_sel = is_private_selected && selected.as_deref() == Some(path.as_str());
                            let disk_path = vault_path.join(&file.path);
                            rsx! {
                                FileItem {
                                    file: file,
                                    is_selected: is_sel,
                                    file_disk_path: Some(disk_path),
                                    source_realm: None::<[u8; 32]>,
                                    on_drag_start: move |payload: DragPayload| {
                                        state.write().drag_payload = Some(payload);
                                    },
                                    on_drag_end: move |_| {
                                        state.write().drag_payload = None;
                                        state.write().drop_target_realm = None;
                                    },
                                    on_click: move |p: String| {
                                        state.write().selection.selected_realm = None;
                                        state.write().selection.selected_file = Some(p.clone());
                                        state.write().modal_file = Some(ModalFile {
                                            realm_id: None,
                                            file_path: p,
                                        });
                                    },
                                    on_context_menu: move |(p, x, y): (String, f64, f64)| {
                                        state.write().context_menu = Some(ContextMenu {
                                            realm_id: None,
                                            file_path: p,
                                            x,
                                            y,
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Generate a unique "Untitled.md" name.
fn unique_untitled_name(existing: &[String]) -> String {
    if !existing.contains(&"Untitled.md".to_string()) {
        return "Untitled.md".to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("Untitled {}.md", n);
        if !existing.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}
