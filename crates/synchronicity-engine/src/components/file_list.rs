//! File list panel showing vault contents sorted by modification time.
//!
//! Supports click to select and double-click to rename.

use dioxus::prelude::*;

use crate::state::AppState;

/// Left panel (280px) listing all vault files, sorted newest-first.
/// Click to select, double-click name to rename. Footer button creates new files.
#[component]
pub fn FileList(mut state: Signal<AppState>) -> Element {
    let files = state.read().files.clone();
    let selected = state.read().selected_file.clone();

    let mut renaming: Signal<Option<String>> = use_signal(|| None);
    let mut rename_draft = use_signal(String::new);

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
                            let path_dblclick = path.clone();
                            let is_selected = selected.as_deref() == Some(path.as_str());
                            let is_renaming = renaming.read().as_deref() == Some(path.as_str());

                            rsx! {
                                div {
                                    class: if is_selected { "file-item selected" } else { "file-item" },
                                    onclick: move |_| {
                                        if renaming.read().is_none() {
                                            state.write().selected_file = Some(path_click.clone());
                                        }
                                    },

                                    if is_renaming {
                                        input {
                                            class: "file-rename-input",
                                            r#type: "text",
                                            value: "{rename_draft}",
                                            autofocus: true,
                                            oninput: move |e| rename_draft.set(e.value()),
                                            onkeydown: {
                                                let old_name = path.clone();
                                                move |e: KeyboardEvent| {
                                                    if e.key() == Key::Enter {
                                                        let new_name = rename_draft.read().trim().to_string();
                                                        if !new_name.is_empty() && new_name != old_name {
                                                            let vault_path = state.read().vault_path.clone();
                                                            let old_path = vault_path.join(&old_name);
                                                            // Ensure .md extension
                                                            let new_name = if new_name.ends_with(".md") {
                                                                new_name
                                                            } else {
                                                                format!("{}.md", new_name)
                                                            };
                                                            let new_path = vault_path.join(&new_name);
                                                            if std::fs::rename(&old_path, &new_path).is_ok() {
                                                                // Update selection to new name
                                                                state.write().selected_file = Some(new_name);
                                                            }
                                                        }
                                                        renaming.set(None);
                                                    }
                                                    if e.key() == Key::Escape {
                                                        renaming.set(None);
                                                    }
                                                }
                                            },
                                        }
                                    } else {
                                        div {
                                            class: "file-item-name",
                                            ondoubleclick: {
                                                let name = file.name.clone();
                                                move |_| {
                                                    rename_draft.set(name.clone());
                                                    renaming.set(Some(path_dblclick.clone()));
                                                }
                                            },
                                            "{file.name}"
                                        }
                                    }

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
