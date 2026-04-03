//! File list panel showing vault contents sorted by modification time.

use dioxus::prelude::*;

use crate::state::AppState;

/// Left panel (280px) listing all vault files, sorted newest-first.
/// Clicking a file selects it. Footer button creates a new untitled file.
#[component]
pub fn FileList(mut state: Signal<AppState>) -> Element {
    let files = state.read().files.clone();
    let selected = state.read().selected_file.clone();

    rsx! {
        div { class: "file-list",
            div { class: "file-list-header", "FILES" }
            if files.is_empty() {
                div { class: "file-list-empty", "No files yet" }
            } else {
                div { class: "file-list-items",
                    for file in files {
                        {
                            let path = file.path.clone();
                            let path_click = path.clone();
                            let is_selected = selected.as_deref() == Some(path.as_str());
                            rsx! {
                                div {
                                    class: if is_selected { "file-item selected" } else { "file-item" },
                                    onclick: move |_| {
                                        state.write().selected_file = Some(path_click.clone());
                                    },
                                    div { class: "file-item-name", "{file.name}" }
                                    div { class: "file-item-meta", "{file.modified}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "file-list-footer",
                button {
                    class: "se-btn-ghost file-new-btn",
                    onclick: move |_| {
                        let vault_path = state.read().vault_path.clone();
                        let existing: Vec<String> = state.read().files.iter().map(|f| f.name.clone()).collect();
                        let name = unique_untitled_name(&existing);
                        let full_path = vault_path.join(&name);
                        if std::fs::write(&full_path, "").is_ok() {
                            state.write().selected_file = Some(name);
                        }
                    },
                    "+ New file"
                }
            }
        }
    }
}

/// Generate a unique "Untitled.md" name, avoiding conflicts with existing files.
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
