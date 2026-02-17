//! Note editor modal for viewing, editing, and creating notes.
//!
//! Three modes:
//! - View: Rendered/Raw toggle for markdown content
//! - Edit: Split view with textarea and live preview
//! - Create: Same as Edit but for new notes

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_ui::markdown::render_markdown_to_html;

use crate::state::{GenesisState, NoteEditorMode};

/// Parse a hex string back to a NoteId ([u8; 16]).
fn hex_to_note_id(hex: &str) -> Option<[u8; 16]> {
    if hex.len() != 32 {
        return None;
    }
    let mut id = [0u8; 16];
    for i in 0..16 {
        id[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(id)
}

/// Note editor modal overlay.
#[component]
pub fn NoteEditorOverlay(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: Option<[u8; 32]>,
) -> Element {
    let s = state.read();
    if !s.note_editor_open {
        return rsx! {};
    }

    let mode = s.note_editor_mode.clone();
    let title = s.note_editor_title.clone();
    let content = s.note_editor_content.clone();
    let preview_mode = s.note_editor_preview_mode;
    let note_id = s.note_editor_id.clone();
    drop(s);

    let is_view = mode == NoteEditorMode::View;
    let is_edit = mode == NoteEditorMode::Edit;
    let is_create = mode == NoteEditorMode::Create;

    let rendered_html = render_markdown_to_html(&content);

    let close_modal = move |_| {
        let mut s = state.write();
        s.note_editor_open = false;
        s.note_editor_id = None;
        s.note_editor_title.clear();
        s.note_editor_content.clear();
        s.note_editor_mode = NoteEditorMode::View;
        s.note_editor_preview_mode = true;
    };

    rsx! {
        div {
            class: "note-editor-overlay",
            onclick: close_modal,

            div {
                class: "note-editor-dialog",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "note-editor-header",

                    if is_view {
                        h2 { class: "note-editor-title", "{title}" }
                    } else {
                        input {
                            class: "genesis-input note-editor-title-input",
                            r#type: "text",
                            placeholder: "Note title...",
                            value: "{title}",
                            oninput: move |evt| {
                                state.write().note_editor_title = evt.value();
                            },
                        }
                    }

                    div {
                        class: "note-editor-controls",

                        if is_view {
                            button {
                                class: "note-editor-toggle",
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.note_editor_preview_mode = !s.note_editor_preview_mode;
                                },
                                if preview_mode { "View Raw" } else { "View Rendered" }
                            }

                            button {
                                class: "genesis-btn-secondary",
                                onclick: move |_| {
                                    state.write().note_editor_mode = NoteEditorMode::Edit;
                                },
                                "Edit"
                            }
                        }

                        button {
                            class: "note-editor-close",
                            onclick: close_modal,
                            "\u{00d7}"
                        }
                    }
                }

                // Content
                div {
                    class: "note-editor-content",

                    if is_view {
                        // View mode: rendered or raw
                        if preview_mode {
                            div {
                                class: "note-editor-rendered",
                                dangerous_inner_html: "{rendered_html}",
                            }
                        } else {
                            pre { class: "note-editor-raw", "{content}" }
                        }
                    } else {
                        // Edit/Create mode: split view
                        div {
                            class: "note-editor-split",

                            textarea {
                                class: "genesis-input note-editor-textarea",
                                placeholder: "Write your note in markdown...",
                                value: "{content}",
                                oninput: move |evt| {
                                    state.write().note_editor_content = evt.value();
                                },
                            }

                            div {
                                class: "note-editor-preview",
                                div {
                                    class: "note-editor-preview-label",
                                    "Preview"
                                }
                                div {
                                    class: "note-editor-rendered",
                                    dangerous_inner_html: "{rendered_html}",
                                }
                            }
                        }
                    }
                }

                // Footer
                if is_edit || is_create {
                    div {
                        class: "note-editor-footer",

                        button {
                            class: "genesis-btn-secondary",
                            onclick: move |_| {
                                if is_edit {
                                    // Cancel edit, go back to view
                                    // Reload original content from state
                                    let note_id_val = note_id.clone();
                                    if let Some(ref nid) = note_id_val {
                                        let notes = if peer_id.is_some() {
                                            state.read().peer_realm_notes.clone()
                                        } else {
                                            state.read().notes.clone()
                                        };
                                        if let Some(note) = notes.iter().find(|n| &n.id == nid) {
                                            let mut s = state.write();
                                            s.note_editor_title = note.title.clone();
                                            s.note_editor_content = note.content.clone();
                                            s.note_editor_mode = NoteEditorMode::View;
                                        }
                                    }
                                } else {
                                    // Cancel create, close modal
                                    let mut s = state.write();
                                    s.note_editor_open = false;
                                    s.note_editor_id = None;
                                    s.note_editor_title.clear();
                                    s.note_editor_content.clear();
                                    s.note_editor_mode = NoteEditorMode::View;
                                }
                            },
                            "Cancel"
                        }

                        button {
                            class: "genesis-btn-primary",
                            disabled: title.trim().is_empty(),
                            onclick: move |_| {
                                let title = state.read().note_editor_title.clone();
                                let content = state.read().note_editor_content.clone();
                                let note_id = state.read().note_editor_id.clone();
                                let is_create = state.read().note_editor_mode == NoteEditorMode::Create;

                                spawn(async move {
                                    let net = {
                                        let guard = network.read();
                                        guard.as_ref().cloned()
                                    };
                                    let Some(net) = net else {
                                        tracing::error!("Network not available for note operation");
                                        return;
                                    };

                                    if is_create {
                                        // Create new note
                                        if let Some(pid) = peer_id {
                                            // Peer realm
                                            let my_id = net.id();
                                            let dm_id = indras_network::artifact_sync::artifact_interface_id(&indras_network::dm_story_id(my_id, pid));
                                            if let Some(realm) = net.get_realm_by_id(&dm_id) {
                                                use indras_sync_engine::RealmNotes;
                                                match realm.create_note(title, content, my_id, vec![]).await {
                                                    Ok(_) => {
                                                        let mut s = state.write();
                                                        s.note_editor_open = false;
                                                        s.note_editor_id = None;
                                                        s.note_editor_title.clear();
                                                        s.note_editor_content.clear();
                                                        s.note_editor_mode = NoteEditorMode::View;
                                                        // Reload will happen via polling
                                                    }
                                                    Err(e) => tracing::error!("Failed to create note: {}", e),
                                                }
                                            }
                                        } else {
                                            // Home realm
                                            if let Ok(home) = net.home_realm().await {
                                                use indras_sync_engine::HomeRealmNotes;
                                                match home.create_note(title, content, vec![]).await {
                                                    Ok(_) => {
                                                        let mut s = state.write();
                                                        s.note_editor_open = false;
                                                        s.note_editor_id = None;
                                                        s.note_editor_title.clear();
                                                        s.note_editor_content.clear();
                                                        s.note_editor_mode = NoteEditorMode::View;
                                                    }
                                                    Err(e) => tracing::error!("Failed to create note: {}", e),
                                                }
                                            }
                                        }
                                    } else {
                                        // Update existing note
                                        if let Some(ref nid) = note_id {
                                            if let Some(id_bytes) = hex_to_note_id(nid) {
                                                if let Some(pid) = peer_id {
                                                    // Peer realm
                                                    let my_id = net.id();
                                                    let dm_id = indras_network::artifact_sync::artifact_interface_id(&indras_network::dm_story_id(my_id, pid));
                                                    if let Some(realm) = net.get_realm_by_id(&dm_id) {
                                                        use indras_sync_engine::RealmNotes;
                                                        match realm.update_note(id_bytes, content).await {
                                                            Ok(_) => {
                                                                let mut s = state.write();
                                                                s.note_editor_mode = NoteEditorMode::View;
                                                            }
                                                            Err(e) => tracing::error!("Failed to update note: {}", e),
                                                        }
                                                    }
                                                } else {
                                                    // Home realm
                                                    if let Ok(home) = net.home_realm().await {
                                                        use indras_sync_engine::HomeRealmNotes;
                                                        match home.update_note(id_bytes, content).await {
                                                            Ok(_) => {
                                                                let mut s = state.write();
                                                                s.note_editor_mode = NoteEditorMode::View;
                                                            }
                                                            Err(e) => tracing::error!("Failed to update note: {}", e),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                });
                            },
                            if is_create { "Create Note" } else { "Save" }
                        }
                    }
                }
            }
        }
    }
}
