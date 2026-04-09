//! Main vault view — 4-column realm layout with file modal and contact management.
//!
//! Uses `notify` filesystem watcher for instant private vault file detection.
//! Polls contacts realm every 2 seconds for peer updates.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AppState, ModalFile, PeerDisplayInfo, PEER_COLORS};
use crate::vault_bridge::scan_vault;

/// Rescan the private vault and update state only if files changed.
fn rescan_private(state: &mut Signal<AppState>, vault_path: &std::path::Path) {
    let files = scan_vault(vault_path);

    let current_names: Vec<String> = state.read().private_files.iter().map(|f| f.name.clone()).collect();
    let new_names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    let list_changed = current_names.iter().map(|s| s.as_str()).collect::<Vec<_>>() != new_names;

    if list_changed {
        state.write().private_files = files;
    }
}

/// Navigate up/down in the current column's file list.
fn navigate_file(mut state: Signal<AppState>, direction: i32) {
    let col = state.read().selection.focused_column;
    let current = state.read().selection.selected_file.clone();

    // Get file list for current column
    let files: Vec<String> = if col == 0 {
        state.read().private_files.iter().map(|f| f.path.clone()).collect()
    } else {
        // For realm columns, get files from expanded realms
        Vec::new() // TODO: implement for realm columns
    };

    if files.is_empty() { return; }

    let current_idx = current.as_ref()
        .and_then(|c| files.iter().position(|f| f == c))
        .unwrap_or(0);

    let new_idx = if direction > 0 {
        (current_idx + 1).min(files.len() - 1)
    } else {
        current_idx.saturating_sub(1)
    };

    state.write().selection.selected_file = Some(files[new_idx].clone());
    if col == 0 {
        state.write().selection.selected_realm = None;
    }
}

