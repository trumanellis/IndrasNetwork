//! Private column — the user's personal vault files.

use dioxus::prelude::*;

use crate::state::{AppState, ModalFile};
use super::file_item::FileItem;

/// Column 1: private vault files with "+ New" button.
#[component]
pub fn PrivateColumn(mut state: Signal<AppState>) -> Element {
    let files = state.read().private_files.clone();
    let selected = state.read().selection.selected_file.clone();
    let is_private_selected = state.read().selection.selected_realm.is_none();

    rsx! {
        div { class: "vault-column",
            div { class: "column-header", "PRIVATE" }
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
                            rsx! {
                                FileItem {
                                    file: file,
                                    is_selected: is_sel,
                                    on_click: move |p: String| {
                                        state.write().selection.selected_realm = None;
                                        state.write().selection.selected_file = Some(p.clone());
                                        state.write().modal_file = Some(ModalFile {
                                            realm_id: None,
                                            file_path: p,
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }
            div { class: "column-footer",
                button {
                    class: "se-btn-ghost file-new-btn",
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
                    "+ New"
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
