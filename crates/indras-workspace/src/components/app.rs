//! Root application component wired to real vault data.

use dioxus::prelude::*;
use dioxus::prelude::Key;

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::bridge::vault_bridge::{VaultHandle, InMemoryVault};
use crate::bridge::network_bridge::{NetworkHandle, is_first_run, create_identity, load_identity};
use crate::components::topbar::Topbar;
use crate::components::document::DocumentView;
use crate::components::story::{StoryView, StoryMessage, StoryArtifactRef};
use crate::components::quest::{QuestView, QuestKind, ProofEntry, ProofArtifact, AssignedToken, AttentionItem};
use crate::components::settings::SettingsView;
use crate::components::setup::SetupView;
use crate::components::pass_story::PassStoryOverlay;
use crate::components::event_log::EventLogView;
use crate::state::workspace::{EventDirection, log_event};
use crate::components::bottom_nav::{BottomNav, NavTab};
use crate::components::fab::Fab;
use crate::state::workspace::{WorkspaceState, ViewType, AppPhase, PeerDisplayInfo};
use crate::state::navigation::{NavigationState, VaultTreeNode};
use crate::state::editor::{EditorState, DocumentMeta, BlockDocumentSchema};


use indras_ui::{
    IdentityRow, PeerStrip,
    VaultSidebar, TreeNode,
    SlashMenu, SlashAction,
    DetailPanel, PropertyRow, AudienceMember, HeatEntry, TrailEvent, ReferenceItem, SyncEntry,
    MarkdownPreviewOverlay, PreviewFile, PreviewViewMode,
    ContactInviteOverlay,
    ChatPanel,
};
use indras_ui::PeerDisplayInfo as UiPeerDisplayInfo;
use indras_network::{IdentityCode, IndrasNetwork, HomeRealm, Realm, RealmChatDocument, EditableChatMessage};

#[cfg(feature = "lua-scripting")]
use crate::scripting::channels::AppTestChannels;
#[cfg(feature = "lua-scripting")]
use crate::scripting::dispatcher::spawn_dispatcher;
#[cfg(feature = "lua-scripting")]
use crate::scripting::event::AppEvent;

