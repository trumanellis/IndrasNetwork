//! Realm column — accordion list of realms for a given category.
//!
//! Each realm entry is a drop target for drag-to-share: dropping a file
//! on a realm uploads the artifact and grants access to the realm's peer.
//!
//! For DM rows, each entry also renders an avatar that opens a peer profile
//! popup, while clicking the name expands the shared file list.
//!
//! Expanded realm rows additionally surface their **nested Projects** as
//! indented child rows plus a `+ New Project` affordance. Clicking a Project
//! row selects it and scopes the [`AgentRoster`] to that Project's folder.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AppState, BraidFocus, ContextMenu, DragPayload, ModalFile, PeerDisplayInfo, RealmCategory, RealmId};
use crate::team::WorkspaceHandle;
use crate::vault_manager::VaultManager;
use super::agent_lane::AgentRoster;
use super::braid_sparkline::BraidSparkline;
use super::file_item::FileItem;

/// Scope the Braid Drawer to a realm and kick off an async load of its
/// `BraidView` from the live `BraidDag`. The drawer opens immediately so
/// the slide-in animation isn't blocked by I/O; the graph fills in a
/// moment later when the load completes.
fn focus_drawer_on(
    mut state: Signal<AppState>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    peers: Signal<Vec<PeerDisplayInfo>>,
    id: RealmId,
) {
    {
        let mut w = state.write();
        w.braid_drawer_open = true;
        w.braid_drawer_focus = Some(BraidFocus::Realm(id));
    }
    let vm_opt = vault_manager.read().clone();
    let peers_snap = peers.read().clone();
    let self_name = state.read().display_name.clone();
    spawn(async move {
        let Some(vm) = vm_opt else { return };
        if let Some(view) = vm.load_braid_view(&id, &peers_snap, &self_name).await {
            state.write().braid_view = Some(view);
        }
    });
}

