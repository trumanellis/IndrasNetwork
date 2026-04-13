//! File modal — popup overlay for viewing/editing a file.
//!
//! Always-live inline editor: markdown renders as styled blocks and each block
//! becomes an inline textarea on click. No mode toggle, no save button.

use dioxus::prelude::*;

use super::markdown_editor::{obsidian_open_url, InlineMarkdownEditor};
use crate::state::AppState;

/// Strip the .md extension for display as a title.
fn title_from_filename(name: &str) -> String {
    name.strip_suffix(".md")
        .or_else(|| name.strip_suffix(".markdown"))
        .unwrap_or(name)
        .to_string()
}

/// Popup modal for viewing and editing a file.
#[component]
pub fn FileModal(mut state: Signal<AppState>) -> Element {
    let modal = state.read().modal_file.clone();
    let Some(modal) = modal else {
        return rsx! {};
    };

    let vault_path = state.read().vault_path.clone();
    let file_path = modal.file_path.clone();
    let full_path = vault_path.join(&file_path);

    let mut title_editing = use_signal(|| false);
    let mut title_draft = use_signal(String::new);

    let close = move |_| {
        title_editing.set(false);
        state.write().modal_file = None;
    };

    rsx! {
        div {
            class: "file-modal-overlay",
            onclick: close,

            div {
                class: "file-modal",
                onclick: move |e| e.stop_propagation(),

                // Header bar
                div { class: "file-modal-header",
                    // Editable title
                    if *title_editing.read() {
                        input {
                            class: "doc-title-input",
                            r#type: "text",
                            value: "{title_draft}",
                            autofocus: true,
                            oninput: move |e| title_draft.set(e.value()),
                            onblur: {
                                let old_name = file_path.clone();
                                let vault_path = vault_path.clone();
                                move |_| {
                                    commit_rename(&old_name, &vault_path, &title_draft.read(), state);
                                    title_editing.set(false);
                                }
                            },
                            onkeydown: {
                                let old_name = file_path.clone();
                                let vault_path = vault_path.clone();
                                move |e: KeyboardEvent| {
                                    if e.key() == Key::Enter {
                                        commit_rename(&old_name, &vault_path, &title_draft.read(), state);
                                        title_editing.set(false);
                                    }
                                    if e.key() == Key::Escape {
                                        title_editing.set(false);
                                    }
                                }
                            },
                        }
                    } else {
                        span {
                            class: "file-modal-title",
                            onclick: {
                                let fp = file_path.clone();
                                move |e: Event<MouseData>| {
                                    e.stop_propagation();
                                    title_draft.set(title_from_filename(&fp));
                                    title_editing.set(true);
                                }
                            },
                            "{title_from_filename(&file_path)}"
                        }
                    }

                    // Controls
                    div { class: "file-modal-controls",
                        button {
                            class: "md-editor-obsidian",
                            title: "Open this file in Obsidian",
                            onclick: {
                                let url = obsidian_open_url(&full_path);
                                move |_| { let _ = open::that_detached(&url); }
                            },
                            "Open in Obsidian"
                        }
                        button {
                            class: "file-modal-close",
                            onclick: close,
                            "\u{00d7}"
                        }
                    }
                }

                // Content area — always live, inline editable.
                div { class: "file-modal-content",
                    InlineMarkdownEditor { full_path: full_path.clone() }
                }
            }
        }
    }
}

fn commit_rename(
    old_name: &str,
    vault_path: &std::path::Path,
    new_title: &str,
    mut state: Signal<AppState>,
) {
    let new_title = new_title.trim();
    if new_title.is_empty() {
        return;
    }
    let new_name = format!("{}.md", new_title);
    if new_name == old_name {
        return;
    }
    let old_p = vault_path.join(old_name);
    let new_p = vault_path.join(&new_name);
    if std::fs::rename(&old_p, &new_p).is_ok() {
        state.write().modal_file = Some(crate::state::ModalFile {
            realm_id: None,
            file_path: new_name.clone(),
        });
        state.write().selection.selected_file = Some(new_name);
    }
}
