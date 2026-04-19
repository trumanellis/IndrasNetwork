//! File modal — popup overlay for viewing/editing a file.
//!
//! Always-live inline editor: markdown renders as styled blocks and each block
//! becomes an inline textarea on click. No mode toggle, no save button.

use std::sync::Arc;

use dioxus::prelude::*;

use super::markdown_editor::{obsidian_open_url, InlineMarkdownEditor};
use super::obsidian::{is_vault_registered, quit_obsidian, register_vault};
use crate::state::AppState;
use crate::vault_manager::VaultManager;

/// Status of the last sync attempt, shown briefly next to the button.
#[derive(Clone, Debug, PartialEq)]
enum SyncStatus {
    Idle,
    Syncing,
    Done(String),
    Failed(String),
}

/// Strip the .md extension for display as a title.
fn title_from_filename(name: &str) -> String {
    name.strip_suffix(".md")
        .or_else(|| name.strip_suffix(".markdown"))
        .unwrap_or(name)
        .to_string()
}

/// Popup modal for viewing and editing a file.
#[component]
pub fn FileModal(
    mut state: Signal<AppState>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
) -> Element {
    let modal = state.read().modal_file.clone();
    let Some(modal) = modal else {
        return rsx! {};
    };

    // Resolve the correct vault directory for this file. Files from a shared
    // realm (DM, group, world) live in that realm's vault dir, not the user's
    // private vault. Falling back to the private vault here caused edits to
    // shared files to silently land in the wrong directory, so they never
    // reached VaultWatcher → send_message → other peers.
    let vault_path = match modal.realm_id {
        Some(rid) => vault_manager
            .read()
            .as_ref()
            .and_then(|vm| vm.vault_path(&rid))
            .unwrap_or_else(|| state.read().vault_path.clone()),
        None => state.read().vault_path.clone(),
    };
    let file_path = modal.file_path.clone();
    let full_path = vault_path.join(&file_path);

    let mut title_editing = use_signal(|| false);
    let mut title_draft = use_signal(String::new);
    let mut vault_registered = use_signal(|| is_vault_registered(&vault_path));
    let mut sync_status = use_signal(|| SyncStatus::Idle);

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
                        // Sync button — pushes local changes through the braid DAG
                        {
                            let realm_id = modal.realm_id;
                            let fp = file_path.clone();
                            let is_syncing = matches!(*sync_status.read(), SyncStatus::Syncing);
                            rsx! {
                                button {
                                    class: "md-editor-sync",
                                    title: "Sync this file to peers via the braid DAG",
                                    disabled: is_syncing,
                                    onclick: move |_| {
                                        let vm = vault_manager;
                                        let fp = fp.clone();
                                        sync_status.set(SyncStatus::Syncing);
                                        spawn(async move {
                                            let result = if let Some(vm) = vm.read().as_ref() {
                                                // Determine which realm to sync. Private vault
                                                // files use the home realm (first realm).
                                                let rid = match realm_id {
                                                    Some(rid) => rid,
                                                    None => {
                                                        // Private vault — find home realm
                                                        match vm.realms().await.first() {
                                                            Some(r) => *r.id().as_bytes(),
                                                            None => {
                                                                sync_status.set(SyncStatus::Failed("no realm".into()));
                                                                return;
                                                            }
                                                        }
                                                    }
                                                };
                                                vm.sync_vault(
                                                    &rid,
                                                    format!("sync {fp}"),
                                                    None,
                                                ).await
                                            } else {
                                                Err("vault manager not ready".into())
                                            };
                                            match result {
                                                Ok(id) => {
                                                    let short: String = id.as_bytes().iter().take(4).map(|b| format!("{b:02x}")).collect();
                                                    sync_status.set(SyncStatus::Done(short));
                                                }
                                                Err(e) => sync_status.set(SyncStatus::Failed(e)),
                                            }
                                        });
                                    },
                                    match &*sync_status.read() {
                                        SyncStatus::Syncing => "Syncing...",
                                        SyncStatus::Done(_) => "Synced",
                                        SyncStatus::Failed(_) => "Sync",
                                        SyncStatus::Idle => "Sync",
                                    }
                                }
                            }
                        }
                        if *vault_registered.read() {
                            button {
                                class: "md-editor-obsidian",
                                title: "Open this file in Obsidian",
                                onclick: {
                                    let url = obsidian_open_url(&full_path);
                                    move |_| { let _ = open::that_detached(&url); }
                                },
                                "Open in Obsidian"
                            }
                        } else {
                            button {
                                class: "md-editor-obsidian",
                                title: "Register this folder with Obsidian, then open",
                                onclick: {
                                    let vp = vault_path.clone();
                                    let url = obsidian_open_url(&full_path);
                                    move |_| {
                                        // Quit any running Obsidian first — it caches its
                                        // vault list in memory at launch, so editing
                                        // obsidian.json while it is running has no effect
                                        // on the live instance.
                                        quit_obsidian();
                                        std::thread::sleep(std::time::Duration::from_millis(600));
                                        match register_vault(&vp) {
                                            Ok(()) => {
                                                vault_registered.set(true);
                                                let _ = open::that_detached(&url);
                                            }
                                            Err(e) => {
                                                tracing::warn!(error = %e, "failed to register Obsidian vault");
                                            }
                                        }
                                    }
                                },
                                "Create Obsidian Vault"
                            }
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
        // Preserve the realm_id so the modal stays pointed at the right vault.
        let realm_id = state.read().modal_file.as_ref().and_then(|m| m.realm_id);
        state.write().modal_file = Some(crate::state::ModalFile {
            realm_id,
            file_path: new_name.clone(),
        });
        state.write().selection.selected_file = Some(new_name);
    }
}