/// A column showing realms of a specific category with accordion file lists.
///
/// Accepts a `network` signal for executing drag-to-share grants on drop.
/// `peers` carries display info for connected peers so DM rows can render
/// avatars and resolve peer ids on click.
///
/// `RealmCategory::Project { .. }` values are filtered out of the top-level
/// list — Projects surface only as nested child rows under their parent Realm,
/// never as top-level realm entries in any column.
#[component]
pub fn RealmColumn(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    peers: Signal<Vec<PeerDisplayInfo>>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
    category: RealmCategory,
    label: &'static str,
) -> Element {
    // Top-level realm list excludes Projects — they live as nested child rows
    // under their parent realm only, never as a column entry.
    let realms: Vec<_> = state.read().realms.iter()
        .filter(|r| r.category == category)
        .filter(|r| !matches!(r.category, RealmCategory::Project { .. }))
        .cloned()
        .collect();
    let expanded = state.read().selection.expanded_realms.clone();
    let selected_realm = state.read().selection.selected_realm;
    let selected_project = state.read().selection.selected_project;
    let selected_file = state.read().selection.selected_file.clone();
    let drop_target = state.read().drop_target_realm;
    let syncing_realm = state.read().syncing_realm;

    let add_title = match category {
        RealmCategory::Dm => "Add Contact",
        RealmCategory::Group => "New Group",
        RealmCategory::Project { .. } => "New Group",
        RealmCategory::World => "New World Vault",
        RealmCategory::Private => "New File",
    };

    let glow_class = match category {
        RealmCategory::Dm => "glow-connections",
        RealmCategory::Group => "glow-groups",
        RealmCategory::Project { .. } => "glow-groups",
        RealmCategory::World => "glow-world",
        RealmCategory::Private => "glow-private",
    };

    // For DM rows, resolve peer info per realm so we can render avatars.
    let net_snap = network.read().clone();
    let peers_snap = peers.read().clone();

    rsx! {
        div { class: "vault-column",
            div { class: "column-header",
                span { class: "{glow_class}", "{label}" }
                button {
                    class: "column-header-add {glow_class}",
                    title: "{add_title}",
                    onclick: move |_| {
                        match category {
                            RealmCategory::Dm => state.write().show_contact_invite = true,
                            RealmCategory::Group => state.write().show_create_group = true,
                            RealmCategory::Project { .. } => state.write().show_create_group = true,
                            RealmCategory::World => state.write().show_create_public = true,
                            RealmCategory::Private => {}
                        }
                    },
                    "+"
                }
            }
            div { class: "vault-column-body",
                if realms.is_empty() {
                    {
                        let (empty_icon, empty_text) = match category {
                            RealmCategory::Dm => ("💬", "Connect with someone to start a conversation"),
                            RealmCategory::Group => ("👥", "Join or create a group to collaborate"),
                            RealmCategory::Project { .. } => ("👥", "Join or create a group to collaborate"),
                            RealmCategory::World => ("🌍", "World realms will appear here"),
                            RealmCategory::Private => ("🏠", "Your private vault is empty"),
                        };
                        rsx! {
                            div { class: "column-empty",
                                div { class: "column-empty-icon", "{empty_icon}" }
                                div { class: "column-empty-text", "{empty_text}" }
                            }
                        }
                    }
                } else {
                    for realm in realms {
                        {
                            let id = realm.id;
                            let is_expanded = expanded.contains(&id);
                            let is_selected = selected_realm == Some(id);
                            let is_drop_target = drop_target == Some(id);
                            let is_aurora = syncing_realm == Some(id);
                            let chevron_class = if is_expanded { "realm-chevron expanded" } else { "realm-chevron" };
                            let entry_class = match (is_selected, is_drop_target, is_aurora) {
                                (_, true, _) => "realm-entry drop-target".to_string(),
                                (true, false, true) => "realm-entry selected aurora-active".to_string(),
                                (true, false, false) => "realm-entry selected".to_string(),
                                (false, false, true) => "realm-entry aurora-active".to_string(),
                                (false, false, false) => "realm-entry".to_string(),
                            };

                            // Resolve DM peer info for avatar rendering.
                            // state's RealmId is `[u8; 32]`; the network's is `InterfaceId`,
                            // so wrap before querying.
                            let peer_for_row = if matches!(category, RealmCategory::Dm) {
                                net_snap.as_ref().and_then(|net| {
                                    let realm_id = indras_network::RealmId::new(id);
                                    net.dm_peer_for_realm(&realm_id)
                                })
                            } else {
                                None
                            };
                            let peer_display = peer_for_row.and_then(|pid| {
                                peers_snap.iter().find(|p| p.member_id == pid).cloned()
                            });


                            rsx! {
                                // Realm row — drop target for drag-to-share.
                                // Click handlers split: avatar opens peer popup, name+chevron toggles expand.
                                div {
                                    class: "{entry_class}",
                                    // Drop target events for drag-to-share
                                    ondragover: move |evt: DragEvent| {
                                        evt.prevent_default();
                                        if state.read().drag_payload.is_some() {
                                            state.write().drop_target_realm = Some(id);
                                        }
                                    },
                                    ondragenter: move |evt: DragEvent| {
                                        evt.prevent_default();
                                        if state.read().drag_payload.is_some() {
                                            state.write().drop_target_realm = Some(id);
                                        }
                                    },
                                    ondragleave: move |_evt: DragEvent| {
                                        if state.read().drop_target_realm == Some(id) {
                                            state.write().drop_target_realm = None;
                                        }
                                    },
                                    ondrop: move |evt: DragEvent| {
                                        evt.prevent_default();
                                        let payload = state.read().drag_payload.clone();
                                        state.write().drag_payload = None;
                                        state.write().drop_target_realm = None;
                                        // Auto-expand the realm accordion so the file appears
                                        state.write().selection.expanded_realms.insert(id);

                                        let Some(payload) = payload else { return; };
                                        // Prevent same-realm drop (no-op)
                                        if payload.source_realm == Some(id) { return; }

                                        let vm = vault_manager.read().clone();

                                        spawn(async move {
                                            let Some(vm) = vm else { return; };
                                            if let Some(vault_dir) = vm.vault_path(&id) {
                                                let dest = vault_dir.join(&payload.file_name);
                                                if let Some(parent) = dest.parent() {
                                                    let _ = tokio::fs::create_dir_all(parent).await;
                                                }
                                                match tokio::fs::copy(&payload.file_disk_path, &dest).await {
                                                    Ok(_) => tracing::info!("Copied '{}' to vault", payload.file_name),
                                                    Err(e) => tracing::error!("Failed to copy file to vault: {e}"),
                                                }
                                            }
                                        });
                                    },
                                    {
                                        let toggle_expand = move |_| {
                                            let mut sel = state.read().selection.clone();
                                            if sel.expanded_realms.contains(&id) {
                                                sel.expanded_realms.remove(&id);
                                            } else {
                                                sel.expanded_realms.insert(id);
                                            }
                                            // Select the realm too so the roster picks the
                                            // right project when the realm expands.
                                            sel.selected_realm = Some(id);
                                            sel.selected_project = None;
                                            state.write().selection = sel;
                                            focus_drawer_on(state, vault_manager, peers, id);
                                        };
                                        rsx! {
                                            span {
                                                class: "{chevron_class}",
                                                onclick: toggle_expand,
                                                "\u{25B8}"
                                            }
                                        }
                                    }
                                    if let Some(peer) = peer_display.clone() {
                                        {
                                            let peer_id = peer.member_id;
                                            let avatar_color = peer.color_class.clone();
                                            let avatar_letter = peer.letter.clone();
                                            rsx! {
                                                button {
                                                    class: "realm-entry-avatar profile-avatar {avatar_color}",
                                                    title: "View profile",
                                                    "aria-label": "View profile for {peer.name}",
                                                    onclick: move |evt: MouseEvent| {
                                                        evt.stop_propagation();
                                                        state.write().profile_popup_target = Some((peer_id, id));
                                                    },
                                                    "{avatar_letter}"
                                                }
                                            }
                                        }
                                    }
                                    {
                                        let toggle_name = move |_| {
                                            let mut sel = state.read().selection.clone();
                                            if sel.expanded_realms.contains(&id) {
                                                sel.expanded_realms.remove(&id);
                                            } else {
                                                sel.expanded_realms.insert(id);
                                            }
                                            sel.selected_realm = Some(id);
                                            sel.selected_project = None;
                                            state.write().selection = sel;
                                            focus_drawer_on(state, vault_manager, peers, id);
                                        };
                                        rsx! {
                                            span {
                                                class: "realm-entry-name",
                                                onclick: toggle_name,
                                                "{realm.display_name}"
                                            }
                                        }
                                    }
                                    {
                                        // Only render the sparkline if the currently-cached
                                        // braid view belongs to this realm (otherwise we'd
                                        // render other realms' history by mistake).
                                        let sparkline_view = state
                                            .read()
                                            .braid_view
                                            .as_ref()
                                            .filter(|v| v.realm_id == id)
                                            .cloned();
                                        rsx! {
                                            BraidSparkline { view: sparkline_view, width: 64.0 }
                                        }
                                    }
                                    span { class: "realm-entry-meta", "{realm.member_count}" }
                                }

                                // Projects accordion — expanded when this realm is selected.
                                if is_expanded {
                                    {
                                        let projects_for_realm: Vec<(RealmId, String)> = vault_manager
                                            .read()
                                            .as_ref()
                                            .map(|vm| {
                                                vm.projects_of(&id)
                                                    .into_iter()
                                                    .map(|pid| {
                                                        let name = vm
                                                            .project_name(&pid)
                                                            .unwrap_or_else(|| short_project_label(&pid));
                                                        (pid, name)
                                                    })
                                                    .collect()
                                            })
                                            .unwrap_or_default();
                                        let realm_files = realm.files.clone();
                                        rsx! {
                                            div { class: "projects-section",
                                                div { class: "projects-section-header",
                                                    span { class: "projects-section-label", "PROJECTS" }
                                                    button {
                                                        class: "projects-section-add-btn",
                                                        title: "New Project",
                                                        onclick: move |_| {
                                                            state.write().show_create_project_for = Some(id);
                                                        },
                                                        "+ PROJECT"
                                                    }
                                                }
                                                for (pid, pname) in projects_for_realm {
                                                    {
                                                        let is_project_selected = selected_project == Some(pid);
                                                        let row_class = if is_project_selected {
                                                            "project-row selected"
                                                        } else {
                                                            "project-row"
                                                        };
                                                        let files_for_project = realm_files.clone();
                                                        // Agent pips for this project (mini peer-bar style).
                                                        let agent_letters: Vec<(String, String)> = vault_manager
                                                            .read()
                                                            .as_ref()
                                                            .and_then(|vm| vm.project_path(&id, &pid))
                                                            .map(|root| {
                                                                workspace_handles
                                                                    .read()
                                                                    .iter()
                                                                    .filter(|h| h.index.root().starts_with(&root))
                                                                    .map(|h| {
                                                                        let name = h.agent.as_str().to_string();
                                                                        let letter = name.strip_prefix("agent-").unwrap_or(&name)
                                                                            .chars()
                                                                            .next()
                                                                            .unwrap_or('?')
                                                                            .to_uppercase()
                                                                            .to_string();
                                                                        (name, letter)
                                                                    })
                                                                    .collect()
                                                            })
                                                            .unwrap_or_default();
                                                        rsx! {
                                                            div {
                                                                class: "{row_class}",
                                                                onclick: move |_| {
                                                                    let mut sel = state.read().selection.clone();
                                                                    sel.selected_realm = Some(id);
                                                                    sel.selected_project = Some(pid);
                                                                    state.write().selection = sel;
                                                                },
                                                                span { class: "project-row-bullet", "\u{2937}" }
                                                                span { class: "project-row-name", "{pname}" }
                                                                for (i, (aname, letter)) in agent_letters.iter().enumerate() {
                                                                    {
                                                                        let key = format!("{}-{}", aname, i);
                                                                        rsx! {
                                                                            span {
                                                                                class: "agent-pip",
                                                                                key: "{key}",
                                                                                title: "{aname}",
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
                                                                        parent_realm: id,
                                                                    }
                                                                    // FILES sub-section
                                                                    div { class: "project-children-section",
                                                                        div { class: "project-children-section-header",
                                                                            span { class: "project-children-section-label", "FILES" }
                                                                        }
                                                                        for file in files_for_project.clone() {
                                                                            {
                                                                                let path = file.path.clone();
                                                                                let is_sel = is_selected && selected_file.as_deref() == Some(path.as_str());
                                                                                let file = file.clone();
                                                                                rsx! {
                                                                                    FileItem {
                                                                                        file: file,
                                                                                        is_selected: is_sel,
                                                                                        source_realm: Some(id),
                                                                                        on_drag_start: move |payload: DragPayload| {
                                                                                            state.write().drag_payload = Some(payload);
                                                                                        },
                                                                                        on_drag_end: move |_| {
                                                                                            state.write().drag_payload = None;
                                                                                            state.write().drop_target_realm = None;
                                                                                        },
                                                                                        on_click: move |p: String| {
                                                                                            state.write().selection.selected_realm = Some(id);
                                                                                            state.write().selection.selected_file = Some(p.clone());
                                                                                            state.write().modal_file = Some(ModalFile {
                                                                                                realm_id: Some(id),
                                                                                                file_path: p,
                                                                                            });
                                                                                        },
                                                                                        on_context_menu: move |(p, x, y): (String, f64, f64)| {
                                                                                            state.write().context_menu = Some(ContextMenu {
                                                                                                realm_id: Some(id),
                                                                                                file_path: p,
                                                                                                x,
                                                                                                y,
                                                                                            });
                                                                                        },
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                        if files_for_project.is_empty() {
                                                                            div { class: "project-children-empty", "No files" }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Six-char lowercase hex label for a Project id used when no display name has
/// been registered yet (e.g. a project synced in from another peer whose name
/// blob hasn't been mirrored). Matches `vault_manager::short_hex` output
/// intentionally so labels line up with log entries.
fn short_project_label(id: &RealmId) -> String {
    id.iter().take(3).map(|b| format!("{b:02x}")).collect()
}