/// Main vault view: peer bar, info bar, 4-column realm layout, file modal, contact invite overlay.
#[component]
pub fn HomeVault(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    let mut peers = use_signal(Vec::<PeerDisplayInfo>::new);
    let mut contact_invite_open = use_signal(|| false);
    let mut create_group_open = use_signal(|| false);
    let mut create_public_open = use_signal(|| false);

    // Initial scan + filesystem watcher for private vault
    use_effect(move || {
        let vault_path = state.read().vault_path.clone();

        rescan_private(&mut state, &vault_path);

        let watch_path = vault_path.clone();
        spawn(async move {
            use notify::{Watcher, RecursiveMode, Config};

            let changed = Arc::new(AtomicBool::new(false));
            let changed_writer = changed.clone();

            let _watcher = notify::RecommendedWatcher::new(
                move |_res: Result<notify::Event, notify::Error>| {
                    changed_writer.store(true, Ordering::Relaxed);
                },
                Config::default(),
            ).ok().and_then(|mut w| {
                w.watch(&watch_path, RecursiveMode::NonRecursive).ok()?;
                tracing::info!("Watching vault: {}", watch_path.display());
                Some(w)
            });

            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if changed.swap(false, Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    changed.store(false, Ordering::Relaxed);
                    rescan_private(&mut state, &vault_path);
                }
            }
        });
    });

    // Contact polling loop — refreshes peers every 2 seconds.
    // Adapted from indras-gift-cycle/src/app.rs:420-520.
    use_effect(move || {
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                let Some(net) = network.read().clone() else {
                    continue;
                };

                // Read contacts realm and build PeerDisplayInfo vec
                if let Some(contacts_realm) = net.contacts_realm().await {
                    if let Ok(doc) = contacts_realm.contacts().await {
                        let _ = doc.refresh().await;
                        let contacts_data = doc.read().await;
                        let current_count = peers.read().len();

                        if contacts_data.contacts.len() != current_count {
                            let peer_infos: Vec<PeerDisplayInfo> = contacts_data
                                .contacts
                                .iter()
                                .enumerate()
                                .map(|(i, (mid, entry))| {
                                    let name = entry.display_name.clone().unwrap_or_else(|| {
                                        mid.iter().take(4).map(|b| format!("{b:02x}")).collect()
                                    });
                                    let letter = name.chars().next().unwrap_or('?').to_string();
                                    let color_class = PEER_COLORS[i % PEER_COLORS.len()].to_string();
                                    PeerDisplayInfo {
                                        name,
                                        letter,
                                        color_class,
                                        online: true,
                                        member_id: *mid,
                                    }
                                })
                                .collect();
                            peers.set(peer_infos);
                        }
                    }
                }

                // Fallback: scan DM realms for peers not yet in contacts
                let known_peers: std::collections::HashSet<_> = peers
                    .read()
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                for rid in net.conversation_realms() {
                    if let Some(peer_mid) = net.dm_peer_for_realm(&rid) {
                        let name: String = peer_mid
                            .iter()
                            .take(4)
                            .map(|b| format!("{b:02x}"))
                            .collect();
                        if !known_peers.contains(&name)
                            && let Some(contacts_realm) = net.contacts_realm().await
                            && !contacts_realm.is_contact(&peer_mid).await
                        {
                            let _ = contacts_realm.add_contact(peer_mid).await;
                        }
                    }
                }

                // Proactive reconnect to ensure mutual discovery
                for peer_info in peers.read().iter() {
                    let _ = net.connect(peer_info.member_id).await;
                }
            }
        });
    });

    // Derive player info from network for PeerBar and ContactInviteOverlay
    let net_ref = network.read().clone();
    let player_name = {
        let state_name = state.read().display_name.clone();
        if state_name.is_empty() {
            net_ref
                .as_ref()
                .and_then(|n| n.display_name())
                .unwrap_or_default()
        } else {
            state_name
        }
    };
    let member_id = net_ref.as_ref().map(|n| n.id()).unwrap_or([0u8; 32]);

    // Sync overlay triggers from state (for column header "+" buttons)
    if state.read().show_contact_invite {
        contact_invite_open.set(true);
        state.write().show_contact_invite = false;
    }
    if state.read().show_create_group {
        create_group_open.set(true);
        state.write().show_create_group = false;
    }
    if state.read().show_create_public {
        create_public_open.set(true);
        state.write().show_create_public = false;
    }

    rsx! {
        div {
            class: "home-vault",
            tabindex: "0",
            onkeydown: move |e: KeyboardEvent| {
                let key = e.key();
                // Pre-read state to avoid borrow conflicts
                let sel_file = state.read().selection.selected_file.clone();
                let sel_realm = state.read().selection.selected_realm;
                let is_private = sel_realm.is_none();
                let vault_path = state.read().vault_path.clone();
                let col = state.read().selection.focused_column;

                match key {
                    // Spacebar = Quick Look (open modal for selected file)
                    Key::Character(ref c) if c == " " => {
                        e.prevent_default();
                        if let Some(ref file) = sel_file {
                            state.write().modal_file = Some(ModalFile {
                                realm_id: sel_realm,
                                file_path: file.clone(),
                            });
                        }
                    }
                    // F2 = Rename
                    Key::F2 => {
                        if let Some(ref file) = sel_file {
                            state.write().renaming_file = Some(file.clone());
                        }
                    }
                    // Backspace with Meta = Delete
                    Key::Backspace if e.modifiers().meta() => {
                        if is_private {
                            if let Some(ref file) = sel_file {
                                let _ = std::fs::remove_file(vault_path.join(file));
                                state.write().selection.selected_file = None;
                            }
                        }
                    }
                    // Arrow keys for navigation
                    Key::ArrowUp => {
                        e.prevent_default();
                        navigate_file(state, -1);
                    }
                    Key::ArrowDown => {
                        e.prevent_default();
                        navigate_file(state, 1);
                    }
                    Key::ArrowLeft => {
                        e.prevent_default();
                        if col > 0 {
                            state.write().selection.focused_column = col - 1;
                        }
                    }
                    Key::ArrowRight => {
                        e.prevent_default();
                        if col < 3 {
                            state.write().selection.focused_column = col + 1;
                        }
                    }
                    _ => {}
                }
            },
            super::vault_columns::VaultColumns { state }
            super::status_bar::StatusBar { state }
            super::file_modal::FileModal { state }
            super::context_menu::ContextMenu { state }
            // Contact invite overlay
            super::contact_invite::ContactInviteOverlay {
                network: network,
                player_name: player_name.clone(),
                member_id: member_id,
                is_open: contact_invite_open,
            }
            // Create group overlay
            super::create_realm::CreateRealmOverlay {
                network: network,
                kind: super::create_realm::CreateRealmKind::Group,
                peers: peers.read().clone(),
                is_open: create_group_open,
            }
            // Create public vault overlay
            super::create_realm::CreateRealmOverlay {
                network: network,
                kind: super::create_realm::CreateRealmKind::Public,
                peers: Vec::new(),
                is_open: create_public_open,
            }
        }
    }
}
