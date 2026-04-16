//! Realm column — accordion list of realms for a given category.
//!
//! Each realm entry is a drop target for drag-to-share: dropping a file
//! on a realm uploads the artifact and grants access to the realm's peer.
//!
//! For DM rows, each entry also renders an avatar that opens a peer profile
//! popup, while clicking the name expands the shared file list.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AppState, ContextMenu, DragPayload, ModalFile, PeerDisplayInfo, RealmCategory};
use crate::vault_manager::VaultManager;
use super::file_item::FileItem;

/// A column showing realms of a specific category with accordion file lists.
///
/// Accepts a `network` signal for executing drag-to-share grants on drop.
/// `peers` carries display info for connected peers so DM rows can render
/// avatars and resolve peer ids on click.
#[component]
pub fn RealmColumn(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    peers: Signal<Vec<PeerDisplayInfo>>,
    category: RealmCategory,
    label: &'static str,
) -> Element {
    let realms: Vec<_> = state.read().realms.iter()
        .filter(|r| r.category == category)
        .cloned()
        .collect();
    let expanded = state.read().selection.expanded_realms.clone();
    let selected_realm = state.read().selection.selected_realm;
    let selected_file = state.read().selection.selected_file.clone();
    let drop_target = state.read().drop_target_realm;

    let add_title = match category {
        RealmCategory::Dm => "Add Contact",
        RealmCategory::Group => "New Group",
        RealmCategory::World => "New World Vault",
        RealmCategory::Private => "New File",
    };

    let glow_class = match category {
        RealmCategory::Dm => "glow-connections",
        RealmCategory::Group => "glow-groups",
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
                            let chevron_class = if is_expanded { "realm-chevron expanded" } else { "realm-chevron" };
                            let entry_class = match (is_selected, is_drop_target) {
                                (_, true) => "realm-entry drop-target",
                                (true, false) => "realm-entry selected",
                                (false, false) => "realm-entry",
                            };
                            let files_class = if is_expanded { "realm-files expanded" } else { "realm-files" };

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
                                            if let Some(vault_dir) = vm.vault_path(&id).await {
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
                                            state.write().selection = sel;
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
                                            state.write().selection = sel;
                                        };
                                        rsx! {
                                            span {
                                                class: "realm-entry-name",
                                                onclick: toggle_name,
                                                "{realm.display_name}"
                                            }
                                        }
                                    }
                                    span { class: "realm-entry-meta", "{realm.member_count}" }
                                }

                                // Accordion file list
                                div {
                                    class: "{files_class}",
                                    for file in &realm.files {
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
                                    if realm.files.is_empty() && is_expanded {
                                        div { class: "realm-files-empty", "No files" }
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
