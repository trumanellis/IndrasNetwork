//! File preview/edit panel — inline markdown editing with editable title.
//!
//! The filename appears as an editable title at the top of the document.
//! Click the rendered content to switch to a full-height textarea editor.

use dioxus::prelude::*;

use crate::state::AppState;

/// Strip the .md extension for display as a title.
fn title_from_filename(name: &str) -> String {
    name.strip_suffix(".md")
        .or_else(|| name.strip_suffix(".markdown"))
        .unwrap_or(name)
        .to_string()
}

/// Right panel — document title + rendered markdown with click-to-edit.
#[component]
pub fn FilePreview(mut state: Signal<AppState>) -> Element {
    let selected = state.read().selected_file.clone();
    let vault_path = state.read().vault_path.clone();

    let mut editing = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut draft_file = use_signal(|| Option::<String>::None);
    let mut title_draft = use_signal(String::new);
    let mut title_editing = use_signal(|| false);

    // Read raw content from disk
    let raw_content = selected.as_ref().and_then(|name| {
        let full_path = vault_path.join(name);
        std::fs::read_to_string(&full_path).ok()
    });

    // If selected file changed, exit edit modes
    if draft_file.read().as_deref() != selected.as_deref() {
        if *editing.read() { editing.set(false); }
        if *title_editing.read() { title_editing.set(false); }
    }

    let rendered = raw_content.as_ref().map(|md| indras_ui::render_markdown_to_html(md));

    rsx! {
        div { class: "file-preview",
            if let Some(ref name) = selected {
                div { class: "document",

                    // Title — always visible, always clickable to rename
                    if *title_editing.read() {
                        input {
                            class: "doc-title-input",
                            r#type: "text",
                            value: "{title_draft}",
                            autofocus: true,
                            oninput: move |e| title_draft.set(e.value()),
                            onkeydown: {
                                let old_name = name.clone();
                                let vault_path = vault_path.clone();
                                move |e: KeyboardEvent| {
                                    if e.key() == Key::Enter {
                                        let new_title = title_draft.read().trim().to_string();
                                        if !new_title.is_empty() {
                                            let new_name = format!("{}.md", new_title);
                                            if new_name != old_name {
                                                let old_path = vault_path.join(&old_name);
                                                let new_path = vault_path.join(&new_name);
                                                if std::fs::rename(&old_path, &new_path).is_ok() {
                                                    state.write().selected_file = Some(new_name);
                                                }
                                            }
                                        }
                                        title_editing.set(false);
                                    }
                                    if e.key() == Key::Escape {
                                        title_editing.set(false);
                                    }
                                }
                            },
                        }
                    } else {
                        h1 {
                            class: "doc-title",
                            onclick: {
                                let name = name.clone();
                                move |_| {
                                    title_draft.set(title_from_filename(&name));
                                    title_editing.set(true);
                                }
                            },
                            "{title_from_filename(name)}"
                        }
                    }

                    // Body — edit or view mode
                    if *editing.read() {
                        div { class: "editor-actions",
                            span { class: "editor-hint", "Ctrl+Enter to save \u{00B7} Esc to cancel" }
                            button {
                                class: "se-btn-primary se-btn-sm",
                                onclick: {
                                    let name = name.clone();
                                    let vault_path = vault_path.clone();
                                    move |_| {
                                        let content = draft.read().clone();
                                        let path = vault_path.join(&name);
                                        let _ = std::fs::write(&path, &content);
                                        editing.set(false);
                                    }
                                },
                                "Done"
                            }
                        }
                        textarea {
                            class: "editor-full",
                            value: "{draft}",
                            autofocus: true,
                            oninput: move |e| draft.set(e.value()),
                            onkeydown: {
                                let name = name.clone();
                                let vault_path = vault_path.clone();
                                move |e: KeyboardEvent| {
                                    if (e.modifiers().meta() || e.modifiers().ctrl()) && e.key() == Key::Enter {
                                        e.prevent_default();
                                        let content = draft.read().clone();
                                        let path = vault_path.join(&name);
                                        let _ = std::fs::write(&path, &content);
                                        editing.set(false);
                                    }
                                    if e.key() == Key::Escape {
                                        editing.set(false);
                                    }
                                }
                            },
                        }
                    } else {
                        {
                            let name = name.clone();
                            let raw = raw_content.clone().unwrap_or_default();
                            let html = rendered.clone().unwrap_or_default();
                            let has_content = !html.trim().is_empty();
                            rsx! {
                                div {
                                    class: "preview-body preview-clickable",
                                    onclick: move |_| {
                                        draft.set(raw.clone());
                                        draft_file.set(Some(name.clone()));
                                        editing.set(true);
                                    },
                                    if has_content {
                                        div { dangerous_inner_html: "{html}" }
                                    } else {
                                        p { class: "preview-placeholder", "Click to start writing..." }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "preview-empty",
                    "Select a file to preview"
                }
            }
        }
    }
}
