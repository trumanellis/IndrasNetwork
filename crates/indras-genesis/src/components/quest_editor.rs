//! Quest editor modal for viewing, editing, and creating quests.
//!
//! Three modes:
//! - View: Rendered markdown description with claims list
//! - Edit: Split view with textarea and live preview
//! - Create: Same as Edit but for new quests

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_ui::markdown::render_markdown_to_html;

use crate::state::{GenesisState, QuestEditorMode};

/// Parse a hex string back to a QuestId ([u8; 16]).
fn hex_to_quest_id(hex: &str) -> Option<[u8; 16]> {
    if hex.len() != 32 {
        return None;
    }
    let mut id = [0u8; 16];
    for i in 0..16 {
        id[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(id)
}

/// Quest editor modal overlay.
#[component]
pub fn QuestEditorOverlay(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: Option<[u8; 32]>,
) -> Element {
    let s = state.read();
    if !s.quest_editor_open {
        return rsx! {};
    }

    let mode = s.quest_editor_mode.clone();
    let title = s.quest_editor_title.clone();
    let description = s.quest_editor_description.clone();
    let preview_mode = s.quest_editor_preview_mode;
    let quest_id = s.quest_editor_id.clone();

    // Get claims for view mode
    let claims = if let Some(ref qid) = quest_id {
        if peer_id.is_some() {
            s.peer_realm_quests.iter()
                .find(|q| &q.id == qid)
                .map(|q| q.claims.clone())
                .unwrap_or_default()
        } else {
            s.quests.iter()
                .find(|q| &q.id == qid)
                .map(|q| q.claims.clone())
                .unwrap_or_default()
        }
    } else {
        Vec::new()
    };
    drop(s);

    let is_view = mode == QuestEditorMode::View;
    let is_edit = mode == QuestEditorMode::Edit;
    let is_create = mode == QuestEditorMode::Create;

    let rendered_html = render_markdown_to_html(&description);

    let close_modal = move |_| {
        let mut s = state.write();
        s.quest_editor_open = false;
        s.quest_editor_id = None;
        s.quest_editor_title.clear();
        s.quest_editor_description.clear();
        s.quest_editor_mode = QuestEditorMode::View;
        s.quest_editor_preview_mode = true;
    };

    rsx! {
        div {
            class: "quest-editor-overlay",
            onclick: close_modal,

            div {
                class: "quest-editor-dialog",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "quest-editor-header",

                    if is_view {
                        h2 { class: "quest-editor-title", "{title}" }
                    } else {
                        input {
                            class: "genesis-input quest-editor-title-input",
                            r#type: "text",
                            placeholder: "Quest title...",
                            value: "{title}",
                            oninput: move |evt| {
                                state.write().quest_editor_title = evt.value();
                            },
                        }
                    }

                    div {
                        class: "quest-editor-controls",

                        if is_view {
                            button {
                                class: "quest-editor-toggle",
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.quest_editor_preview_mode = !s.quest_editor_preview_mode;
                                },
                                if preview_mode { "View Raw" } else { "View Rendered" }
                            }

                            button {
                                class: "genesis-btn-secondary",
                                onclick: move |_| {
                                    state.write().quest_editor_mode = QuestEditorMode::Edit;
                                },
                                "Edit"
                            }
                        }

                        button {
                            class: "quest-editor-close",
                            onclick: close_modal,
                            "\u{00d7}"
                        }
                    }
                }

                // Content
                div {
                    class: "quest-editor-content",

                    if is_view {
                        // View mode: rendered or raw description
                        div {
                            class: "quest-editor-description-section",
                            h3 { class: "quest-editor-section-title", "Description" }
                            if preview_mode {
                                div {
                                    class: "quest-editor-rendered",
                                    dangerous_inner_html: "{rendered_html}",
                                }
                            } else {
                                pre { class: "quest-editor-raw", "{description}" }
                            }
                        }

                        // Claims section
                        if !claims.is_empty() {
                            div {
                                class: "quest-editor-claims-section",
                                h3 { class: "quest-editor-section-title", "Claims ({claims.len()})" }
                                div {
                                    class: "quest-editor-claims-list",
                                    for claim in claims.iter() {
                                        div {
                                            class: if claim.verified { "quest-editor-claim quest-editor-claim-verified" } else { "quest-editor-claim" },
                                            span { class: "quest-editor-claim-claimant", "{claim.claimant_name.clone().unwrap_or_else(|| claim.claimant_id_short.clone())}" }
                                            span { class: "quest-editor-claim-time", "{claim.submitted_at}" }
                                            if claim.has_proof {
                                                span { class: "quest-editor-claim-proof", "\u{1f4ce}" }
                                            }
                                            if claim.verified {
                                                span { class: "quest-editor-claim-verified-badge", "\u{2713} Verified" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Edit/Create mode: split view
                        div {
                            class: "quest-editor-split",

                            textarea {
                                class: "genesis-input quest-editor-textarea",
                                placeholder: "Write the quest description in markdown...",
                                value: "{description}",
                                oninput: move |evt| {
                                    state.write().quest_editor_description = evt.value();
                                },
                            }

                            div {
                                class: "quest-editor-preview",
                                div {
                                    class: "quest-editor-preview-label",
                                    "Preview"
                                }
                                div {
                                    class: "quest-editor-rendered",
                                    dangerous_inner_html: "{rendered_html}",
                                }
                            }
                        }
                    }
                }

                // Footer
                if is_edit || is_create {
                    div {
                        class: "quest-editor-footer",

                        button {
                            class: "genesis-btn-secondary",
                            onclick: move |_| {
                                if is_edit {
                                    // Cancel edit, go back to view
                                    let quest_id_val = quest_id.clone();
                                    if let Some(ref qid) = quest_id_val {
                                        let quests = if peer_id.is_some() {
                                            state.read().peer_realm_quests.clone()
                                        } else {
                                            state.read().quests.clone()
                                        };
                                        if let Some(quest) = quests.iter().find(|q| &q.id == qid) {
                                            let mut s = state.write();
                                            s.quest_editor_title = quest.title.clone();
                                            s.quest_editor_description = quest.description.clone();
                                            s.quest_editor_mode = QuestEditorMode::View;
                                        }
                                    }
                                } else {
                                    // Cancel create, close modal
                                    let mut s = state.write();
                                    s.quest_editor_open = false;
                                    s.quest_editor_id = None;
                                    s.quest_editor_title.clear();
                                    s.quest_editor_description.clear();
                                    s.quest_editor_mode = QuestEditorMode::View;
                                }
                            },
                            "Cancel"
                        }

                        button {
                            class: "genesis-btn-primary",
                            disabled: title.trim().is_empty(),
                            onclick: move |_| {
                                let title = state.read().quest_editor_title.clone();
                                let description = state.read().quest_editor_description.clone();
                                let quest_id = state.read().quest_editor_id.clone();
                                let is_create = state.read().quest_editor_mode == QuestEditorMode::Create;

                                spawn(async move {
                                    let net = {
                                        let guard = network.read();
                                        guard.as_ref().cloned()
                                    };
                                    let Some(net) = net else {
                                        tracing::error!("Network not available for quest operation");
                                        return;
                                    };

                                    if is_create {
                                        // Create new quest
                                        if let Some(pid) = peer_id {
                                            // Peer realm
                                            let my_id = net.id();
                                            let dm_id = indras_network::direct_connect::dm_realm_id(my_id, pid);
                                            if let Some(realm) = net.get_realm_by_id(&dm_id) {
                                                use indras_sync_engine::RealmQuests;
                                                match realm.create_quest(title, description, None, my_id).await {
                                                    Ok(_) => {
                                                        let mut s = state.write();
                                                        s.quest_editor_open = false;
                                                        s.quest_editor_id = None;
                                                        s.quest_editor_title.clear();
                                                        s.quest_editor_description.clear();
                                                        s.quest_editor_mode = QuestEditorMode::View;
                                                    }
                                                    Err(e) => tracing::error!("Failed to create quest: {}", e),
                                                }
                                            }
                                        } else {
                                            // Home realm
                                            if let Ok(home) = net.home_realm().await {
                                                use indras_sync_engine::HomeRealmQuests;
                                                match home.create_quest(title, description, None).await {
                                                    Ok(_) => {
                                                        let mut s = state.write();
                                                        s.quest_editor_open = false;
                                                        s.quest_editor_id = None;
                                                        s.quest_editor_title.clear();
                                                        s.quest_editor_description.clear();
                                                        s.quest_editor_mode = QuestEditorMode::View;
                                                    }
                                                    Err(e) => tracing::error!("Failed to create quest: {}", e),
                                                }
                                            }
                                        }
                                    } else {
                                        // Update existing quest
                                        if let Some(ref qid) = quest_id {
                                            if let Some(id_bytes) = hex_to_quest_id(qid) {
                                                if let Some(pid) = peer_id {
                                                    // Peer realm
                                                    let my_id = net.id();
                                                    let dm_id = indras_network::direct_connect::dm_realm_id(my_id, pid);
                                                    if let Some(realm) = net.get_realm_by_id(&dm_id) {
                                                        use indras_sync_engine::RealmQuests;
                                                        match realm.update_quest(id_bytes, title, description).await {
                                                            Ok(_) => {
                                                                let mut s = state.write();
                                                                s.quest_editor_mode = QuestEditorMode::View;
                                                            }
                                                            Err(e) => tracing::error!("Failed to update quest: {}", e),
                                                        }
                                                    }
                                                } else {
                                                    // Home realm
                                                    if let Ok(home) = net.home_realm().await {
                                                        use indras_sync_engine::HomeRealmQuests;
                                                        match home.update_quest(id_bytes, title, description).await {
                                                            Ok(_) => {
                                                                let mut s = state.write();
                                                                s.quest_editor_mode = QuestEditorMode::View;
                                                            }
                                                            Err(e) => tracing::error!("Failed to update quest: {}", e),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                });
                            },
                            if is_create { "Create Quest" } else { "Save" }
                        }
                    }
                }
            }
        }
    }
}
