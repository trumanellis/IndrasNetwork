//! Private column — the user's personal vault files.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::state::{agent_class_for, AppState, ContextMenu, DragPayload, ModalFile, RealmId};
use crate::team::WorkspaceHandle;
use crate::vault_manager::VaultManager;
use super::agent_lane::AgentRoster;
use super::file_item::FileItem;

/// Column 1: private vault files with "+ New" button.
#[component]
pub fn PrivateColumn(
    mut state: Signal<AppState>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
) -> Element {
    let files = state.read().private_files.clone();
    let selected = state.read().selection.selected_file.clone();
    let is_private_selected = state.read().selection.selected_realm.is_none();
    let vault_path = state.read().vault_path.clone();
    let display_name = state.read().display_name.clone();
    let selected_project = state.read().selection.selected_project;
    let header_label = if display_name.trim().is_empty() {
        "PRIVATE".to_string()
    } else {
        display_name.clone()
    };

    // Private vault sentinel — agents and projects live under [0u8; 32].
    const PRIVATE_REALM: RealmId = [0u8; 32];

    let private_projects: Vec<(RealmId, String)> = vault_manager
        .read()
        .as_ref()
        .map(|vm| {
            vm.projects_of(&PRIVATE_REALM)
                .into_iter()
                .map(|pid| {
                    let name = vm.project_name(&pid).unwrap_or_else(|| {
                        pid.iter().take(3).map(|b| format!("{b:02x}")).collect()
                    });
                    (pid, name)
                })
                .collect()
        })
        .unwrap_or_default();
    let private_projects_empty = private_projects.is_empty();

    // Bootstrap: ensure the private vault always has a default Home project on
    // first render, and auto-select it so the accordion opens to reveal the
    // (now-migrated) realm-root files. The use_resource captures `state` and
    // `vault_manager` as signals so it re-fires when either becomes available.
    let _ensure_home = use_resource(move || {
        let vm_opt = vault_manager.read().clone();
        let already_selected = state.read().selection.selected_project.is_some();
        async move {
            if already_selected { return None; }
            let vm = vm_opt?;
            let home_id = vm.default_project(&PRIVATE_REALM).await.ok()?;
            // Only set selection if nothing else has claimed it in the
            // meantime — second guard avoids stomping on a fast user click.
            if state.read().selection.selected_project.is_none() {
                state.write().selection.selected_project = Some(home_id);
            }
            Some(home_id)
        }
    });

    rsx! {
        div { class: "vault-column",
            div { class: "column-header",
                span {
                    class: "column-header-label glow-private",
                    title: "Edit profile",
                    onclick: move |_| {
                        state.write().show_profile = true;
                    },
                    "{header_label}"
                }
                button {
                    class: "column-header-folder glow-private",
                    title: "Open vault folder",
                    onclick: move |_| {
                        let vault = state.read().vault_path.clone();
                        let _ = open::that(vault.parent().unwrap_or(&vault));
                    },
                    "\u{1F4C1}"
                }
                button {
                    class: "column-header-sync glow-private",
                    title: "Open sync panel",
                    onclick: move |_| {
                        state.write().show_sync = true;
                    },
                    "\u{1F504}"
                }
            }
            // Projects section — each project is an accordion row.
            // Selecting a project expands it to show Agents + Files inline.
            div { class: "projects-section",
                div { class: "projects-section-header",
                    span { class: "projects-section-label", "PROJECTS" }
                    button {
                        class: "projects-section-add-btn",
                        title: "New Project",
                        onclick: move |_| {
                            state.write().show_create_project_for = Some(PRIVATE_REALM);
                        },
                        "+ PROJECT"
                    }
                }
                for (pid, pname) in private_projects {
                    {
                        let is_project_selected = selected_project == Some(pid);
                        let row_class = if is_project_selected {
                            "project-row selected"
                        } else {
                            "project-row"
                        };
                        // Mini agent-pip strip — letters of agents living under
                        // this project's folder. Always visible so users can see
                        // who's in each project at a glance, like the peer bar.
                        let agent_letters: Vec<(String, String, &'static str)> = vault_manager
                            .read()
                            .as_ref()
                            .and_then(|vm| vm.project_path(&PRIVATE_REALM, &pid))
                            .map(|root| {
                                workspace_handles
                                    .read()
                                    .iter()
                                    .filter(|h| h.index.root().starts_with(&root))
                                    .map(|h| {
                                        let name = h.agent.as_str().to_string();
                                        let letter = strip_agent_prefix(&name)
                                            .chars()
                                            .next()
                                            .unwrap_or('?')
                                            .to_uppercase()
                                            .to_string();
                                        let color = agent_class_for(&name);
                                        (name, letter, color)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        rsx! {
                            div {
                                class: "{row_class}",
                                onclick: move |_| {
                                    state.write().selection.selected_project = Some(pid);
                                },
                                span { class: "project-row-bullet", "\u{2937}" }
                                span { class: "project-row-name", "{pname}" }
                                for (i, (name, letter, color)) in agent_letters.iter().enumerate() {
                                    {
                                        let key = format!("{}-{}", name, i);
                                        let cls = format!("agent-pip {}", color);
                                        rsx! {
                                            span {
                                                class: "{cls}",
                                                key: "{key}",
                                                title: "{name}",
                                                "{letter}"
                                            }
                                        }
                                    }
                                }
                            }
                            // Accordion children — only visible for the selected project.
                            if is_project_selected {
                                div { class: "project-children",
                                    // AGENTS sub-section
                                    AgentRoster {
                                        state,
                                        workspace_handles,
                                        vault_manager,
                                        project_id: pid,
                                        parent_realm: PRIVATE_REALM,
                                    }
                                    // FILES sub-section
                                    div { class: "project-children-section",
                                        div { class: "project-children-section-header",
                                            span { class: "project-children-section-label", "FILES" }
                                            button {
                                                class: "project-children-add-btn",
                                                title: "New File",
                                                onclick: move |_| {
                                                    let vault_path = state.read().vault_path.clone();
                                                    let existing: Vec<String> = state.read().private_files.iter().map(|f| f.name.clone()).collect();
                                                    let name = unique_untitled_name(&existing);
                                                    let full_path = vault_manager
                                                        .read()
                                                        .as_ref()
                                                        .and_then(|vm| vm.project_path(&PRIVATE_REALM, &pid))
                                                        .map(|p| p.join(&name))
                                                        .unwrap_or_else(|| vault_path.join(&name));
                                                    if std::fs::write(&full_path, "").is_ok() {
                                                        state.write().selection.selected_realm = None;
                                                        state.write().selection.selected_file = Some(name.clone());
                                                        state.write().modal_file = Some(ModalFile {
                                                            realm_id: None,
                                                            file_path: name,
                                                        });
                                                    }
                                                },
                                                "+ FILE"
                                            }
                                        }
                                        for file in files.clone() {
                                            {
                                                let path = file.path.clone();
                                                let is_sel = is_private_selected && selected.as_deref() == Some(path.as_str());
                                                let disk_path = vault_path.join(&file.path);
                                                rsx! {
                                                    FileItem {
                                                        file: file,
                                                        is_selected: is_sel,
                                                        file_disk_path: Some(disk_path),
                                                        source_realm: None::<[u8; 32]>,
                                                        on_drag_start: move |payload: DragPayload| {
                                                            state.write().drag_payload = Some(payload);
                                                        },
                                                        on_drag_end: move |_| {
                                                            state.write().drag_payload = None;
                                                            state.write().drop_target_realm = None;
                                                        },
                                                        on_click: move |p: String| {
                                                            state.write().selection.selected_realm = None;
                                                            state.write().selection.selected_file = Some(p.clone());
                                                            state.write().modal_file = Some(ModalFile {
                                                                realm_id: None,
                                                                file_path: p,
                                                            });
                                                        },
                                                        on_context_menu: move |(p, x, y): (String, f64, f64)| {
                                                            state.write().context_menu = Some(ContextMenu {
                                                                realm_id: None,
                                                                file_path: p,
                                                                x,
                                                                y,
                                                            });
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                        if files.is_empty() {
                                            div { class: "project-children-empty", "No files" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Empty state — only shown when there are no projects at all.
            if private_projects_empty {
                div { class: "vault-column-body",
                    div { class: "column-empty",
                        div { class: "column-empty-icon", "🏠" }
                        div { class: "column-empty-text", "Your private vault is empty" }
                    }
                }
            }
        }
    }
}

/// Strip the conventional `agent-` prefix from a logical agent id so the
/// project-row pip shows the meaningful first letter.
fn strip_agent_prefix(name: &str) -> &str {
    name.strip_prefix("agent-").unwrap_or(name)
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
