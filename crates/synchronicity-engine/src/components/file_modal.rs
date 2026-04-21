//! File modal — popup overlay for viewing/editing a file.
//!
//! Always-live inline editor: markdown renders as styled blocks and each block
//! becomes an inline textarea on click. No mode toggle, no save button.
//! Includes a Sync button that shows commit/pull/merge progress.

use std::sync::Arc;

use dioxus::prelude::*;

use super::markdown_editor::{obsidian_open_url, InlineMarkdownEditor};
use super::obsidian::{is_vault_registered, quit_obsidian, register_vault};
use crate::state::AppState;
use crate::vault_manager::{SyncStep, VaultManager};

/// Strip the .md extension for display as a title.
fn title_from_filename(name: &str) -> String {
    name.strip_suffix(".md")
        .or_else(|| name.strip_suffix(".markdown"))
        .unwrap_or(name)
        .to_string()
}

/// Render the current sync step as a user-visible string.
fn step_text(step: &SyncStep) -> &'static str {
    match step {
        SyncStep::Checking => "Checking...",
        SyncStep::Committing { .. } => "Committing...",
        SyncStep::Committed { .. } => "Committed",
        SyncStep::Pulling => "Pulling...",
        SyncStep::PeerForks { .. } => "Forks found",
        SyncStep::Merged { .. } => "Merged",
        SyncStep::Done { .. } => "Done",
        SyncStep::NothingToSync => "Up to date",
        SyncStep::Failed(_) => "Failed",
    }
}

/// CSS class suffix for the current step (drives color).
fn step_class(step: &SyncStep) -> &'static str {
    match step {
        SyncStep::Checking | SyncStep::Pulling => "active",
        SyncStep::Committing { .. } | SyncStep::Merged { .. } => "active",
        SyncStep::Committed { .. } | SyncStep::Done { .. } => "done",
        SyncStep::NothingToSync => "idle",
        SyncStep::PeerForks { .. } => "info",
        SyncStep::Failed(_) => "fail",
    }
}

/// Detail text for richer steps.
fn step_detail(step: &SyncStep) -> Option<String> {
    match step {
        SyncStep::Committing { dirty_count } => Some(format!("{dirty_count} files")),
        SyncStep::Committed { change_id } => Some(change_id.clone()),
        SyncStep::PeerForks { count } => Some(format!("{count} available")),
        SyncStep::Merged { peer, change_id } => Some(format!("{peer} -> {change_id}")),
        SyncStep::Done { summary } => Some(summary.clone()),
        SyncStep::Failed(msg) => Some(msg.clone()),
        _ => None,
    }
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
    let mut sync_step: Signal<Option<SyncStep>> = use_signal(|| None);

    let close = move |_| {
        title_editing.set(false);
        state.write().modal_file = None;
    };

    let is_syncing = sync_step
        .read()
        .as_ref()
        .map_or(false, |s| matches!(s, SyncStep::Checking | SyncStep::Committing { .. } | SyncStep::Pulling | SyncStep::Merged { .. }));

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
                        // ── Sync button + status ──
                        {
                            let realm_id = modal.realm_id;
                            let fp = file_path.clone();
                            rsx! {
                                div { class: "sync-inline",
                                    button {
                                        class: "md-editor-sync",
                                        title: "Sync changes with peers",
                                        disabled: is_syncing,
                                        onclick: move |_| {
                                            let vm = vault_manager;
                                            let fp = fp.clone();
                                            sync_step.set(Some(SyncStep::Checking));
                                            spawn(async move {
                                                let Some(vm) = vm.read().as_ref().cloned() else {
                                                    sync_step.set(Some(SyncStep::Failed("not ready".into())));
                                                    return;
                                                };
                                                let rid = match realm_id {
                                                    Some(rid) => rid,
                                                    None => {
                                                        match vm.realms().await.first() {
                                                            Some(r) => *r.id().as_bytes(),
                                                            None => {
                                                                sync_step.set(Some(SyncStep::Failed("no realm".into())));
                                                                return;
                                                            }
                                                        }
                                                    }
                                                };
                                                // Light the aurora on the owning column for the
                                                // duration of the sync.
                                                state.write().syncing_realm = Some(rid);
                                                let (tx, mut rx) = tokio::sync::mpsc::channel::<SyncStep>(16);

                                                // Spawn the sync and drain progress in parallel.
                                                let vm_clone = vm.clone();
                                                let intent = format!("sync {fp}");
                                                let sync_handle = tokio::spawn(async move {
                                                    vm_clone.full_sync(&rid, intent, tx).await
                                                });

                                                // Drain progress updates into the signal.
                                                while let Some(step) = rx.recv().await {
                                                    sync_step.set(Some(step));
                                                }

                                                // Capture final result.
                                                match sync_handle.await {
                                                    Ok(Ok(_)) => {} // Done step already sent
                                                    Ok(Err(e)) => sync_step.set(Some(SyncStep::Failed(e))),
                                                    Err(e) => sync_step.set(Some(SyncStep::Failed(format!("{e}")))),
                                                }
                                                // Clear the aurora.
                                                if state.read().syncing_realm == Some(rid) {
                                                    state.write().syncing_realm = None;
                                                }
                                            });
                                        },
                                        if is_syncing { "Syncing..." } else { "Sync" }
                                    }
                                    // Status pill
                                    if let Some(ref step) = *sync_step.read() {
                                        span {
                                            class: "sync-step-pill sync-step-{step_class(step)}",
                                            span { class: "sync-step-label", "{step_text(step)}" }
                                            if let Some(detail) = step_detail(step) {
                                                span { class: "sync-step-detail", " {detail}" }
                                            }
                                        }
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
                    InlineMarkdownEditor { full_path: full_path.clone(), on_content: None }
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
        let realm_id = state.read().modal_file.as_ref().and_then(|m| m.realm_id);
        state.write().modal_file = Some(crate::state::ModalFile {
            realm_id,
            file_path: new_name.clone(),
        });
        state.write().selection.selected_file = Some(new_name);
    }
}