/// Convert a MemberId to a hex string for identity comparison.
fn member_id_hex(id: &[u8; 32]) -> String {
    id.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Convert a CRDT chat message to the UI's StoryMessage format.
fn chat_msg_to_story(
    msg: &EditableChatMessage,
    my_name: &str,
    my_id: Option<&str>,
    chat_doc: &RealmChatDocument,
) -> StoryMessage {
    let is_self = msg.author_id.as_deref()
        .and_then(|aid| my_id.map(|mid| aid == mid))
        .unwrap_or_else(|| msg.author == my_name);
    let letter = msg.author.chars().next().unwrap_or('?').to_string();
    let time = {
        let dt = chrono::DateTime::from_timestamp_millis(msg.created_at as i64)
            .unwrap_or_default();
        dt.format("%H:%M").to_string()
    };
    let reactions: Vec<(String, usize)> = msg.reactions.iter()
        .map(|(emoji, authors)| (emoji.clone(), authors.len()))
        .collect();
    let reply_to_preview = msg.reply_to.as_ref()
        .and_then(|id| chat_doc.reply_preview(id))
        .map(|(author, text)| format!("{}: {}", author, text));

    StoryMessage {
        sender_name: msg.author.clone(),
        sender_letter: letter,
        sender_color_class: String::new(),
        content: msg.current_content.clone(),
        time,
        is_self,
        artifact_ref: None,
        image_ref: if msg.is_image() { Some(true) } else { None },
        branch_label: None,
        day_separator: None,
        message_id: Some(msg.id.clone()),
        reactions,
        reply_to_preview,
    }
}

/// Root application component.
#[component]
pub fn RootApp() -> Element {
    let mut workspace = use_signal(WorkspaceState::new);
    let mut vault_handle = use_signal(|| None::<VaultHandle>);
    let mut story_messages = use_signal(Vec::<StoryMessage>::new);
    let mut quest_data = use_signal(|| None::<QuestViewData>);
    let mut active_tab = use_signal(|| NavTab::Vault);
    let mut token_picker_open = use_signal(|| false);
    let mut preview_open = use_signal(|| false);
    let mut preview_file = use_signal(|| None::<PreviewFile>);
    let mut preview_view_mode = use_signal(|| PreviewViewMode::Rendered);
    let mut network_handle = use_signal(|| None::<NetworkHandle>);
    let mut setup_error = use_signal(|| None::<String>);
    let mut setup_loading = use_signal(|| false);
    let mut pass_story_open = use_signal(|| false);
    let mut contact_invite_open = use_signal(|| false);
    let mut contact_invite_input = use_signal(String::new);
    let mut contact_invite_status = use_signal(|| None::<String>);
    let mut contact_parsed_name = use_signal(|| None::<String>);
    let mut contact_copy_feedback = use_signal(|| false);
    let mut network_for_chat = use_signal(|| None::<Arc<IndrasNetwork>>);
    let mut contact_invite_uri = use_signal(String::new);
    let mut contact_display_name_sig = use_signal(String::new);
    let mut contact_member_id_short_sig = use_signal(String::new);
    let mut home_realm_handle = use_signal(|| None::<HomeRealm>);
    let mut realm_map = use_signal(|| std::collections::HashMap::<String, Realm>::new());

    // --- Lua scripting dispatcher (feature-gated) ---
    #[cfg(feature = "lua-scripting")]
    let mut lua_event_tx = use_signal(|| None::<tokio::sync::broadcast::Sender<AppEvent>>);
    #[cfg(not(feature = "lua-scripting"))]
    let _lua_event_tx = use_signal(|| None::<()>);

    #[cfg(feature = "lua-scripting")]
    {
        let test_channels: Option<Arc<tokio::sync::Mutex<AppTestChannels>>> =
            use_context::<Option<Arc<tokio::sync::Mutex<AppTestChannels>>>>();

        use_effect(move || {
            if let Some(ref channels) = test_channels {
                // Extract event_tx for use in other closures
                if let Ok(guard) = channels.try_lock() {
                    lua_event_tx.set(Some(guard.event_tx.clone()));
                }
                spawn_dispatcher(
                    Arc::clone(channels),
                    workspace,
                    contact_invite_open,
                    contact_invite_input,
                );
            }
        });
    }

    // Save world view and stop network on shutdown
    let network_for_cleanup = network_handle;
    use_drop(move || {
        if let Some(nh) = network_for_cleanup.read().as_ref() {
            let net = nh.network.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = net.save_world_view().await {
                        tracing::error!(error = %e, "Failed to save world view on shutdown");
                    }
                    if let Err(e) = net.stop().await {
                        tracing::error!(error = %e, "Failed to stop network on shutdown");
                    }
                });
            })
            .join()
            .ok();
        }
    });

    // Memo wrappers for ReadSignal props
    let ci_uri = use_memo(move || contact_invite_uri.read().clone());
    let ci_name = use_memo(move || contact_display_name_sig.read().clone());
    let ci_mid = use_memo(move || contact_member_id_short_sig.read().clone());
    let ci_status = use_memo(move || contact_invite_status.read().clone());
    let ci_parsed = use_memo(move || contact_parsed_name.read().clone());
    let ci_copied = use_memo(move || *contact_copy_feedback.read());
    let attention_items: Vec<AttentionItem> = vec![
        AttentionItem { target: "Architecture Notes".into(), when: "Today 10:14 AM".into(), duration: "6m 33s".into() },
        AttentionItem { target: "Team Discussion".into(), when: "Today 9:41 AM".into(), duration: "12m 08s".into() },
        AttentionItem { target: "Design Assets".into(), when: "Today 9:22 AM".into(), duration: "4m 51s".into() },
        AttentionItem { target: "Need: Logo Design".into(), when: "Yesterday 4:38 PM".into(), duration: "2m 14s".into() },
        AttentionItem { target: "DM with Sage".into(), when: "Yesterday 4:02 PM".into(), duration: "9m 37s".into() },
        AttentionItem { target: "Personal Journal".into(), when: "Yesterday 3:18 PM".into(), duration: "18m 02s".into() },
        AttentionItem { target: "Project Alpha".into(), when: "Yesterday 2:55 PM".into(), duration: "7m 45s".into() },
    ];

    // Phase-based boot: check first-run on mount
    use_effect(move || {
        spawn(async move {
            if is_first_run() {
                workspace.write().phase = AppPhase::Setup;
            } else {
                // Returning user — load existing identity
                match load_identity().await {
                    Ok(nh) => {
                        let player_name = nh.network.display_name()
                            .unwrap_or("Unknown").to_string();
                        let player_id = nh.network.id();

                        let now = chrono::Utc::now().timestamp_millis();
                        match InMemoryVault::in_memory(player_id, now) {
                            Ok(vault) => {
                                vault_handle.set(Some(VaultHandle {
                                    vault: Arc::new(Mutex::new(vault)),
                                    player_id,
                                    player_name: player_name.clone(),
                                }));
                                let net = Arc::clone(&nh.network);
                                network_for_chat.set(Some(Arc::clone(&net)));
                                network_handle.set(Some(nh));
                                {
                                    let mut ws = workspace.write();
                                    ws.phase = AppPhase::Workspace;
                                    log_event(&mut ws, EventDirection::System, format!("Identity loaded: {}", player_name));
                                }

                                // Emit AppReady event for Lua scripting
                                #[cfg(feature = "lua-scripting")]
                                if let Some(ref tx) = *lua_event_tx.read() {
                                    let _ = tx.send(AppEvent::AppReady);
                                }

                                // Start the network (enables inbox listener for incoming connections)
                                log_event(&mut workspace.write(), EventDirection::System, "Starting network...".to_string());
                                if let Err(e) = net.start().await {
                                    tracing::warn!(error = %e, "Failed to start network (non-fatal)");
                                    log_event(&mut workspace.write(), EventDirection::System, format!("Network start warning: {}", e));
                                } else {
                                    log_event(&mut workspace.write(), EventDirection::System, "Network started \u{2014} listening for connections".to_string());
                                }

                                // Join contacts realm so inbox listener can store contacts
                                if let Err(e) = net.join_contacts_realm().await {
                                    tracing::warn!(error = %e, "Failed to join contacts realm (non-fatal)");
                                }

                                // Initialize home realm for persistent artifact storage
                                match net.home_realm().await {
                                    Ok(hr) => {
                                        let hr_clone = hr.clone();
                                        home_realm_handle.set(Some(hr));
                                        log_event(&mut workspace.write(), EventDirection::System, "Home realm initialized".to_string());

                                        // Restore sidebar from HomeRealm artifact index
                                        match hr_clone.artifact_index().await {
                                            Ok(doc) => {
                                                let data = doc.read().await;
                                                let entries: Vec<_> = data.active_artifacts().collect();
                                                if !entries.is_empty() {
                                                    // Build realm lookup: artifact_id -> Realm
                                                    let mut art_to_realm = std::collections::HashMap::new();
                                                    for rid in net.realms() {
                                                        if let Some(realm) = net.get_realm_by_id(&rid) {
                                                            if let Some(art_id) = realm.artifact_id() {
                                                                art_to_realm.insert(*art_id, realm);
                                                            }
                                                        }
                                                    }

                                                    let mut nodes = Vec::new();
                                                    let mut contacts_section_added = false;

                                                    // Get vault for artifact type lookups
                                                    let vault_for_restore = vault_handle.read().as_ref()
                                                        .map(|vh| Arc::clone(&vh.vault));

                                                    // All entries are flat (no parent/child nesting)
                                                    for entry in &entries {
                                                        let node_id = format!("{:?}", entry.id);

                                                        // Look up artifact from vault to get type
                                                        let (icon, view_type_str, section) = if let Some(ref vault_arc) = vault_for_restore {
                                                            let v = vault_arc.lock().await;
                                                            if let Ok(Some(artifact)) = v.get_artifact(&entry.id) {
                                                                let ic = NavigationState::icon_for_type(&artifact.artifact_type);
                                                                let vt = NavigationState::view_type_for(&artifact.artifact_type);
                                                                let is_contact = artifact.artifact_type == "contact";
                                                                let sect = if nodes.is_empty() {
                                                                    Some("Vault".to_string())
                                                                } else if is_contact && !contacts_section_added {
                                                                    contacts_section_added = true;
                                                                    Some("Contacts".to_string())
                                                                } else {
                                                                    None
                                                                };
                                                                (ic.to_string(), vt.to_string(), sect)
                                                            } else {
                                                                let sect = if nodes.is_empty() { Some("Vault".to_string()) } else { None };
                                                                ("\u{1F4C4}".to_string(), "document".to_string(), sect)
                                                            }
                                                        } else {
                                                            let sect = if nodes.is_empty() { Some("Vault".to_string()) } else { None };
                                                            ("\u{1F4C4}".to_string(), "document".to_string(), sect)
                                                        };

                                                        // Use realm alias as label if available
                                                        let label = if let Some(realm) = art_to_realm.get(&entry.id) {
                                                            match realm.get_alias().await {
                                                                Ok(Some(alias)) => alias,
                                                                _ => entry.name.clone(),
                                                            }
                                                        } else {
                                                            entry.name.clone()
                                                        };

                                                        nodes.push(VaultTreeNode {
                                                            id: node_id.clone(),
                                                            artifact_id: Some(entry.id),
                                                            label,
                                                            icon,
                                                            heat_level: 0,
                                                            depth: 0,
                                                            has_children: false,
                                                            expanded: false,
                                                            section,
                                                            view_type: view_type_str,
                                                        });

                                                        // Insert realm into realm_map if found
                                                        if let Some(realm) = art_to_realm.remove(&entry.id) {
                                                            realm_map.write().insert(node_id, realm);
                                                        }
                                                    }

                                                    if !nodes.is_empty() {
                                                        workspace.write().nav.vault_tree = nodes;
                                                        log_event(&mut workspace.write(), EventDirection::System, format!("Restored {} artifacts from home realm", entries.len()));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::warn!(error = %e, "Failed to read artifact index (non-fatal)");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "Failed to initialize home realm (non-fatal)");
                                        log_event(&mut workspace.write(), EventDirection::System, format!("Home realm warning: {}", e));
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Vault creation failed: {}", e);
                                {
                                    let mut ws = workspace.write();
                                    ws.phase = AppPhase::Setup;
                                    log_event(&mut ws, EventDirection::System, format!("ERROR: Vault creation failed: {}", e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to load identity: {}", e);
                        {
                            let mut ws = workspace.write();
                            ws.phase = AppPhase::Setup;
                            log_event(&mut ws, EventDirection::System, format!("ERROR: Failed to load identity: {}", e));
                        }
                    }
                }
            }
        });
    });

    // Poll contacts realm every 2 seconds to detect new connections
    let peer_colors = [
        "peer-dot-sage", "peer-dot-zeph", "peer-dot-rose",
        "peer-dot-sage", "peer-dot-zeph", "peer-dot-rose",
    ];
    use_effect(move || {
        spawn(async move {
            let mut tick: u64 = 0;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                // Only poll when in Workspace phase
                if workspace.read().phase != AppPhase::Workspace {
                    continue;
                }

                let net = {
                    let guard = network_handle.read();
                    guard.as_ref().map(|nh| nh.network.clone())
                };
                let Some(net) = net else { continue; };

                if let Some(contacts_realm) = net.contacts_realm().await {
                    if let Ok(doc) = contacts_realm.contacts().await {
                        let data = doc.read().await;
                        let current_count = workspace.read().peers.entries.len();
                        let new_count = data.contacts.len();

                        if new_count != current_count {
                            let entries: Vec<PeerDisplayInfo> = data.contacts.iter().enumerate().map(|(i, (mid, entry))| {
                                let name = entry.display_name.clone().unwrap_or_else(|| {
                                    mid.iter().take(4).map(|b| format!("{:02x}", b)).collect()
                                });
                                let letter = name.chars().next().unwrap_or('?').to_string();
                                let color = peer_colors[i % peer_colors.len()].to_string();
                                PeerDisplayInfo {
                                    name,
                                    letter,
                                    color_class: color,
                                    online: true,
                                    player_id: *mid,
                                }
                            }).collect();

                            // Log new contacts, create Contact trees, and emit events
                            let mut sidebar_needs_rebuild = false;
                            for entry in &entries {
                                let already_known = workspace.read().peers.entries.iter().any(|p| p.player_id == entry.player_id);
                                if !already_known {
                                    log_event(&mut workspace.write(), EventDirection::Received, format!("Contact confirmed: {}", entry.name));

                                    // Create/find Contact tree and add event leaf
                                    let vh = vault_handle.read().clone();
                                    if let Some(vh) = vh {
                                        let mut vault = vh.vault.lock().await;
                                        let now = chrono::Utc::now().timestamp_millis();
                                        let root_id = vault.root.id.clone();

                                        // Check if Contact tree already exists for this peer
                                        let existing_contact = if let Ok(Some(root_art)) = vault.get_artifact(&root_id) {
                                            root_art.references.iter().find_map(|aref| {
                                                if aref.label.as_deref() == Some(&entry.name) {
                                                    if let Ok(Some(t)) = vault.get_artifact(&aref.artifact_id) {
                                                        if t.artifact_type == "contact" {
                                                            return Some((aref.artifact_id.clone(), t.references.len() as u64));
                                                        }
                                                    }
                                                }
                                                None
                                            })
                                        } else {
                                            None
                                        };

                                        let contact_id_and_pos = if let Some(info) = existing_contact {
                                            Some(info)
                                        } else {
                                            // Create Contact tree (receiving side)
                                            let audience = vec![vh.player_id, entry.player_id];
                                            if let Ok(contact_tree) = vault.place_tree("contact", audience, now) {
                                                let ct_id = contact_tree.id.clone();
                                                let position = if let Ok(Some(root_art)) = vault.get_artifact(&root_id) {
                                                    root_art.references.len() as u64
                                                } else {
                                                    0
                                                };
                                                let _ = vault.compose(&root_id, ct_id.clone(), position, Some(entry.name.clone()));
                                                sidebar_needs_rebuild = true;
                                                Some((ct_id, 0))
                                            } else {
                                                None
                                            }
                                        };

                                        // Add "Connection confirmed" event leaf
                                        if let Some((contact_id, pos)) = contact_id_and_pos {
                                            if let Ok(event_leaf) = vault.place_leaf(b"Connection confirmed", String::new(), None, "message", now) {
                                                let _ = vault.compose(
                                                    &contact_id,
                                                    event_leaf.id,
                                                    pos,
                                                    Some("msg:System".to_string()),
                                                );
                                            }
                                        }

                                        // Rebuild sidebar if new Contact tree was created
                                        if sidebar_needs_rebuild {
                                            if let Ok(Some(rebuilt_root)) = vault.get_artifact(&vault.root.id) {
                                                let mut nodes = Vec::new();
                                                let mut quest_section_added = false;
                                                let mut exchange_section_added = false;
                                                let mut tokens_section_added = false;
                                                let mut contacts_section_added = false;

                                                for aref in &rebuilt_root.references {
                                                    if let Ok(Some(artifact)) = vault.get_artifact(&aref.artifact_id) {
                                                        let view_type_str = NavigationState::view_type_for(&artifact.artifact_type);
                                                        let icon = NavigationState::icon_for_type(&artifact.artifact_type);
                                                        let heat_val = vault.heat(&aref.artifact_id, now).unwrap_or(0.0);
                                                        let heat_lvl = indras_ui::heat_level(heat_val);
                                                        let label_str = aref.label.clone().unwrap_or_default();

                                                        let is_quest_type = matches!(
                                                            artifact.artifact_type.as_str(),
                                                            "quest" | "need" | "offering" | "intention"
                                                        );
                                                        let is_contact = artifact.artifact_type == "contact";
                                                        let section = if nodes.is_empty() {
                                                            Some("Vault".to_string())
                                                        } else if is_contact && !contacts_section_added {
                                                            contacts_section_added = true;
                                                            Some("Contacts".to_string())
                                                        } else if is_quest_type && !quest_section_added {
                                                            quest_section_added = true;
                                                            Some("Intentions & Quests".to_string())
                                                        } else if artifact.artifact_type == "exchange" && !exchange_section_added {
                                                            exchange_section_added = true;
                                                            Some("Exchanges".to_string())
                                                        } else if artifact.artifact_type == "collection" && !tokens_section_added {
                                                            tokens_section_added = true;
                                                            Some("Tokens".to_string())
                                                        } else {
                                                            None
                                                        };

                                                        let node_id = format!("{:?}", aref.artifact_id);
                                                        let has_children = !artifact.references.is_empty();

                                                        nodes.push(VaultTreeNode {
                                                            id: node_id,
                                                            artifact_id: Some(aref.artifact_id.clone()),
                                                            label: label_str,
                                                            icon: icon.to_string(),
                                                            heat_level: heat_lvl,
                                                            depth: 0,
                                                            has_children,
                                                            expanded: has_children,
                                                            section,
                                                            view_type: view_type_str.to_string(),
                                                        });
                                                    }
                                                }

                                                workspace.write().nav.vault_tree = nodes;
                                            }
                                        }
                                    }

                                    #[cfg(feature = "lua-scripting")]
                                    if let Some(ref tx) = *lua_event_tx.read() {
                                        let _ = tx.send(AppEvent::PeerConnected(entry.name.clone()));
                                    }
                                }
                            }

                            workspace.write().peers.entries = entries;
                        }
                    }
                }

                // Periodic world view save every ~30 seconds
                if tick % 15 == 0 {
                    if let Some(nh) = network_handle.read().as_ref() {
                        let _ = nh.network.save_world_view().await;
                    }
                }
                tick += 1;
            }
        });
    });

    // Subscribe to global network events and feed into event log
    use_effect(move || {
        spawn(async move {
            use futures::StreamExt;

            // Wait for network to be available
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if workspace.read().phase == AppPhase::Workspace {
                    break;
                }
            }

            let net = {
                let guard = network_handle.read();
                guard.as_ref().map(|nh| nh.network.clone())
            };
            let Some(net) = net else { return; };

            let mut events = std::pin::pin!(net.events());
            while let Some(event) = events.next().await {
                let description = format!("{:?}", event.event.event);
                // Extract just the variant name (e.g. "Message", "MembershipChange", "Presence")
                let event_type = description.split_once('{')
                    .or_else(|| description.split_once('('))
                    .map(|(prefix, _)| prefix.trim())
                    .unwrap_or(&description);

                let realm_short = format!("{}", event.realm_id);
                let msg = format!("[{}] {}", &realm_short[..8.min(realm_short.len())], event_type);

                log_event(&mut workspace.write(), EventDirection::Received, msg);
            }
        });
    });

    // --- Event handlers ---

    let mut on_tree_click = {
        let vault_handle = vault_handle.clone();
        move |node_id: String| {
            // Find the node in the tree to get its artifact_id and view_type
            let tree = workspace.read().nav.vault_tree.clone();
            let node = tree.iter().find(|n| n.id == node_id).cloned();

            if let Some(node) = node {
                let label = node.label.clone();
                let view_type_str = node.view_type.clone();
                let artifact_id = node.artifact_id.clone();

                // Update navigation state
                workspace.write().nav.navigate_to(node_id.clone(), label.clone());

                // Set the active view type
                let vt = match view_type_str.as_str() {
                    "story" => ViewType::Story,
                    "quest" => ViewType::Quest,
                    _ => ViewType::Document,
                };
                workspace.write().ui.active_view = vt.clone();

                // Load artifact data
                if let Some(artifact_id) = artifact_id {
                    let vh = vault_handle.read().clone();
                    if let Some(vh) = vh {
                        let tree_node_id = node_id.clone();
                        spawn(async move {
                            let vault = vh.vault.lock().await;
                            let _now = chrono::Utc::now().timestamp_millis();

                            // Record navigation attention event
                            // (vault is borrowed immutably, skip navigate_to here
                            //  since we need &mut; will be done when we add write support)

                            if let Ok(Some(artifact)) = vault.get_artifact(&artifact_id) {
                                {
                                    let audience_count = artifact.grants.len();
                                    let steward_is_self = artifact.steward == vh.player_id;
                                    let steward_name = if steward_is_self {
                                        vh.player_name.clone()
                                    } else {
                                        // Look up peer name
                                        vault.peers().iter()
                                            .find(|p| p.peer_id == artifact.steward)
                                            .and_then(|p| p.display_name.clone())
                                            .unwrap_or_else(|| "Unknown".to_string())
                                    };

                                    match vt {
                                        ViewType::Settings => {}
                                        ViewType::Document => {
                                            // Load blocks from artifact references
                                            let mut blocks = Vec::new();
                                            for child_ref in &artifact.references {
                                                // Get leaf payload for content
                                                let content = if let Ok(Some(payload)) = vault.get_payload(&child_ref.artifact_id) {
                                                    String::from_utf8_lossy(&payload).to_string()
                                                } else {
                                                    String::new()
                                                };

                                                let block = EditorState::parse_block_from_label(
                                                    &child_ref.label,
                                                    content,
                                                    Some(format!("{:?}", child_ref.artifact_id)),
                                                );
                                                blocks.push(block);
                                            }

                                            let editor = EditorState {
                                                title: label.clone(),
                                                blocks,
                                                meta: DocumentMeta {
                                                    doc_type: "Document".to_string(),
                                                    audience_count,
                                                    steward_name,
                                                    is_own_steward: steward_is_self,
                                                    created_at: String::new(),
                                                    edited_ago: "just now".to_string(),
                                                },
                                                tree_id: Some(artifact_id.clone()),
                                            };
                                            workspace.write().editor = editor;
                                        }
                                        ViewType::Story => {
                                            // Load story messages from tree references
                                            // Extended label format: "msg:Name[:artifact:ArtName:ArtType][:image][:branch:Label][:day:DayLabel]"
                                            let mut msgs = Vec::new();
                                            let times = ["14:22", "14:25", "14:31", "14:33", "14:38", "14:40", "09:14", "09:22"];
                                            for (i, child_ref) in artifact.references.iter().enumerate() {
                                                let content = if let Ok(Some(payload)) = vault.get_payload(&child_ref.artifact_id) {
                                                    String::from_utf8_lossy(&payload).to_string()
                                                } else {
                                                    String::new()
                                                };

                                                let label_str = child_ref.label.as_deref().unwrap_or("");
                                                let parts: Vec<&str> = label_str.split(':').collect();

                                                // First part is "msg", second is sender name
                                                let sender_name = parts.get(1).unwrap_or(&"Unknown").to_string();
                                                let is_self = sender_name == vh.player_name;
                                                let letter = sender_name.chars().next().unwrap_or('?').to_string();
                                                let color_class = format!("peer-dot-{}", sender_name.to_lowercase());
                                                let time = times.get(i).unwrap_or(&"now").to_string();

                                                // Parse optional rich metadata from label parts
                                                let mut artifact_ref = None;
                                                let mut image_ref = None;
                                                let mut branch_label = None;
                                                let mut day_separator = None;

                                                let mut j = 2;
                                                while j < parts.len() {
                                                    match parts[j] {
                                                        "artifact" => {
                                                            let art_name = parts.get(j + 1).unwrap_or(&"").to_string();
                                                            let art_type = parts.get(j + 2).unwrap_or(&"Document").to_string();
                                                            artifact_ref = Some(StoryArtifactRef {
                                                                icon: "\u{1F4C4}".to_string(),
                                                                name: art_name,
                                                                artifact_type: art_type,
                                                                artifact_id: None,
                                                            });
                                                            j += 3;
                                                        }
                                                        "image" => {
                                                            image_ref = Some(true);
                                                            j += 1;
                                                        }
                                                        "branch" => {
                                                            branch_label = Some(parts.get(j + 1).unwrap_or(&"").to_string());
                                                            j += 2;
                                                        }
                                                        "day" => {
                                                            day_separator = Some(parts.get(j + 1).unwrap_or(&"").to_string());
                                                            j += 2;
                                                        }
                                                        _ => { j += 1; }
                                                    }
                                                }

                                                msgs.push(StoryMessage {
                                                    sender_name,
                                                    sender_letter: letter,
                                                    sender_color_class: color_class,
                                                    content,
                                                    time,
                                                    is_self,
                                                    artifact_ref,
                                                    image_ref,
                                                    branch_label,
                                                    day_separator,
                                                    message_id: None,
                                                    reactions: vec![],
                                                    reply_to_preview: None,
                                                });
                                            }
                                            story_messages.set(msgs);

                                            // Also set editor meta for topbar steward display
                                            workspace.write().editor.meta.steward_name = steward_name;
                                            workspace.write().editor.meta.audience_count = audience_count;
                                            workspace.write().editor.title = label.clone();
                                        }
                                        ViewType::Quest => {
                                            let kind = match artifact.artifact_type.as_str() {
                                                "need" => QuestKind::Need,
                                                "offering" => QuestKind::Offering,
                                                "intention" => QuestKind::Intention,
                                                _ => QuestKind::Quest,
                                            };

                                            // Build description from first leaf if any
                                            let description = if let Some(first_ref) = artifact.references.first() {
                                                if let Ok(Some(payload)) = vault.get_payload(&first_ref.artifact_id) {
                                                    String::from_utf8_lossy(&payload).to_string()
                                                } else {
                                                    "No description yet.".to_string()
                                                }
                                            } else {
                                                "No description yet.".to_string()
                                            };

                                            let steward_for_meta = steward_name.clone();

                                            // Build proofs for "Build P2P Workspace" quest
                                            let proofs = if label == "Build P2P Workspace" {
                                                vec![
                                                    ProofEntry {
                                                        author_name: "Sage".into(),
                                                        author_letter: "S".into(),
                                                        author_color_class: "peer-dot-sage".into(),
                                                        body: "Completed the responsive mockup with all three breakpoints \u{2014} desktop, tablet, mobile. Implemented swipe gestures, bottom nav, Anytype-inspired overlay patterns, and the full Quiet Protocol design system.".into(),
                                                        time_ago: "2d ago".into(),
                                                        artifact_attachments: vec![
                                                            ProofArtifact { icon: "\u{1F4C4}".into(), name: "indras_workspace.html".into(), artifact_type: "File".into() },
                                                        ],
                                                        tokens: vec![
                                                            AssignedToken { duration: "14m 22s".into(), source: "Architecture Notes".into() },
                                                            AssignedToken { duration: "8m 47s".into(), source: "Team Discussion".into() },
                                                            AssignedToken { duration: "3m 11s".into(), source: "Design Assets".into() },
                                                        ],
                                                        has_tokens: true,
                                                        total_token_count: 3,
                                                        total_token_duration: "26m 20s".into(),
                                                    },
                                                    ProofEntry {
                                                        author_name: "Zephyr".into(),
                                                        author_letter: "Z".into(),
                                                        author_color_class: "peer-dot-zeph".into(),
                                                        body: "Created the system architecture diagram showing all artifact types, their relationships, and the attention flow. Also wrote the TreeType enum documentation with inline examples.".into(),
                                                        time_ago: "18h ago".into(),
                                                        artifact_attachments: vec![
                                                            ProofArtifact { icon: "\u{1F5BC}".into(), name: "system_diagram.svg".into(), artifact_type: "Image".into() },
                                                            ProofArtifact { icon: "\u{1F4C4}".into(), name: "treetype_docs.md".into(), artifact_type: "File".into() },
                                                        ],
                                                        tokens: Vec::new(),
                                                        has_tokens: false,
                                                        total_token_count: 0,
                                                        total_token_duration: String::new(),
                                                    },
                                                ]
                                            } else {
                                                Vec::new()
                                            };

                                            let status_str = if label == "Build P2P Workspace" { "Proven" } else { "Open" };
                                            let posted_ago_str = if label == "Build P2P Workspace" { "Posted 5 days ago" } else { "" };

                                            quest_data.set(Some(QuestViewData {
                                                kind,
                                                title: label.clone(),
                                                description,
                                                status: status_str.to_string(),
                                                steward_name,
                                                audience_count,
                                                proofs,
                                                posted_ago: posted_ago_str.to_string(),
                                            }));

                                            // Set editor meta for topbar
                                            workspace.write().editor.meta.steward_name = steward_for_meta;
                                            workspace.write().editor.meta.audience_count = audience_count;
                                            workspace.write().editor.title = label.clone();
                                        }
                                    }
                                }
                            }

                            // If viewing a Document and this tree has a realm, load from CRDT.
                            // Drop the vault lock first before the async realm call.
                            drop(vault);
                            if vt == ViewType::Document {
                                let realm = realm_map.read().get(&tree_node_id).cloned();
                                if let Some(realm) = realm {
                                    match realm.document::<BlockDocumentSchema>("blocks").await {
                                        Ok(doc) => {
                                            let data = doc.read().await;
                                            if !data.blocks.is_empty() {
                                                let blocks = data.to_blocks();
                                                workspace.write().editor.blocks = blocks;
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(error = %e, "Failed to load CRDT document for Document view");
                                        }
                                    }
                                }
                            }

                            // If this tree has an associated realm, load chat from CRDT document.
                            // Subscribe to changes BEFORE reading initial state to avoid
                            // missing messages that arrive between load and subscription.
                            let realm = realm_map.read().get(&tree_node_id).cloned();
                            if let Some(realm) = realm {
                                match realm.chat_doc().await {
                                    Ok(doc) => {
                                        // 1. Spawn changes listener FIRST (creates broadcast subscription)
                                        let doc_for_changes = doc.clone();
                                        spawn(async move {
                                            use futures::StreamExt;
                                            let mut changes = Box::pin(doc_for_changes.changes());
                                            while let Some(_change) = changes.next().await {
                                                let (my_name, my_id) = {
                                                    let guard = network_handle.read();
                                                    let name = guard.as_ref()
                                                        .and_then(|nh| nh.network.display_name().map(|s| s.to_string()))
                                                        .unwrap_or_default();
                                                    let id = guard.as_ref()
                                                        .map(|nh| member_id_hex(&nh.network.id()));
                                                    (name, id)
                                                };
                                                let data = doc_for_changes.read().await;
                                                let messages: Vec<StoryMessage> = data.visible_messages().iter()
                                                    .map(|msg| chat_msg_to_story(msg, &my_name, my_id.as_deref(), &data))
                                                    .collect();
                                                story_messages.set(messages);
                                            }
                                        });

                                        // 2. Read initial state (subscription already active above)
                                        let (my_name, my_id) = {
                                            let guard = network_handle.read();
                                            let name = guard.as_ref()
                                                .and_then(|nh| nh.network.display_name().map(|s| s.to_string()))
                                                .unwrap_or_default();
                                            let id = guard.as_ref()
                                                .map(|nh| member_id_hex(&nh.network.id()));
                                            (name, id)
                                        };
                                        let data = doc.read().await;
                                        let realm_messages: Vec<StoryMessage> = data.visible_messages().iter()
                                            .map(|msg| chat_msg_to_story(msg, &my_name, my_id.as_deref(), &data))
                                            .collect();
                                        if vt == ViewType::Story {
                                            story_messages.set(realm_messages);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "Failed to load chat document");
                                    }
                                }
                            }

                            // Mark messages as read for stories
                            if vt == ViewType::Story {
                                let realm_for_read = realm_map.read().get(&tree_node_id).cloned();
                                if let Some(realm) = realm_for_read {
                                    let my_id = {
                                        let guard = network_handle.read();
                                        guard.as_ref().map(|nh| nh.network.id())
                                    };
                                    if let Some(mid) = my_id {
                                        if let Err(e) = realm.mark_read(mid).await {
                                            tracing::warn!(error = %e, "Failed to mark messages as read");
                                        }
                                    }
                                }
                            }

                            // Load realm members for peer strip
                            let realm2 = realm_map.read().get(&tree_node_id).cloned();
                            if let Some(realm) = realm2 {
                                if let Ok(members) = realm.member_list().await {
                                    let peer_colors = [
                                        "peer-dot-sage", "peer-dot-zeph", "peer-dot-rose",
                                    ];
                                    let my_id = {
                                        let guard = network_handle.read();
                                        guard.as_ref().map(|nh| nh.network.id())
                                    };
                                    let entries: Vec<PeerDisplayInfo> = members.iter()
                                        .filter(|m| my_id.map_or(true, |mid| m.id() != mid))
                                        .enumerate()
                                        .map(|(i, m)| {
                                            let name = m.name();
                                            let letter = name.chars().next().unwrap_or('?').to_string();
                                            PeerDisplayInfo {
                                                name,
                                                letter,
                                                color_class: peer_colors[i % peer_colors.len()].to_string(),
                                                online: true,
                                                player_id: m.id(),
                                            }
                                        })
                                        .collect();
                                    if !entries.is_empty() {
                                        workspace.write().peers.entries = entries;
                                    }
                                }

                                // Subscribe to live member events for presence updates
                                let realm_for_events = realm_map.read().get(&tree_node_id).cloned();
                                if let Some(realm) = realm_for_events {
                                    let my_id = {
                                        let guard = network_handle.read();
                                        guard.as_ref().map(|nh| nh.network.id())
                                    };
                                    spawn(async move {
                                        use futures::StreamExt;
                                        let mut stream = Box::pin(realm.member_events());
                                        let peer_colors = [
                                            "peer-dot-sage", "peer-dot-zeph", "peer-dot-rose",
                                        ];
                                        while let Some(event) = stream.next().await {
                                            match event {
                                                indras_network::MemberEvent::Joined(member) => {
                                                    if my_id.map_or(true, |mid| member.id() != mid) {
                                                        let name = member.name();
                                                        let letter = name.chars().next().unwrap_or('?').to_string();
                                                        let mut peers = workspace.read().peers.entries.clone();
                                                        if !peers.iter().any(|p| p.player_id == member.id()) {
                                                            let idx = peers.len();
                                                            peers.push(PeerDisplayInfo {
                                                                name,
                                                                letter,
                                                                color_class: peer_colors[idx % peer_colors.len()].to_string(),
                                                                online: true,
                                                                player_id: member.id(),
                                                            });
                                                            workspace.write().peers.entries = peers;
                                                        }
                                                    }
                                                }
                                                indras_network::MemberEvent::Left(member) => {
                                                    let mut peers = workspace.read().peers.entries.clone();
                                                    peers.retain(|p| p.player_id != member.id());
                                                    workspace.write().peers.entries = peers;
                                                }
                                                _ => {}
                                            }
                                        }
                                    });
                                }

                                // Changes subscription moved earlier (next to chat_doc load)
                                // to eliminate gap where messages could be missed.
                            }
                        });
                    }
                }
            }
        }
    };

    // Peer click → find Contact tree by label and navigate to it
    let on_peer_click_handler = move |peer_name: String| {
        let tree = workspace.read().nav.vault_tree.clone();
        if let Some(node) = tree.iter().find(|n| n.label == peer_name) {
            on_tree_click(node.id.clone());
        }
    };

    let on_tree_toggle = move |node_id: String| {
        workspace.write().nav.toggle_expand(&node_id);
        // Update the expanded state in the vault tree nodes
        let _expanded = workspace.read().nav.expanded_nodes.contains(&node_id);
        for node in &mut workspace.write().nav.vault_tree {
            if node.id == node_id {
                node.expanded = !node.expanded;
                break;
            }
        }
    };

    let on_crumb_click = move |crumb_id: String| {
        // Navigate to the breadcrumb target
        if crumb_id == "root" {
            workspace.write().nav.current_id = None;
            workspace.write().nav.breadcrumbs.truncate(1);
        }
    };

    let on_toggle_detail = move |_: ()| {
        let is_open = workspace.read().ui.detail_open;
        workspace.write().ui.detail_open = !is_open;
    };

    let on_toggle_sidebar = move |_: ()| {
        let is_open = workspace.read().ui.sidebar_open;
        tracing::info!("Hamburger clicked: sidebar_open {} -> {}", is_open, !is_open);
        workspace.write().ui.sidebar_open = !is_open;
    };

    let on_slash_select = {
        let vault_handle = vault_handle.clone();
        move |action: SlashAction| {
            workspace.write().ui.slash_menu_open = false;

            // Clone signals for async block
            let vh_signal = vault_handle.clone();
            let mut ws_signal = workspace.clone();

            spawn(async move {
                let vh = match vh_signal.read().clone() {
                    Some(h) => h,
                    None => return,
                };

                let mut vault = vh.vault.lock().await;
                let now = chrono::Utc::now().timestamp_millis();

                match action {
                    // Tree actions - create new tree and add to root
                    SlashAction::Document | SlashAction::Story | SlashAction::Quest |
                    SlashAction::Need | SlashAction::Offering | SlashAction::Intention => {
                        let tree_type = match action {
                            SlashAction::Document => "document",
                            SlashAction::Story => "story",
                            SlashAction::Quest => "quest",
                            SlashAction::Need => "need",
                            SlashAction::Offering => "offering",
                            SlashAction::Intention => "intention",
                            _ => unreachable!(),
                        };

                        let label = match action {
                            SlashAction::Document => "Untitled Document",
                            SlashAction::Story => "Untitled Story",
                            SlashAction::Quest => "Untitled Quest",
                            SlashAction::Need => "Untitled Need",
                            SlashAction::Offering => "Untitled Offering",
                            SlashAction::Intention => "Untitled Intention",
                            _ => unreachable!(),
                        };

                        // Create the tree
                        let audience = vec![vh.player_id];
                        let tree = match vault.place_tree(tree_type, audience, now) {
                            Ok(t) => t,
                            Err(e) => {
                                tracing::error!("Failed to create tree: {}", e);
                                return;
                            }
                        };

                        // Add to root (read position from store, not stale root field)
                        let root_id = vault.root.id.clone();
                        let root_for_pos = match vault.get_artifact(&root_id) {
                            Ok(Some(a)) => a,
                            _ => return,
                        };
                        let position = root_for_pos.references.len() as u64;
                        drop(root_for_pos);
                        if let Err(e) = vault.compose(&root_id, tree.id.clone(), position, Some(label.to_string())) {
                            tracing::error!("Failed to add tree to root: {}", e);
                            return;
                        }

                        // Create a network Realm for this tree (enables messaging/sync)
                        let tree_node_id = format!("{:?}", tree.id);
                        drop(vault); // Release vault lock before async network call
                        let net = {
                            let guard = network_handle.read();
                            guard.as_ref().map(|nh| nh.network.clone())
                        };
                        if let Some(net) = net {
                            match net.create_realm(label).await {
                                Ok(realm) => {
                                    tracing::info!("Created realm for tree: {}", label);
                                    log_event(&mut ws_signal.write(), EventDirection::System, format!("Realm created: {}", label));
                                    realm_map.write().insert(tree_node_id.clone(), realm);

                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to create realm for tree (non-fatal)");
                                }
                            }
                        }
                        // Re-acquire vault lock for sidebar rebuild
                        let vh = match vh_signal.read().clone() {
                            Some(h) => h,
                            None => return,
                        };
                        let vault = vh.vault.lock().await;

                        // Rebuild sidebar tree (read from store, not stale root field)
                        let mut nodes = Vec::new();
                        let rebuilt_root = match vault.get_artifact(&vault.root.id) {
                            Ok(Some(a)) => a,
                            _ => return,
                        };
                        let root_refs = &rebuilt_root.references;
                        let mut quest_section_added = false;
                        let mut exchange_section_added = false;
                        let mut tokens_section_added = false;
                        let mut contacts_section_added = false;

                        for aref in root_refs {
                            if let Ok(Some(artifact)) = vault.get_artifact(&aref.artifact_id) {
                                let view_type_str = NavigationState::view_type_for(&artifact.artifact_type);
                                let icon = NavigationState::icon_for_type(&artifact.artifact_type);
                                let heat_val = vault.heat(&aref.artifact_id, now).unwrap_or(0.0);
                                let heat_lvl = indras_ui::heat_level(heat_val);
                                let label_str = aref.label.clone().unwrap_or_default();

                                let is_quest_type = matches!(
                                    artifact.artifact_type.as_str(),
                                    "quest" | "need" | "offering" | "intention"
                                );
                                let is_contact = artifact.artifact_type == "contact";
                                let section = if nodes.is_empty() {
                                    Some("Vault".to_string())
                                } else if is_contact && !contacts_section_added {
                                    contacts_section_added = true;
                                    Some("Contacts".to_string())
                                } else if is_quest_type && !quest_section_added {
                                    quest_section_added = true;
                                    Some("Intentions & Quests".to_string())
                                } else if artifact.artifact_type == "exchange" && !exchange_section_added {
                                    exchange_section_added = true;
                                    Some("Exchanges".to_string())
                                } else if artifact.artifact_type == "collection" && !tokens_section_added {
                                    tokens_section_added = true;
                                    Some("Tokens".to_string())
                                } else {
                                    None
                                };

                                let node_id = format!("{:?}", aref.artifact_id);
                                let has_children = !artifact.references.is_empty();

                                nodes.push(VaultTreeNode {
                                    id: node_id.clone(),
                                    artifact_id: Some(aref.artifact_id.clone()),
                                    label: label_str.clone(),
                                    icon: icon.to_string(),
                                    heat_level: heat_lvl,
                                    depth: 0,
                                    has_children,
                                    expanded: has_children,
                                    section,
                                    view_type: view_type_str.to_string(),
                                });

                                if has_children {
                                    for child_ref in &artifact.references {
                                        if let Ok(Some(child_artifact)) = vault.get_artifact(&child_ref.artifact_id) {
                                            let child_vt = NavigationState::view_type_for(&child_artifact.artifact_type);
                                            let child_icon = NavigationState::icon_for_type(&child_artifact.artifact_type);
                                            let child_heat = vault.heat(&child_ref.artifact_id, now).unwrap_or(0.0);
                                            let child_heat_lvl = indras_ui::heat_level(child_heat);
                                            let child_label = child_ref.label.clone().unwrap_or_default();
                                            let child_node_id = format!("{:?}", child_ref.artifact_id);

                                            nodes.push(VaultTreeNode {
                                                id: child_node_id,
                                                artifact_id: Some(child_ref.artifact_id.clone()),
                                                label: child_label,
                                                icon: child_icon.to_string(),
                                                heat_level: child_heat_lvl,
                                                depth: 1,
                                                has_children: false,
                                                expanded: false,
                                                section: None,
                                                view_type: child_vt.to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        ws_signal.write().nav.vault_tree = nodes;

                        // Navigate to the new tree
                        let new_node_id = format!("{:?}", tree.id);
                        ws_signal.write().nav.navigate_to(new_node_id.clone(), label.to_string());

                        // Log artifact creation
                        log_event(&mut ws_signal.write(), EventDirection::System, format!("Created: {}", label));
                    }

                    // Leaf block actions - add to currently active tree
                    SlashAction::Text | SlashAction::Heading | SlashAction::Code |
                    SlashAction::Callout | SlashAction::Todo | SlashAction::Image | SlashAction::Divider => {
                        // Find the active tree via current_id
                        let active_tree_id = {
                            let ws = ws_signal.read();
                            ws.nav.current_id.as_ref()
                                .and_then(|cid| ws.nav.vault_tree.iter()
                                    .find(|n| &n.id == cid)
                                    .and_then(|n| n.artifact_id.clone()))
                        };

                        let active_tree_id = match active_tree_id {
                            Some(id) => id,
                            None => {
                                tracing::warn!("No active tree to add block to");
                                return;
                            }
                        };

                        // Create default content based on type
                        let (content, label_str) = match action {
                            SlashAction::Text => ("", "text"),
                            SlashAction::Heading => ("New heading", "heading:2"),
                            SlashAction::Code => ("", "code"),
                            SlashAction::Callout => ("", "callout"),
                            SlashAction::Todo => ("New task", "todo"),
                            SlashAction::Image => ("", "image"),
                            SlashAction::Divider => ("", "divider"),
                            _ => unreachable!(),
                        };

                        // Create the leaf
                        let leaf = match vault.place_leaf(content.as_bytes(), String::new(), None, "message", now) {
                            Ok(l) => l,
                            Err(e) => {
                                tracing::error!("Failed to create leaf: {}", e);
                                return;
                            }
                        };

                        // Get position in the active tree
                        let position = if let Ok(Some(active_art)) = vault.get_artifact(&active_tree_id) {
                            active_art.references.len() as u64
                        } else {
                            0
                        };

                        // Add to active tree
                        if let Err(e) = vault.compose(&active_tree_id, leaf.id.clone(), position, Some(label_str.to_string())) {
                            tracing::error!("Failed to add leaf to tree: {}", e);
                            return;
                        }

                        // Reload the document's blocks
                        if let Ok(Some(active_art)) = vault.get_artifact(&active_tree_id) {
                            let mut blocks = Vec::new();
                            for child_ref in &active_art.references {
                                let content = if let Ok(Some(payload)) = vault.get_payload(&child_ref.artifact_id) {
                                    String::from_utf8_lossy(&payload).to_string()
                                } else {
                                    String::new()
                                };

                                let block = EditorState::parse_block_from_label(
                                    &child_ref.label,
                                    content,
                                    Some(format!("{:?}", child_ref.artifact_id)),
                                );
                                blocks.push(block);
                            }

                            ws_signal.write().editor.blocks = blocks;
                        }
                    }
                }
            });
        }
    };

    let on_slash_close = move |_: ()| {
        workspace.write().ui.slash_menu_open = false;
    };

    let on_detail_tab_change = move |tab: usize| {
        workspace.write().ui.active_detail_tab = tab;
    };

    let on_detail_close = move |_: ()| {
        workspace.write().ui.detail_open = false;
    };

    let on_share = move |_: ()| {
        workspace.write().ui.detail_open = true;
        workspace.write().ui.active_detail_tab = 1; // Audience tab
    };

    let on_fab_click = move |_: ()| {
        workspace.write().ui.slash_menu_open = true;
    };

    let on_settings = move |_: ()| {
        workspace.write().ui.active_view = ViewType::Settings;
    };

    let on_tab_change = move |tab: NavTab| {
        let is_profile = tab == NavTab::Profile;
        active_tab.set(tab);
        if is_profile {
            workspace.write().ui.active_view = ViewType::Settings;
        }
    };

    // --- Build render data ---

    let ws = workspace.read();
    let detail_open = ws.ui.detail_open;
    let sidebar_open = ws.ui.sidebar_open;
    let slash_menu_open = ws.ui.slash_menu_open;
    let active_detail_tab = ws.ui.active_detail_tab;
    let active_view = ws.ui.active_view.clone();
    let breadcrumbs = ws.nav.breadcrumbs.clone();
    let steward_name = if ws.editor.meta.steward_name.is_empty() {
        None
    } else {
        Some(ws.editor.meta.steward_name.clone())
    };

    // Convert VaultTreeNode -> indras_ui::TreeNode for sidebar, filtering collapsed children
    let sidebar_nodes: Vec<TreeNode> = {
        let current = ws.nav.current_id.as_ref();
        let mut nodes = Vec::new();
        let mut skip_depth: Option<usize> = None;
        for n in ws.nav.vault_tree.iter() {
            // Skip children of collapsed parents
            if let Some(sd) = skip_depth {
                if n.depth > sd {
                    continue;
                } else {
                    skip_depth = None;
                }
            }
            // If this node has children but is collapsed, skip its children
            if n.has_children && !n.expanded {
                skip_depth = Some(n.depth);
            }
            nodes.push(TreeNode {
                id: n.id.clone(),
                label: n.label.clone(),
                icon: n.icon.clone(),
                heat_level: n.heat_level,
                depth: n.depth,
                has_children: n.has_children,
                expanded: n.expanded,
                active: current == Some(&n.id),
                section: n.section.clone(),
                view_type: n.view_type.clone(),
            });
        }
        nodes
    };

    // Build peer strip data
    let ui_peers: Vec<UiPeerDisplayInfo> = ws.peers.entries.iter().map(|p| {
        UiPeerDisplayInfo {
            name: p.name.clone(),
            letter: p.letter.clone(),
            color_class: p.color_class.clone(),
            online: p.online,
        }
    }).collect();

    // Player identity
    let player_name = vault_handle.read().as_ref()
        .map(|vh| vh.player_name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let player_letter = player_name.chars().next().unwrap_or('?').to_string();
    let player_short_id = network_handle.read().as_ref()
        .map(|nh| {
            let code = nh.network.identity_code();
            if code.len() > 14 {
                format!("{}...{}", &code[..10], &code[code.len()-4..])
            } else {
                code
            }
        })
        .unwrap_or_else(|| "not connected".to_string());
    let identity_uri = network_handle.read().as_ref()
        .map(|nh| nh.network.identity_uri());

    // Build detail panel data
    let properties = vec![
        PropertyRow { key: "Type".to_string(), value: ws.editor.meta.doc_type.clone(), accent: true },
        PropertyRow { key: "Steward".to_string(), value: ws.editor.meta.steward_name.clone(), accent: true },
        PropertyRow { key: "Audience".to_string(), value: format!("{} members", ws.editor.meta.audience_count), accent: false },
        PropertyRow { key: "Created".to_string(), value: ws.editor.meta.created_at.clone(), accent: false },
        PropertyRow { key: "Edited".to_string(), value: ws.editor.meta.edited_ago.clone(), accent: false },
    ];

    let audience_members: Vec<AudienceMember> = {
        let mut members = vec![AudienceMember {
            name: player_name.clone(),
            letter: player_letter.clone(),
            color_class: String::new(),
            role: "steward".to_string(),
            short_id: player_short_id.clone(),
        }];
        for peer in ws.peers.entries.iter() {
            let short_id = peer.player_id.iter().take(4).map(|b| format!("{:02x}", b)).collect::<String>();
            members.push(AudienceMember {
                name: peer.name.clone(),
                letter: peer.letter.clone(),
                color_class: peer.color_class.clone(),
                role: "peer".to_string(),
                short_id,
            });
        }
        members
    };

    // Build references from current editor blocks
    let references: Vec<ReferenceItem> = ws.editor.blocks.iter().take(5).map(|block| {
        let (icon, ref_type) = match block {
            crate::state::editor::Block::Code { .. } => ("\u{1F4BB}", "code"),
            crate::state::editor::Block::Image { .. } => ("\u{1F5FA}", "image"),
            crate::state::editor::Block::Heading { .. } => ("\u{1F4D1}", "heading"),
            _ => ("\u{1F4C4}", "text"),
        };
        let content = block.content();
        let name = if content.len() > 40 { format!("{}...", &content[..40]) } else { content.to_string() };
        ReferenceItem { icon: icon.into(), name, ref_type: ref_type.into() }
    }).collect();
    let refs_count = ws.editor.blocks.len();

    // Build sync entries from real peers
    let sync_entries: Vec<SyncEntry> = {
        let mut entries = vec![
            SyncEntry { name: format!("{} (local)", player_name), status: "synced".into(), status_text: "up to date".into() },
        ];
        for peer in ws.peers.entries.iter() {
            entries.push(SyncEntry {
                name: peer.name.clone(),
                status: if peer.online { "syncing" } else { "offline" }.into(),
                status_text: if peer.online { "connected".into() } else { "offline".into() },
            });
        }
        entries
    };

    let heat_entries = vec![
        HeatEntry { label: player_name.clone(), value: 0.7, color: "var(--accent-teal)".to_string() },
        HeatEntry { label: "Sage".to_string(), value: 0.5, color: "var(--accent-violet)".to_string() },
        HeatEntry { label: "Zephyr".to_string(), value: 0.3, color: "var(--accent-gold)".to_string() },
    ];

    let trail_events = vec![
        TrailEvent { time: "2m ago".to_string(), target: "Architecture Notes".to_string() },
        TrailEvent { time: "15m ago".to_string(), target: "Team Discussion".to_string() },
        TrailEvent { time: "40m ago".to_string(), target: "Build P2P Workspace".to_string() },
    ];

    let editor = ws.editor.clone();
    let current_story_messages = story_messages.read().clone();
    let current_quest_data = quest_data.read().clone();
    let current_active_tab = active_tab.read().clone();

    drop(ws); // Release the read borrow

    // --- Layout classes ---

    let mut app_class = "app".to_string();
    if !detail_open {
        app_class.push_str(" detail-closed");
    }
    if !sidebar_open {
        app_class.push_str(" sidebar-closed");
    }

    // Keyboard handler for / and Escape
    let on_keydown = move |evt: KeyboardEvent| {
        let key = evt.key();
        match key {
            Key::Character(ref c) if c == "/" => {
                // Only open if not already typing in an input/textarea
                let slash_open = workspace.read().ui.slash_menu_open;
                if !slash_open {
                    workspace.write().ui.slash_menu_open = true;
                }
            }
            Key::Escape => {
                let slash_open = workspace.read().ui.slash_menu_open;
                if slash_open {
                    workspace.write().ui.slash_menu_open = false;
                } else {
                    workspace.write().ui.detail_open = false;
                }
            }
            _ => {}
        }
    };

    let current_phase = workspace.read().phase.clone();

    match current_phase {
        AppPhase::Loading => rsx! {
            div { class: "setup-container",
                div { class: "setup-card",
                    div { class: "setup-title", "Loading..." }
                }
            }
        },
        AppPhase::Setup => rsx! {
            SetupView {
                on_create: move |(name, pass_story_slots): (String, Option<[String; 23]>)| {
                    setup_loading.set(true);
                    setup_error.set(None);
                    spawn(async move {
                        match create_identity(&name, pass_story_slots).await {
                            Ok(nh) => {
                                let player_id = nh.network.id();
                                let now = chrono::Utc::now().timestamp_millis();
                                match InMemoryVault::in_memory(player_id, now) {
                                    Ok(vault) => {
                                        vault_handle.set(Some(VaultHandle {
                                            vault: Arc::new(Mutex::new(vault)),
                                            player_id,
                                            player_name: name.clone(),
                                        }));
                                        let net = Arc::clone(&nh.network);
                                        network_for_chat.set(Some(Arc::clone(&net)));
                                        network_handle.set(Some(nh));
                                        {
                                            let mut ws = workspace.write();
                                            ws.phase = AppPhase::Workspace;
                                            log_event(&mut ws, EventDirection::System, format!("Identity created: {}", name));
                                        }

                                        // Emit events for Lua scripting
                                        #[cfg(feature = "lua-scripting")]
                                        if let Some(ref tx) = *lua_event_tx.read() {
                                            let _ = tx.send(AppEvent::IdentityCreated(name.clone()));
                                            let _ = tx.send(AppEvent::AppReady);
                                        }

                                        // Start the network (enables inbox listener for incoming connections)
                                        log_event(&mut workspace.write(), EventDirection::System, "Starting network...".to_string());
                                        if let Err(e) = net.start().await {
                                            tracing::warn!(error = %e, "Failed to start network (non-fatal)");
                                            log_event(&mut workspace.write(), EventDirection::System, format!("Network start warning: {}", e));
                                        } else {
                                            log_event(&mut workspace.write(), EventDirection::System, "Network started \u{2014} listening for connections".to_string());
                                        }

                                        // Join contacts realm so inbox listener can store contacts
                                        if let Err(e) = net.join_contacts_realm().await {
                                            tracing::warn!(error = %e, "Failed to join contacts realm (non-fatal)");
                                        }

                                        // Initialize home realm for persistent artifact storage
                                        match net.home_realm().await {
                                            Ok(hr) => {
                                                home_realm_handle.set(Some(hr));
                                                log_event(&mut workspace.write(), EventDirection::System, "Home realm initialized".to_string());
                                            }
                                            Err(e) => {
                                                tracing::warn!(error = %e, "Failed to initialize home realm (non-fatal)");
                                                log_event(&mut workspace.write(), EventDirection::System, format!("Home realm warning: {}", e));
                                            }
                                        }
                                    }
                                    Err(e) => setup_error.set(Some(format!("{}", e))),
                                }
                            }
                            Err(e) => setup_error.set(Some(e)),
                        }
                        setup_loading.set(false);
                    });
                },
                error: setup_error.read().clone(),
                loading: *setup_loading.read(),
            }
        },
        AppPhase::Workspace => rsx! {
            div {
                class: "{app_class}",
                tabindex: "0",
                onkeydown: on_keydown,

                // Sidebar backdrop (mobile overlay)
                div {
                    class: if sidebar_open { "sidebar-backdrop visible" } else { "sidebar-backdrop" },
                    onclick: move |_| {
                        workspace.write().ui.sidebar_open = false;
                    },
                }

                // Sidebar
                div {
                    class: if sidebar_open { "sidebar open" } else { "sidebar" },

                    div {
                        class: "sidebar-header",
                        IdentityRow {
                            avatar_letter: player_letter.clone(),
                            name: player_name.clone(),
                            short_id: player_short_id.clone(),
                        }
                        button {
                            class: "sidebar-close-btn",
                            onclick: move |_| {
                                workspace.write().ui.sidebar_open = false;
                            },
                            "\u{00d7}"
                        }
                    }

                    PeerStrip {
                        peers: ui_peers,
                        on_peer_click: on_peer_click_handler,
                        on_add_contact: move |_| {
                            // Populate signals from current network handle
                            if let Some(nh) = network_handle.read().as_ref() {
                                contact_invite_uri.set(nh.network.identity_uri());
                                contact_display_name_sig.set(
                                    nh.network.display_name().unwrap_or("Unknown").to_string()
                                );
                                let code = nh.network.identity_code();
                                let short = if code.len() > 14 {
                                    format!("{}...{}", &code[..10], &code[code.len()-4..])
                                } else {
                                    code
                                };
                                contact_member_id_short_sig.set(short);
                            }
                            contact_invite_input.set(String::new());
                            contact_invite_status.set(None);
                            contact_parsed_name.set(None);
                            contact_copy_feedback.set(false);
                            contact_invite_open.set(true);
                        },
                    }

                    VaultSidebar {
                        nodes: sidebar_nodes,
                        on_click: on_tree_click,
                        on_toggle: on_tree_toggle,
                    }

                    div {
                        class: "sidebar-footer",
                        button {
                            class: "sidebar-footer-btn",
                            onclick: move |_| {
                                workspace.write().ui.slash_menu_open = true;
                            },
                            "\u{26A1} New"
                        }
                        button {
                            class: "sidebar-footer-btn",
                            "\u{1F50D} Search"
                        }
                    }
                }

                // Main content area
                div {
                    class: "main",

                    Topbar {
                        breadcrumbs: breadcrumbs,
                        steward_name: steward_name,
                        on_crumb_click: on_crumb_click,
                        on_toggle_detail: on_toggle_detail,
                        on_toggle_sidebar: on_toggle_sidebar,
                        on_share: on_share,
                        on_settings: on_settings,
                    }

                    // Render active view
                    match active_view {
                        ViewType::Settings => {
                            rsx! {
                                SettingsView {
                                    player_name: player_name.clone(),
                                    player_letter: player_letter.clone(),
                                    player_short_id: player_short_id.clone(),
                                    identity_uri: identity_uri.clone(),
                                    network_handle: network_handle,
                                    on_open_pass_story: move |_| {
                                        pass_story_open.set(true);
                                    },
                                }
                            }
                        }
                        ViewType::Document => {
                            if editor.blocks.is_empty() && editor.title.is_empty() {
                                let event_log = workspace.read().event_log.clone();
                                rsx! {
                                    EventLogView { event_log: event_log }
                                }
                            } else {
                                rsx! {
                                    DocumentView {
                                        editor: editor,
                                        vault_handle: vault_handle,
                                        workspace: workspace,
                                        realm_map: realm_map,
                                    }
                                }
                            }
                        }
                        ViewType::Story => {
                            // Check if this is a Contact (relationship story) or regular story
                            // Use current_id rather than active flag — sidebar rebuilds can reset active
                            let is_contact = {
                                let ws = workspace.read();
                                ws.nav.current_id.as_ref()
                                    .and_then(|id| ws.nav.vault_tree.iter().find(|n| &n.id == id))
                                    .map_or(false, |n| n.icon == "\u{1F464}")
                            };

                            if is_contact {
                                // Use ChatPanel for Contact relationship stories
                                let peer_name = editor.title.clone();
                                let chat_peer_id = workspace.read().peers.entries.iter()
                                    .find(|p| p.name == peer_name)
                                    .map(|p| p.player_id)
                                    .unwrap_or([0u8; 32]);
                                let chat_my_id = vault_handle.read().as_ref()
                                    .map(|vh| vh.player_id)
                                    .unwrap_or([0u8; 32]);
                                rsx! {
                                    div {
                                        class: "view active",
                                        ChatPanel {
                                            network: network_for_chat,
                                            peer_id: chat_peer_id,
                                            my_id: chat_my_id,
                                            peer_name: peer_name,
                                        }
                                    }
                                }
                            } else {
                                // Regular story view
                                let title = editor.title.clone();
                                let audience_count = editor.meta.audience_count;
                                let message_count = current_story_messages.len();
                                rsx! {
                                    StoryView {
                                        title: title,
                                        audience_count: audience_count,
                                        message_count: message_count,
                                        messages: current_story_messages,
                                        on_artifact_click: move |aref: StoryArtifactRef| {
                                            // Show preview immediately
                                            preview_file.set(Some(PreviewFile {
                                                name: aref.name.clone(),
                                                content: format!("# {}\n\nType: {}", aref.name, aref.artifact_type),
                                                raw_content: format!("# {}\n\nType: {}", aref.name, aref.artifact_type),
                                                mime_type: "text/markdown".to_string(),
                                                data_url: None,
                                            }));
                                            preview_view_mode.set(PreviewViewMode::Rendered);
                                            preview_open.set(true);

                                            // Log download request (full download pipeline requires ArtifactId propagation)
                                            if let Some(ref aid_str) = aref.artifact_id {
                                                tracing::info!(artifact = %aid_str, name = %aref.name, "Artifact download requested");
                                            }
                                        },
                                        on_send: move |text: String| {
                                            let node_id = workspace.read().nav.current_id.clone();
                                            let realm = node_id.as_ref().and_then(|id| realm_map.read().get(id).cloned());
                                            let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                            spawn(async move {
                                                if let Some(realm) = realm {
                                                    let my_name = net.as_ref()
                                                        .and_then(|n| n.display_name().map(|s| s.to_string()))
                                                        .unwrap_or_else(|| "Me".to_string());
                                                    if let Err(e) = realm.chat_send(&my_name, text).await {
                                                        tracing::error!(error = %e, "Failed to send chat message");
                                                    }
                                                }
                                            });
                                        },
                                        on_reply: move |(msg_id, text): (String, String)| {
                                            let node_id = workspace.read().nav.current_id.clone();
                                            let realm = node_id.as_ref().and_then(|id| realm_map.read().get(id).cloned());
                                            let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                            spawn(async move {
                                                if let Some(realm) = realm {
                                                    let my_name = net.as_ref()
                                                        .and_then(|n| n.display_name().map(|s| s.to_string()))
                                                        .unwrap_or_else(|| "Me".to_string());
                                                    if let Err(e) = realm.chat_reply(&my_name, &msg_id, text).await {
                                                        tracing::error!(error = %e, "Failed to send reply");
                                                    }
                                                }
                                            });
                                        },
                                        on_react: move |(msg_id, emoji): (String, String)| {
                                            let node_id = workspace.read().nav.current_id.clone();
                                            let realm = node_id.as_ref().and_then(|id| realm_map.read().get(id).cloned());
                                            let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                            spawn(async move {
                                                if let Some(realm) = realm {
                                                    let my_name = net.as_ref()
                                                        .and_then(|n| n.display_name().map(|s| s.to_string()))
                                                        .unwrap_or_else(|| "Me".to_string());
                                                    if let Err(e) = realm.chat_react(&my_name, &msg_id, &emoji).await {
                                                        tracing::error!(error = %e, "Failed to send reaction");
                                                    }
                                                }
                                            });
                                        },
                                        on_search: move |query: String| {
                                            let node_id = workspace.read().nav.current_id.clone();
                                            let realm = node_id.as_ref().and_then(|id| realm_map.read().get(id).cloned());
                                            let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                            spawn(async move {
                                                if let Some(realm) = realm {
                                                    if let Ok(doc) = realm.chat_doc().await {
                                                        let my_name = net.as_ref()
                                                            .and_then(|n| n.display_name().map(|s| s.to_string()))
                                                            .unwrap_or_default();
                                                        let my_id = net.as_ref()
                                                            .map(|n| member_id_hex(&n.id()));
                                                        let data = doc.read().await;
                                                        let messages: Vec<StoryMessage> = data.visible_messages().iter()
                                                            .filter(|msg| {
                                                                query.is_empty() || msg.current_content.to_lowercase()
                                                                    .contains(&query.to_lowercase())
                                                            })
                                                            .map(|msg| chat_msg_to_story(msg, &my_name, my_id.as_deref(), &data))
                                                            .collect();
                                                        story_messages.set(messages);
                                                    }
                                                }
                                            });
                                        },
                                        on_attach: move |_: ()| {
                                            let node_id = workspace.read().nav.current_id.clone();
                                            let realm = node_id.as_ref().and_then(|id| realm_map.read().get(id).cloned());
                                            let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                            spawn(async move {
                                                // Open native file picker
                                                let file = rfd::AsyncFileDialog::new()
                                                    .set_title("Share a file")
                                                    .pick_file()
                                                    .await;
                                                let Some(file) = file else { return; };
                                                let path = file.path().to_path_buf();
                                                let file_name = file.file_name();

                                                if let Some(realm) = realm {
                                                    match realm.share_artifact(&path).await {
                                                        Ok(_artifact_id) => {
                                                            // Send a message with artifact reference
                                                            let msg_text = format!("Shared: {}", file_name);
                                                            if let Err(e) = realm.send(msg_text.clone()).await {
                                                                tracing::error!(error = %e, "Failed to send share message");
                                                                return;
                                                            }
                                                            // Add to local UI
                                                            let my_name = net.as_ref()
                                                                .and_then(|n| n.display_name().map(|s| s.to_string()))
                                                                .unwrap_or_else(|| "Me".to_string());
                                                            let letter = my_name.chars().next().unwrap_or('?').to_string();
                                                            let now = chrono::Local::now().format("%H:%M").to_string();
                                                            story_messages.write().push(StoryMessage {
                                                                sender_name: my_name,
                                                                sender_letter: letter,
                                                                sender_color_class: String::new(),
                                                                content: msg_text,
                                                                time: now,
                                                                is_self: true,
                                                                artifact_ref: Some(StoryArtifactRef {
                                                                    icon: "\u{1F4CE}".to_string(),
                                                                    name: file_name,
                                                                    artifact_type: "File".to_string(),
                                                                    artifact_id: Some(format!("{:?}", _artifact_id)),
                                                                }),
                                                                image_ref: None,
                                                                branch_label: None,
                                                                day_separator: None,
                                                                message_id: None,
                                                                reactions: vec![],
                                                                reply_to_preview: None,
                                                            });
                                                        }
                                                        Err(e) => {
                                                            tracing::error!(error = %e, "Failed to share artifact");
                                                        }
                                                    }
                                                }
                                            });
                                        },
                                    }
                                }
                            }
                        }
                        ViewType::Quest => {
                            if let Some(qd) = current_quest_data {
                                let current_token_picker_open = *token_picker_open.read();
                                let current_attention_items = attention_items.clone();
                                rsx! {
                                    QuestView {
                                        kind: qd.kind,
                                        title: qd.title,
                                        description: qd.description,
                                        status: qd.status,
                                        steward_name: qd.steward_name,
                                        audience_count: qd.audience_count,
                                        proofs: qd.proofs,
                                        posted_ago: qd.posted_ago,
                                        token_picker_open: current_token_picker_open,
                                        on_open_token_picker: move |_: ()| {
                                            token_picker_open.set(true);
                                        },
                                        on_close_token_picker: move |_: ()| {
                                            token_picker_open.set(false);
                                        },
                                        attention_items: current_attention_items,
                                    }
                                }
                            } else {
                                rsx! {
                                    div {
                                        class: "view active",
                                        div {
                                            class: "content-scroll",
                                            div {
                                                class: "content-body",
                                                div { class: "doc-title", "Select a quest" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Detail panel
                if detail_open {
                    DetailPanel {
                        active_tab: active_detail_tab,
                        on_tab_change: on_detail_tab_change,
                        on_close: on_detail_close,
                        properties: properties,
                        audience: audience_members,
                        heat_entries: heat_entries.clone(),
                        trail_events: trail_events,
                        references: references,
                        artifact_id_display: workspace.read().nav.current_id.clone().unwrap_or_else(|| "root".to_string()),
                        refs_count: refs_count,
                        steward_name: player_name.clone(),
                        steward_letter: player_letter.clone(),
                        is_own_steward: true,
                        sync_entries: sync_entries,
                        combined_heat: 0.56,
                        on_revoke: move |member_short_id: String| {
                            // Find the full member ID from the short_id
                            let peer = workspace.read().peers.entries.iter()
                                .find(|p| {
                                    let short = p.player_id.iter().take(4).map(|b| format!("{:02x}", b)).collect::<String>();
                                    short == member_short_id
                                })
                                .map(|p| p.player_id);
                            if let Some(member_id) = peer {
                                let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                spawn(async move {
                                    if let Some(net) = net {
                                        if let Some(contacts_realm) = net.contacts_realm().await {
                                            match contacts_realm.remove_contact(&member_id).await {
                                                Ok(true) => {
                                                    tracing::info!("Removed contact {}", member_short_id);
                                                }
                                                Ok(false) => {
                                                    tracing::warn!("Contact not found: {}", member_short_id);
                                                }
                                                Err(e) => {
                                                    tracing::error!(error = %e, "Failed to remove contact");
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                        },
                        on_transfer: move |_: ()| {
                            tracing::info!("Transfer stewardship requested");
                            // Transfer requires selecting a target member — log for now
                        },
                    }
                }

                // Slash menu overlay
                SlashMenu {
                    visible: slash_menu_open,
                    on_select: on_slash_select,
                    on_close: on_slash_close,
                }

                // Preview overlay
                MarkdownPreviewOverlay {
                    is_open: preview_open,
                    file: preview_file,
                    view_mode: preview_view_mode,
                }

                // PassStory overlay
                PassStoryOverlay {
                    visible: *pass_story_open.read(),
                    on_close: move |_| pass_story_open.set(false),
                    on_protect: move |slots: [String; 23]| {
                        // Build PassStory and apply to network
                        let nh_signal = network_handle;
                        spawn(async move {
                            if let Some(_nh) = nh_signal.read().as_ref() {
                                match indras_crypto::PassStory::from_normalized(slots) {
                                    Ok(story) => {
                                        tracing::info!("PassStory created with {} slots", story.slots().len());
                                        // Future: rebuild network with .pass_story(story)
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to create PassStory: {}", e);
                                    }
                                }
                            }
                        });
                        pass_story_open.set(false);
                    },
                }

                // Contact invite overlay
                ContactInviteOverlay {
                    is_open: contact_invite_open,
                    invite_uri: ci_uri,
                    display_name: ci_name,
                    member_id_short: ci_mid,
                    connect_input: contact_invite_input,
                    connect_status: ci_status,
                    parsed_inviter_name: ci_parsed,
                    on_connect: move |uri: String| {
                        contact_invite_status.set(None);
                        spawn(async move {
                            tracing::info!(uri = %uri, "on_connect: parsing identity/invite code");

                            // Parse identity code to extract peer info
                            let (code, peer_name) = match IdentityCode::parse_uri(&uri) {
                                Ok(parsed) => parsed,
                                Err(e) => {
                                    contact_invite_status.set(Some(format!("error:Invalid code: {}", e)));
                                    return;
                                }
                            };
                            let peer_id = code.member_id();
                            let peer_display = peer_name.clone().unwrap_or_else(|| "peer".to_string());

                            // Clone Arcs from signals BEFORE any .await to avoid
                            // holding Signal read guards across await points
                            let net = {
                                let guard = network_handle.read();
                                guard.as_ref().map(|nh| nh.network.clone())
                            };
                            let vh = {
                                let guard = vault_handle.read();
                                guard.clone()
                            };

                            // Track the contact tree node key for realm_map insertion
                            let mut contact_node_key = None::<String>;

                            // Register as peer artifact in the vault
                            if let Some(vh) = vh.as_ref() {
                                let mut vault = vh.vault.lock().await;
                                let now = chrono::Utc::now().timestamp_millis();

                                // Add peer to vault's peer registry
                                if let Err(e) = vault.peer(peer_id, peer_name.clone(), now) {
                                    tracing::warn!("Failed to register peer in vault: {}", e);
                                } else {
                                    log_event(&mut workspace.write(), EventDirection::System, format!("Peer registered: {}", peer_display));
                                }

                                // Create a Contact tree artifact for this peer
                                let audience = vec![vh.player_id, peer_id];
                                match vault.place_tree("contact", audience, now) {
                                    Ok(contact_tree) => {
                                        let contact_tree_id = contact_tree.id.clone();
                                        contact_node_key = Some(format!("{:?}", contact_tree_id));
                                        let contact_payload = serde_json::json!({
                                            "identity_code": uri,
                                            "display_name": peer_name,
                                        }).to_string();

                                        if let Ok(leaf) = vault.place_leaf(
                                            contact_payload.as_bytes(),
                                            String::new(),
                                            None,
                                            "contact_card",
                                            now,
                                        ) {
                                            let label = peer_name.clone().unwrap_or_else(|| "Unknown".to_string());
                                            let _ = vault.compose(
                                                &contact_tree_id,
                                                leaf.id,
                                                0,
                                                Some(label),
                                            );
                                        }

                                        // Add connection event leaf to Contact tree
                                        if let Ok(event_leaf) = vault.place_leaf(
                                            format!("Connected to {}", peer_display).as_bytes(),
                                            String::new(),
                                            None,
                                            "message",
                                            now,
                                        ) {
                                            let _ = vault.compose(
                                                &contact_tree_id,
                                                event_leaf.id,
                                                1,
                                                Some("msg:System".to_string()),
                                            );
                                        }

                                        let root_id = vault.root.id.clone();
                                        let position = match vault.get_artifact(&root_id) {
                                            Ok(Some(root_art)) => root_art.references.len() as u64,
                                            _ => 0,
                                        };
                                        let contact_label = peer_name.unwrap_or_else(|| "Unknown Contact".to_string());
                                        let _ = vault.compose(
                                            &root_id,
                                            contact_tree_id,
                                            position,
                                            Some(contact_label),
                                        );

                                        // Rebuild sidebar tree so Contact appears immediately
                                        if let Ok(Some(rebuilt_root)) = vault.get_artifact(&vault.root.id) {
                                            let mut nodes = Vec::new();
                                            let root_refs = &rebuilt_root.references;
                                            let mut quest_section_added = false;
                                            let mut exchange_section_added = false;
                                            let mut tokens_section_added = false;
                                            let mut contacts_section_added = false;

                                            for aref in root_refs {
                                                if let Ok(Some(art)) = vault.get_artifact(&aref.artifact_id) {
                                                    let view_type_str = NavigationState::view_type_for(&art.artifact_type);
                                                    let icon = NavigationState::icon_for_type(&art.artifact_type);
                                                    let heat_val = vault.heat(&aref.artifact_id, now).unwrap_or(0.0);
                                                    let heat_lvl = indras_ui::heat_level(heat_val);
                                                    let label_str = aref.label.clone().unwrap_or_default();

                                                    let is_quest_type = matches!(
                                                        art.artifact_type.as_str(),
                                                        "quest" | "need" | "offering" | "intention"
                                                    );
                                                    let is_contact = art.artifact_type == "contact";
                                                    let section = if nodes.is_empty() {
                                                        Some("Vault".to_string())
                                                    } else if is_contact && !contacts_section_added {
                                                        contacts_section_added = true;
                                                        Some("Contacts".to_string())
                                                    } else if is_quest_type && !quest_section_added {
                                                        quest_section_added = true;
                                                        Some("Intentions & Quests".to_string())
                                                    } else if art.artifact_type == "exchange" && !exchange_section_added {
                                                        exchange_section_added = true;
                                                        Some("Exchanges".to_string())
                                                    } else if art.artifact_type == "collection" && !tokens_section_added {
                                                        tokens_section_added = true;
                                                        Some("Tokens".to_string())
                                                    } else {
                                                        None
                                                    };

                                                    let node_id = format!("{:?}", aref.artifact_id);
                                                    let has_children = !art.references.is_empty();

                                                    nodes.push(VaultTreeNode {
                                                        id: node_id.clone(),
                                                        artifact_id: Some(aref.artifact_id.clone()),
                                                        label: label_str,
                                                        icon: icon.to_string(),
                                                        heat_level: heat_lvl,
                                                        depth: 0,
                                                        has_children,
                                                        expanded: has_children,
                                                        section,
                                                        view_type: view_type_str.to_string(),
                                                    });

                                                    if has_children {
                                                        for child_ref in &art.references {
                                                            if let Ok(Some(child_artifact)) = vault.get_artifact(&child_ref.artifact_id) {
                                                                let child_vt = NavigationState::view_type_for(&child_artifact.artifact_type);
                                                                let child_icon = NavigationState::icon_for_type(&child_artifact.artifact_type);
                                                                let child_heat = vault.heat(&child_ref.artifact_id, now).unwrap_or(0.0);
                                                                let child_heat_lvl = indras_ui::heat_level(child_heat);
                                                                let child_label = child_ref.label.clone().unwrap_or_default();
                                                                let child_node_id = format!("{:?}", child_ref.artifact_id);

                                                                nodes.push(VaultTreeNode {
                                                                    id: child_node_id,
                                                                    artifact_id: Some(child_ref.artifact_id.clone()),
                                                                    label: child_label,
                                                                    icon: child_icon.to_string(),
                                                                    heat_level: child_heat_lvl,
                                                                    depth: 1,
                                                                    has_children: false,
                                                                    expanded: false,
                                                                    section: None,
                                                                    view_type: child_vt.to_string(),
                                                                });
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            workspace.write().nav.vault_tree = nodes;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to create contact artifact: {}", e);
                                    }
                                }
                            }

                            // Connect via network layer
                            let Some(net) = net else {
                                tracing::error!("on_connect: network is None");
                                contact_invite_status.set(Some("error:Network not ready".into()));
                                return;
                            };

                            tracing::info!("on_connect: connecting to {}...", peer_display);
                            match net.connect_by_code(&uri).await {
                                Ok(realm) => {
                                    tracing::info!("on_connect: connection established with {}", peer_display);
                                    if let Some(key) = contact_node_key.as_ref() {
                                        realm_map.write().insert(key.clone(), realm);
                                    }
                                    log_event(&mut workspace.write(), EventDirection::Sent, format!("Connected to {}", peer_display));
                                    contact_invite_status.set(Some("success:Connected!".into()));
                                    // Clear input and close overlay on success
                                    contact_invite_input.set(String::new());
                                    contact_parsed_name.set(None);
                                    // Close overlay after a brief moment so user sees success
                                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                                    contact_invite_open.set(false);
                                    contact_invite_status.set(None);
                                }
                                Err(e) => {
                                    let err_str = e.to_string();
                                    tracing::error!(error = %err_str, "on_connect: connect failed");
                                    log_event(&mut workspace.write(), EventDirection::System, format!("ERROR: Connection failed: {}", err_str));
                                    contact_invite_status.set(Some(format!("error:Connection failed: {}", err_str)));
                                    // Close overlay after showing error briefly
                                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                    contact_invite_open.set(false);
                                    contact_invite_status.set(None);
                                }
                            }
                        });
                    },
                    on_parse_input: move |input: String| {
                        let trimmed = input.trim().to_string();
                        if trimmed.is_empty() {
                            contact_parsed_name.set(None);
                            return;
                        }
                        match IdentityCode::parse_uri(&trimmed) {
                            Ok((_code, name)) => contact_parsed_name.set(name),
                            Err(_) => contact_parsed_name.set(None),
                        }
                    },
                    copy_feedback: ci_copied,
                    on_copy: move |_| {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let uri = contact_invite_uri.read().clone();
                            let _ = clipboard.set_text(uri);
                        }
                        contact_copy_feedback.set(true);
                        spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                            contact_copy_feedback.set(false);
                            contact_invite_open.set(false);
                        });
                    },
                }

                // Floating action button (mobile)
                Fab { on_click: on_fab_click }

                // Bottom navigation (mobile)
                BottomNav {
                    active_tab: current_active_tab,
                    on_tab_change: on_tab_change,
                }
            }
        },
    }
}

/// Internal data struct for quest view rendering.
#[derive(Clone, Debug)]
struct QuestViewData {
    kind: QuestKind,
    title: String,
    description: String,
    status: String,
    steward_name: String,
    audience_count: usize,
    proofs: Vec<ProofEntry>,
    posted_ago: String,
}
