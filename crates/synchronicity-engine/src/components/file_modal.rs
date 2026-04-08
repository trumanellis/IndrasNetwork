//! File modal — popup overlay for viewing/editing a file.
//!
//! Shows editable title + rendered markdown body (click to edit).
//! Esc or clicking backdrop closes. Ctrl+Enter saves edits.

use dioxus::prelude::*;

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

    let mut editing = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut title_editing = use_signal(|| false);
    let mut title_draft = use_signal(String::new);

    // Read raw content from disk
    let raw_content = std::fs::read_to_string(&full_path).unwrap_or_default();
    let rendered = indras_ui::render_markdown_to_html(&raw_content);

    let close = move |_| {
        editing.set(false);
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

                // Editable title
                if *title_editing.read() {
                    input {
                        class: "doc-title-input",
                        r#type: "text",
                        value: "{title_draft}",
                        autofocus: true,
                        oninput: move |e| title_draft.set(e.value()),
                        onkeydown: {
                            let old_name = file_path.clone();
                            let vault_path = vault_path.clone();
                            move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    let new_title = title_draft.read().trim().to_string();
                                    if !new_title.is_empty() {
                                        let new_name = format!("{}.md", new_title);
                                        if new_name != old_name {
                                            let old_p = vault_path.join(&old_name);
                                            let new_p = vault_path.join(&new_name);
                                            if std::fs::rename(&old_p, &new_p).is_ok() {
                                                state.write().modal_file = Some(crate::state::ModalFile {
                                                    realm_id: None,
                                                    file_path: new_name.clone(),
                                                });
                                                state.write().selection.selected_file = Some(new_name);
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

                // Body — edit or view
                if *editing.read() {
                    div { class: "editor-actions",
                        span { class: "editor-hint", "Ctrl+Enter to save \u{00B7} Esc to cancel" }
                        button {
                            class: "se-btn-primary se-btn-sm",
                            onclick: {
                                let fp = file_path.clone();
                                let vp = vault_path.clone();
                                move |_| {
                                    let content = draft.read().clone();
                                    let _ = std::fs::write(vp.join(&fp), &content);
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
                            let fp = file_path.clone();
                            let vp = vault_path.clone();
                            move |e: KeyboardEvent| {
                                if (e.modifiers().meta() || e.modifiers().ctrl()) && e.key() == Key::Enter {
                                    e.prevent_default();
                                    let content = draft.read().clone();
                                    let _ = std::fs::write(vp.join(&fp), &content);
                                    editing.set(false);
                                }
                                if e.key() == Key::Escape {
                                    editing.set(false);
                                }
                            }
                        },
                    }
                } else {
                    div { class: "modal-body",
                        {
                            let raw = raw_content.clone();
                            let fp = file_path.clone();
                            let has_content = !rendered.trim().is_empty();
                            rsx! {
                                div {
                                    class: "preview-body preview-clickable",
                                    onclick: move |_| {
                                        draft.set(raw.clone());
                                        editing.set(true);
                                    },
                                    if has_content {
                                        div { dangerous_inner_html: "{rendered}" }
                                    } else {
                                        p { class: "preview-placeholder", "Click to start writing..." }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Global Esc handler
        div {
            tabindex: "0",
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape && !*editing.read() && !*title_editing.read() {
                    state.write().modal_file = None;
                }
            },
        }
    }
}
