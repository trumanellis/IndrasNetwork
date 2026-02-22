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
use crate::components::quest::{QuestView, QuestKind, ProofEntry, AttentionItem, PledgedToken, AttentionPeerSummary, StewardshipChainEntry, format_duration_secs, PeerOption, IntentionCreateOverlay};
use crate::components::settings::SettingsView;
use crate::components::setup::SetupView;
use crate::components::pass_story::PassStoryOverlay;
use crate::components::event_log::EventLogView;
use crate::components::artifact_browser::{ArtifactBrowserView, BrowsableArtifact, GrantDisplay, MimeCategory};
use crate::state::workspace::{EventDirection, log_event};
use crate::state::workspace::{WorkspaceState, ViewType, AppPhase, PeerDisplayInfo};
use crate::state::navigation::{NavigationState, VaultTreeNode};
use crate::state::editor::{EditorState, DocumentMeta, BlockDocumentSchema};

use indras_artifacts::Intention;
use indras_ui::{
    IdentityRow, PeerStrip,
    NavigationSidebar, NavDestination, CreateAction, RecentItem,
    SlashMenu, SlashAction,
    DetailPanel, PropertyRow, AudienceMember, HeatEntry, TrailEvent, ReferenceItem, SyncEntry,
    MarkdownPreviewOverlay, PreviewFile, PreviewViewMode,
    ContactInviteOverlay,
    ChatPanel,
};
use indras_ui::PeerDisplayInfo as UiPeerDisplayInfo;
use indras_network::{ArtifactStatus, GeoLocation, HomeArtifactEntry, IdentityCode, IndrasNetwork, HomeRealm, Realm, RealmChatDocument, EditableChatMessage, EditableMessageType, AccessMode};
use indras_ui::artifact_display::{ArtifactDisplayInfo, ArtifactDisplayStatus};

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

/// Rebuild the sidebar vault_tree from the ArtifactIndex (single source of truth).
///
/// Reads all active artifacts from the home realm's index and builds
/// VaultTreeNode entries using the stored `artifact_type` for icons and view types.
/// Also resolves realm aliases for labels and populates `realm_map`.
async fn rebuild_sidebar_from_index(
    home: &HomeRealm,
    network: &IndrasNetwork,
    mut workspace: Signal<WorkspaceState>,
    mut realm_map: Signal<std::collections::HashMap<String, Realm>>,
) {
    let doc = match home.artifact_index().await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to read artifact index for sidebar rebuild");
            return;
        }
    };
    let data = doc.read().await;
    let mut entries: Vec<_> = data.active_artifacts().collect();
    if entries.is_empty() {
        return;
    }

    // Sort by category so section headers group correctly:
    // 0 = vault (file/document/story/realm/gallery/etc), 1 = contact,
    // 2 = quest/need/offering/intention, 3 = exchange, 4 = collection
    fn category_order(t: &str) -> u8 {
        match t {
            "contact" => 1,
            "quest" | "need" | "offering" | "intention" => 2,
            "exchange" => 3,
            "collection" => 4,
            _ => 0,
        }
    }
    entries.sort_by_key(|e| category_order(&e.artifact_type));

    // Build realm lookup: artifact_id -> Realm
    let mut art_to_realm = std::collections::HashMap::new();
    for rid in network.realms() {
        if let Some(realm) = network.get_realm_by_id(&rid) {
            if let Some(art_id) = realm.artifact_id() {
                art_to_realm.insert(*art_id, realm);
            }
        }
    }

    let mut nodes = Vec::new();
    let mut contacts_section_added = false;
    let mut quest_section_added = false;
    let mut exchange_section_added = false;
    let mut tokens_section_added = false;

    for entry in &entries {
        let node_id = format!("{:?}", entry.id);
        let icon = NavigationState::icon_for_type(&entry.artifact_type);
        let view_type_str = NavigationState::view_type_for(&entry.artifact_type);

        let is_contact = entry.artifact_type == "contact";
        let is_quest_type = matches!(
            entry.artifact_type.as_str(),
            "quest" | "need" | "offering" | "intention"
        );
        let section = if nodes.is_empty() {
            Some("Vault".to_string())
        } else if is_contact && !contacts_section_added {
            contacts_section_added = true;
            Some("Contacts".to_string())
        } else if is_quest_type && !quest_section_added {
            quest_section_added = true;
            Some("Intentions & Quests".to_string())
        } else if entry.artifact_type == "exchange" && !exchange_section_added {
            exchange_section_added = true;
            Some("Exchanges".to_string())
        } else if entry.artifact_type == "collection" && !tokens_section_added {
            tokens_section_added = true;
            Some("Tokens".to_string())
        } else {
            None
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
            icon: icon.to_string(),
            heat_level: 0,
            depth: 0,
            has_children: false,
            expanded: false,
            section,
            view_type: view_type_str.to_string(),
        });

        // Insert realm into realm_map if found
        if let Some(realm) = art_to_realm.remove(&entry.id) {
            realm_map.write().insert(node_id, realm);
        }
    }

    if !nodes.is_empty() {
        workspace.write().nav.vault_tree = nodes;
    }
}

/// Root application component.
#[component]
pub fn RootApp() -> Element {
    let mut workspace = use_signal(WorkspaceState::new);
    let mut vault_handle = use_signal(|| None::<VaultHandle>);
    let mut story_messages = use_signal(Vec::<StoryMessage>::new);
    let mut quest_data = use_signal(|| None::<QuestViewData>);
    // token_picker_open removed — inline pickers per-proof now
    let mut preview_open = use_signal(|| false);
    let mut preview_file = use_signal(|| None::<PreviewFile>);
    let mut preview_view_mode = use_signal(|| PreviewViewMode::Rendered);
    let mut network_handle = use_signal(|| None::<NetworkHandle>);
    let mut setup_error = use_signal(|| None::<String>);
    let mut setup_loading = use_signal(|| false);
    let mut pass_story_open = use_signal(|| false);
    let mut intention_create_open = use_signal(|| false);
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

    // Artifact browser state
    let mut browser_artifacts = use_signal(Vec::<BrowsableArtifact>::new);
    let mut browser_search = use_signal(String::new);
    let mut browser_filter = use_signal(|| MimeCategory::All);
    let mut browser_radius = use_signal(|| 100.0_f64);
    let mut user_location = use_signal(|| None::<GeoLocation>);
    let mut peer_filter = use_signal(String::new);

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
                // Extract channel parts from the mutex — take() moves receivers
                // out so each dispatcher task owns its receiver directly,
                // avoiding the deadlock of a shared mutex held across .await.
                if let Ok(mut guard) = channels.try_lock() {
                    lua_event_tx.set(Some(guard.event_tx.clone()));
                    let action_rx = guard.action_rx.take();
                    let event_tx = guard.event_tx.clone();
                    let query_rx = guard.query_rx.take();
                    drop(guard);

                    if let (Some(action_rx), Some(query_rx)) = (action_rx, query_rx) {
                        spawn_dispatcher(
                            action_rx,
                            event_tx,
                            query_rx,
                            workspace,
                            contact_invite_open,
                            contact_invite_input,
                            home_realm_handle,
                            user_location,
                            network_handle,
                        );
                    }
                }
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
    // attention_items now loaded from vault into QuestViewData

    // Phase-based boot: check first-run on mount
    use_effect(move || {
        spawn(async move {
            if is_first_run() {
                // Auto-create identity if INDRAS_NAME is set (e.g., --remock)
                if let Ok(auto_name) = std::env::var("INDRAS_NAME") {
                    match create_identity(&auto_name, None).await {
                        Ok(_) => {
                            tracing::info!("Auto-created identity: {}", auto_name);
                            // Fall through to load_identity below
                        }
                        Err(e) => {
                            tracing::error!("Auto-create identity failed: {}", e);
                            workspace.write().phase = AppPhase::Setup;
                            return;
                        }
                    }
                } else {
                    workspace.write().phase = AppPhase::Setup;
                    return;
                }
            }
            {
                // Load identity (works for both returning user and auto-created)
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
                                        rebuild_sidebar_from_index(&hr_clone, &net, workspace, realm_map).await;
                                        let restored_count = workspace.read().nav.vault_tree.len();
                                        if restored_count > 0 {
                                            log_event(&mut workspace.write(), EventDirection::System, format!("Restored {} artifacts from home realm", restored_count));
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "Failed to initialize home realm (non-fatal)");
                                        log_event(&mut workspace.write(), EventDirection::System, format!("Home realm warning: {}", e));
                                    }
                                }

                                // Emit AppReady AFTER network + home realm are initialized,
                                // so Lua scripts can immediately query/store artifacts.
                                #[cfg(feature = "lua-scripting")]
                                if let Some(ref tx) = *lua_event_tx.read() {
                                    let _ = tx.send(AppEvent::AppReady);
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
            let mut processed_invites = std::collections::HashSet::<String>::new();
            let mut dm_realms: std::collections::HashMap<[u8; 32], indras_network::Realm> = std::collections::HashMap::new();
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

                                        // Store contact in ArtifactIndex and rebuild sidebar
                                        if sidebar_needs_rebuild {
                                            // Get the new contact's ArtifactId from vault root's last reference
                                            let contact_art_id = vault.get_artifact(&vault.root.id)
                                                .ok().flatten()
                                                .and_then(|root| root.references.last().map(|r| r.artifact_id));
                                            drop(vault);

                                            if let Some(contact_art_id) = contact_art_id {
                                                if let Some(home) = home_realm_handle.read().as_ref().cloned() {
                                                    if let Ok(doc) = home.artifact_index().await {
                                                        let index_entry = HomeArtifactEntry {
                                                            id: contact_art_id,
                                                            name: entry.name.clone(),
                                                            artifact_type: "contact".to_string(),
                                                            mime_type: None,
                                                            size: 0,
                                                            created_at: now,
                                                            encrypted_key: None,
                                                            status: ArtifactStatus::Active,
                                                            grants: vec![],
                                                            provenance: None,
                                                            location: None,
                                                            content: None,
                                                        };
                                                        let _ = doc.update(|index| { index.store(index_entry); }).await;
                                                    }
                                                    let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                                    if let Some(net) = net {
                                                        rebuild_sidebar_from_index(&home, &net, workspace, realm_map).await;
                                                    }
                                                }
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

                // Check DM chats for incoming realm invites (every ~10s)
                if tick % 5 == 2 {
                    let peers_snapshot = workspace.read().peers.entries.clone();
                    let my_name = vault_handle.read().as_ref().map(|vh| vh.player_name.clone()).unwrap_or_default();
                    let mut any_joined = false;
                    for peer_entry in &peers_snapshot {
                        // Reuse persistent DM realm so Document listener stays alive across polls
                        if !dm_realms.contains_key(&peer_entry.player_id) {
                            if let Ok(r) = net.connect(peer_entry.player_id).await {
                                dm_realms.insert(peer_entry.player_id, r);
                            }
                        }
                        if let Some(dm_realm) = dm_realms.get(&peer_entry.player_id) {
                            if let Ok(chat_doc) = dm_realm.chat_doc().await {
                                let _ = chat_doc.refresh().await;
                                let data = chat_doc.read().await;
                                for msg in data.visible_messages() {
                                    if msg.author == my_name { continue; }
                                    if let EditableMessageType::RealmInvite {
                                        ref invite_code, ref name, ref description, ..
                                    } = msg.message_type {
                                        // Skip already-processed invites
                                        if processed_invites.contains(&msg.id) { continue; }
                                        processed_invites.insert(msg.id.clone());

                                        // Join the realm
                                        if let Ok(shared_realm) = net.join(invite_code).await {
                                            if let Some(vh) = vault_handle.read().clone() {
                                                let mut vault = vh.vault.lock().await;
                                                let join_now = chrono::Utc::now().timestamp_millis();
                                                let audience = vec![vh.player_id, peer_entry.player_id];

                                                // Use Intention::create to get description leaf
                                                if let Ok(intention) = Intention::create(&mut vault, description, audience, join_now) {
                                                    let root_id = vault.root.id.clone();
                                                    let pos = vault.get_artifact(&root_id)
                                                        .ok().flatten()
                                                        .map(|a| a.references.len() as u64)
                                                        .unwrap_or(0);
                                                    let _ = vault.compose(&root_id, intention.id, pos, Some(name.clone()));

                                                    let node_id = format!("{:?}", intention.id);
                                                    drop(vault);
                                                    realm_map.write().insert(node_id, shared_realm);
                                                    any_joined = true;

                                                    log_event(&mut workspace.write(), EventDirection::Received,
                                                        format!("Joined shared Intention: {}", name));

                                                    // Store in ArtifactIndex
                                                    if let Some(home) = home_realm_handle.read().as_ref().cloned() {
                                                        if let Ok(doc) = home.artifact_index().await {
                                                            let entry = HomeArtifactEntry {
                                                                id: intention.id,
                                                                name: name.clone(),
                                                                artifact_type: "intention".to_string(),
                                                                mime_type: None,
                                                                size: 0,
                                                                created_at: join_now,
                                                                encrypted_key: None,
                                                                status: ArtifactStatus::Active,
                                                                grants: vec![],
                                                                provenance: None,
                                                                location: None,
                                                                content: None,
                                                            };
                                                            let _ = doc.update(|index| { index.store(entry); }).await;
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

                    // Rebuild sidebar once if any new intentions were joined
                    if any_joined {
                        if let Some(home) = home_realm_handle.read().as_ref().cloned() {
                            rebuild_sidebar_from_index(&home, &net, workspace, realm_map).await;
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
                                        ViewType::Settings | ViewType::Artifacts => {}
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

                                            // Build description from first ref with "description" label
                                            let description = {
                                                let desc_ref = artifact.references.iter()
                                                    .find(|r| r.label.as_deref() == Some("description"));
                                                if let Some(dr) = desc_ref {
                                                    vault.get_payload(&dr.artifact_id)
                                                        .ok()
                                                        .flatten()
                                                        .map(|p| String::from_utf8_lossy(&p).to_string())
                                                        .unwrap_or_else(|| "No description yet.".to_string())
                                                } else {
                                                    "No description yet.".to_string()
                                                }
                                            };

                                            let steward_for_meta = steward_name.clone();
                                            let intention = Intention::from_id(artifact.id);
                                            let player_name_for_peers = vh.player_name.clone();

                                            // Load proofs
                                            let proofs = {
                                                let proof_refs = intention.proofs(&vault).unwrap_or_default();
                                                let mut proof_entries = Vec::new();
                                                for proof_ref in &proof_refs {
                                                    let (author_name, author_letter, author_color) = if let Some(label) = &proof_ref.label {
                                                        let parts: Vec<&str> = label.splitn(3, ':').collect();
                                                        if parts.len() >= 2 {
                                                            let hex = parts[1];
                                                            if hex.starts_with("02") {
                                                                ("Sage".into(), "S".into(), "peer-dot-sage".into())
                                                            } else if hex.starts_with("03") {
                                                                ("Zephyr".into(), "Z".into(), "peer-dot-zeph".into())
                                                            } else {
                                                                (player_name_for_peers.clone(), player_name_for_peers.chars().next().unwrap_or('N').to_string(), "peer-dot-self".into())
                                                            }
                                                        } else {
                                                            ("Unknown".into(), "?".into(), String::new())
                                                        }
                                                    } else {
                                                        ("Unknown".into(), "?".into(), String::new())
                                                    };

                                                    let body = vault.get_payload(&proof_ref.artifact_id)
                                                        .ok()
                                                        .flatten()
                                                        .map(|p| String::from_utf8_lossy(&p).to_string())
                                                        .unwrap_or_else(|| "Proof submitted".to_string());

                                                    proof_entries.push(ProofEntry {
                                                        author_name,
                                                        author_letter,
                                                        author_color_class: author_color,
                                                        body,
                                                        time_ago: "recently".into(),
                                                        artifact_attachments: Vec::new(),
                                                        tokens: Vec::new(),
                                                        has_tokens: false,
                                                        total_token_count: 0,
                                                        total_token_duration: String::new(),
                                                    });
                                                }
                                                proof_entries
                                            };

                                            // Status
                                            let status_str = {
                                                match intention.status(&vault) {
                                                    Ok(Some(s)) if s == "fulfilled" => "Fulfilled",
                                                    _ => if proofs.is_empty() { "Open" } else { "Proven" },
                                                }
                                            };

                                            // Heat
                                            let now_ms = chrono::Utc::now().timestamp_millis();
                                            let heat = vault.heat(&artifact.id, now_ms).unwrap_or(0.0);

                                            // Attention per peer
                                            let attention_peers = {
                                                let mut peers_summary = Vec::new();
                                                let audience_ids: Vec<indras_artifacts::PlayerId> = artifact.grants.iter().map(|g| g.grantee).collect();
                                                let mut max_secs = 0u64;

                                                // First pass: compute totals
                                                let mut peer_data: Vec<(indras_artifacts::PlayerId, Vec<indras_artifacts::DwellWindow>)> = Vec::new();
                                                for &pid in &audience_ids {
                                                    let windows = intention.unreleased_attention(&vault, pid).unwrap_or_default();
                                                    if !windows.is_empty() {
                                                        let total_ms: u64 = windows.iter().map(|w| w.duration_ms).sum();
                                                        let total_secs = total_ms / 1000;
                                                        if total_secs > max_secs { max_secs = total_secs; }
                                                        peer_data.push((pid, windows));
                                                    }
                                                }

                                                // Second pass: build summaries with bar fractions
                                                for (pid, windows) in &peer_data {
                                                    let total_ms: u64 = windows.iter().map(|w| w.duration_ms).sum();
                                                    let total_secs = total_ms / 1000;
                                                    let (name, letter, color) = peer_display_info(*pid, &player_name_for_peers);
                                                    peers_summary.push(AttentionPeerSummary {
                                                        peer_name: name,
                                                        peer_letter: letter,
                                                        peer_color_class: color,
                                                        total_duration: format_duration_secs(total_secs),
                                                        total_duration_secs: total_secs,
                                                        window_count: windows.len(),
                                                        bar_fraction: if max_secs > 0 { total_secs as f32 / max_secs as f32 } else { 0.0 },
                                                    });
                                                }
                                                peers_summary
                                            };

                                            let total_attention_duration = {
                                                let total: u64 = attention_peers.iter().map(|p| p.total_duration_secs).sum();
                                                format_duration_secs(total)
                                            };

                                            // Attention items for local player (for inline picker)
                                            let attention_items = {
                                                let windows = intention.unreleased_attention(&vault, vh.player_id).unwrap_or_default();
                                                windows.iter().map(|w| {
                                                    AttentionItem {
                                                        target: "This Intention".into(),
                                                        when: format!("{}ms ago", w.start_timestamp),
                                                        duration: format_duration_secs(w.duration_ms / 1000),
                                                    }
                                                }).collect::<Vec<_>>()
                                            };

                                            // Pledged tokens
                                            let pledged_tokens = {
                                                let pledge_refs = intention.pledged_tokens(&vault).unwrap_or_default();
                                                let mut pts = Vec::new();
                                                for pref in &pledge_refs {
                                                    let duration = vault.get_payload(&pref.artifact_id)
                                                        .ok()
                                                        .flatten()
                                                        .and_then(|p| {
                                                            if p.len() >= 8 {
                                                                Some(u64::from_le_bytes(p[..8].try_into().unwrap_or([0u8; 8])))
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                        .unwrap_or(0);
                                                    let from_name = if let Some(label) = &pref.label {
                                                        let parts: Vec<&str> = label.splitn(3, ':').collect();
                                                        if parts.len() >= 2 {
                                                            let hex = parts[1];
                                                            if hex.starts_with("02") { "Sage".into() }
                                                            else if hex.starts_with("03") { "Zephyr".into() }
                                                            else { player_name_for_peers.clone() }
                                                        } else {
                                                            "Unknown".into()
                                                        }
                                                    } else {
                                                        "Unknown".into()
                                                    };
                                                    pts.push(PledgedToken {
                                                        token_label: format!("Token"),
                                                        duration: format_duration_secs(duration / 1000),
                                                        from_name,
                                                    });
                                                }
                                                pts
                                            };

                                            // Stewardship chain
                                            let stewardship_chain = {
                                                let mut chain = Vec::new();
                                                // From proof refs — each proof represents a "created" link
                                                let proof_refs = intention.proofs(&vault).unwrap_or_default();
                                                for pref in &proof_refs {
                                                    // Get tokens assigned to this proof via stewardship transfers
                                                    if let Ok(Some(artifact)) = vault.get_artifact(&pref.artifact_id) {
                                                        let (from_name, from_letter, from_color) = peer_display_info(artifact.steward, &player_name_for_peers);
                                                        // Check blessing history on any tokens
                                                        for blessing in &artifact.blessing_history {
                                                            let (bn, bl, bc) = peer_display_info(blessing.from, &player_name_for_peers);
                                                            chain.push(StewardshipChainEntry {
                                                                from_name: bn,
                                                                from_letter: bl,
                                                                from_color_class: bc,
                                                                action: "blessed".into(),
                                                                token_label: "Token".into(),
                                                                token_duration: String::new(),
                                                                to_name: from_name.clone(),
                                                                to_letter: from_letter.clone(),
                                                                to_color_class: from_color.clone(),
                                                            });
                                                        }
                                                    }
                                                }
                                                // From steward_history on any known tokens
                                                let pledge_refs = intention.pledged_tokens(&vault).unwrap_or_default();
                                                for pref in &pledge_refs {
                                                    if let Ok(history) = vault.steward_history(&pref.artifact_id) {
                                                        for record in &history {
                                                            let (fn_, fl, fc) = peer_display_info(record.from, &player_name_for_peers);
                                                            let (tn, tl, tc) = peer_display_info(record.to, &player_name_for_peers);
                                                            chain.push(StewardshipChainEntry {
                                                                from_name: fn_,
                                                                from_letter: fl,
                                                                from_color_class: fc,
                                                                action: "released".into(),
                                                                token_label: "Token".into(),
                                                                token_duration: String::new(),
                                                                to_name: tn,
                                                                to_letter: tl,
                                                                to_color_class: tc,
                                                            });
                                                        }
                                                    }
                                                }
                                                chain
                                            };

                                            quest_data.set(Some(QuestViewData {
                                                kind,
                                                title: label.clone(),
                                                description,
                                                status: status_str.to_string(),
                                                steward_name,
                                                audience_count,
                                                proofs,
                                                posted_ago: String::new(),
                                                heat,
                                                attention_peers,
                                                total_attention_duration,
                                                pledged_tokens,
                                                stewardship_chain,
                                                attention_items,
                                                intention_id: artifact.id,
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

    let mut on_slash_select = {
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
                    // Intention opens a creation form modal instead of creating directly
                    SlashAction::Intention => {
                        drop(vault); // Release lock — modal handles creation
                        intention_create_open.set(true);
                        return;
                    }
                    // Tree actions - create new tree and add to root
                    SlashAction::Document | SlashAction::Story | SlashAction::Quest |
                    SlashAction::Need | SlashAction::Offering => {
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
                        // Store in ArtifactIndex so artifact browser sees it
                        if let Some(home) = home_realm_handle.read().as_ref().cloned() {
                            if let Ok(doc) = home.artifact_index().await {
                                let entry = HomeArtifactEntry {
                                    id: tree.id,
                                    name: label.to_string(),
                                    artifact_type: tree_type.to_string(),
                                    mime_type: None,
                                    size: 0,
                                    created_at: now,
                                    encrypted_key: None,
                                    status: ArtifactStatus::Active,
                                    grants: vec![],
                                    provenance: None,
                                    location: None,
                                    content: None,
                                };
                                let _ = doc.update(|index| { index.store(entry); }).await;
                            }
                        }

                        // Rebuild sidebar from ArtifactIndex
                        if let Some(home) = home_realm_handle.read().as_ref().cloned() {
                            let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                            if let Some(net) = net {
                                rebuild_sidebar_from_index(&home, &net, ws_signal, realm_map).await;
                            }
                        }

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

    let on_settings = move |_: ()| {
        workspace.write().ui.active_view = ViewType::Settings;
    };

    // Navigation hub: map NavDestination → ViewType (with async artifact loading for Artifacts)
    let on_navigate = move |dest: NavDestination| {
        match dest {
            NavDestination::Home => {
                workspace.write().ui.active_view = ViewType::Document;
            }
            NavDestination::Artifacts => {
                workspace.write().ui.active_view = ViewType::Artifacts;
                let hr = home_realm_handle;
                let ul = user_location;
                spawn(async move {
                    let hr_read = hr.read();
                    if let Some(ref home) = *hr_read {
                        if let Ok(index) = home.artifact_index().await {
                            let data = index.read().await;
                            let loc = ul.read().clone();
                            let peers_state = workspace.read().peers.entries.clone();
                            let browsable: Vec<BrowsableArtifact> = data
                                .active_artifacts()
                                .map(|entry| {
                                    let distance_km = match (&loc, &entry.location) {
                                        (Some(ul), Some(al)) => Some(ul.distance_km(al)),
                                        _ => None,
                                    };
                                    let status = if entry.status.is_active() {
                                        ArtifactDisplayStatus::Active
                                    } else {
                                        ArtifactDisplayStatus::Recalled
                                    };
                                    let origin_label = match &entry.provenance {
                                        None => "Mine".to_string(),
                                        Some(p) => {
                                            peers_state.iter()
                                                .find(|peer| peer.player_id == p.received_from)
                                                .map(|peer| peer.name.clone())
                                                .unwrap_or_else(|| format!("{:02x}{:02x}..", p.received_from[0], p.received_from[1]))
                                        }
                                    };
                                    let owner_label = match &entry.provenance {
                                        Some(_) => Some(format!("From {}", origin_label)),
                                        None if entry.grants.is_empty() => Some("Private".into()),
                                        None => Some(format!("Shared with {}", entry.grants.len())),
                                    };
                                    let grants: Vec<GrantDisplay> = entry.grants.iter().map(|g| {
                                        let peer = peers_state.iter().find(|p| p.player_id == g.grantee);
                                        GrantDisplay {
                                            peer_name: peer.map(|p| p.name.clone())
                                                .unwrap_or_else(|| format!("{:02x}{:02x}..", g.grantee[0], g.grantee[1])),
                                            peer_letter: peer.map(|p| p.letter.clone())
                                                .unwrap_or_else(|| "?".to_string()),
                                            mode_label: g.mode.label().to_string(),
                                        }
                                    }).collect();
                                    BrowsableArtifact {
                                        info: ArtifactDisplayInfo {
                                            id: entry.hash_hex(),
                                            name: entry.name.clone(),
                                            size: entry.size,
                                            mime_type: entry.mime_type.clone(),
                                            status,
                                            data_url: None,
                                            grant_count: entry.grants.len(),
                                            owner_label,
                                        },
                                        distance_km,
                                        origin_label,
                                        content: entry.content.clone(),
                                        grants,
                                    }
                                })
                                .collect();
                            browser_artifacts.set(browsable);
                        }
                    }
                });
            }
            NavDestination::Contacts => {
                workspace.write().ui.active_view = ViewType::Story;
            }
            NavDestination::Quests => {
                workspace.write().ui.active_view = ViewType::Quest;
            }
            NavDestination::Settings => {
                workspace.write().ui.active_view = ViewType::Settings;
            }
        }
    };

    // Create hub: map CreateAction → SlashAction and delegate to on_slash_select
    let on_create = move |action: CreateAction| {
        let slash = match action {
            CreateAction::Document => SlashAction::Document,
            CreateAction::Story => SlashAction::Story,
            CreateAction::Quest => SlashAction::Quest,
            CreateAction::Need => SlashAction::Need,
            CreateAction::Offering => SlashAction::Offering,
            CreateAction::Intention => SlashAction::Intention,
        };
        on_slash_select(slash);
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

    // Derive active navigation destination from current view
    let active_destination = match &active_view {
        ViewType::Document => NavDestination::Home,
        ViewType::Story => NavDestination::Contacts,
        ViewType::Artifacts => NavDestination::Artifacts,
        ViewType::Quest => NavDestination::Quests,
        ViewType::Settings => NavDestination::Settings,
    };

    // Build recent items from recent_artifact_ids cross-referenced with vault_tree
    let recent_items: Vec<RecentItem> = ws.nav.recent_artifact_ids.iter()
        .filter_map(|id| {
            ws.nav.vault_tree.iter().find(|n| n.id == *id).map(|n| {
                RecentItem {
                    id: n.id.clone(),
                    label: n.label.clone(),
                    icon: n.icon.clone(),
                }
            })
        })
        .collect();

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

                                        // Emit events AFTER network + home realm are initialized
                                        #[cfg(feature = "lua-scripting")]
                                        if let Some(ref tx) = *lua_event_tx.read() {
                                            let _ = tx.send(AppEvent::IdentityCreated(name.clone()));
                                            let _ = tx.send(AppEvent::AppReady);
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

                    NavigationSidebar {
                        active: active_destination,
                        recent_items: recent_items,
                        on_navigate: on_navigate,
                        on_create: on_create,
                        on_recent_click: on_tree_click,
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
                                    user_location: user_location.read().clone(),
                                    on_location_change: move |loc: Option<GeoLocation>| {
                                        user_location.set(loc);
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
                        ViewType::Artifacts => {
                            let current_artifacts = browser_artifacts.read().clone();
                            let current_search = browser_search.read().clone();
                            let current_filter = browser_filter.read().clone();
                            let current_radius = *browser_radius.read();
                            let current_peer_filter = peer_filter.read().clone();
                            // Compute distinct origin labels for peer filter chips
                            let mut peers: Vec<String> = current_artifacts.iter()
                                .map(|a| a.origin_label.clone())
                                .collect::<std::collections::BTreeSet<_>>()
                                .into_iter()
                                .collect();
                            peers.sort();
                            rsx! {
                                ArtifactBrowserView {
                                    artifacts: current_artifacts,
                                    search_query: current_search,
                                    on_search: move |q: String| browser_search.set(q),
                                    active_filter: current_filter,
                                    on_filter: move |f: MimeCategory| browser_filter.set(f),
                                    radius_km: current_radius,
                                    on_radius_change: move |r: f64| browser_radius.set(r),
                                    peer_filter: current_peer_filter,
                                    on_peer_filter: move |p: String| peer_filter.set(p),
                                    available_peers: peers,
                                }
                            }
                        }
                        ViewType::Quest => {
                            if let Some(qd) = current_quest_data {
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
                                        heat: qd.heat,
                                        attention_peers: qd.attention_peers,
                                        total_attention_duration: qd.total_attention_duration,
                                        attention_items: qd.attention_items,
                                        pledged_tokens: qd.pledged_tokens,
                                        stewardship_chain: qd.stewardship_chain,
                                        on_submit_proof: move |body: String| {
                                            spawn(async move {
                                                if let Some(vh) = vault_handle.read().clone() {
                                                    let intention_id = quest_data.read().as_ref().map(|q| q.intention_id);
                                                    if let Some(iid) = intention_id {
                                                        let mut vault = vh.vault.lock().await;
                                                        let intention = Intention::from_id(iid);
                                                        let now = chrono::Utc::now().timestamp_millis();
                                                        match intention.submit_proof(&mut vault, &body, now) {
                                                            Ok(_) => {
                                                                tracing::info!("Proof submitted successfully");
                                                            }
                                                            Err(e) => {
                                                                tracing::error!(error = %e, "Failed to submit proof");
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        },
                                        on_confirm_tokens: move |data: (usize, Vec<usize>)| {
                                            let (_proof_idx, selected_indices) = data;
                                            spawn(async move {
                                                if let Some(vh) = vault_handle.read().clone() {
                                                    let intention_id = quest_data.read().as_ref().map(|q| q.intention_id);
                                                    if let Some(iid) = intention_id {
                                                        let mut vault = vh.vault.lock().await;
                                                        let intention = Intention::from_id(iid);
                                                        let now = chrono::Utc::now().timestamp_millis();
                                                        // Get unreleased windows for local player
                                                        let windows = intention.unreleased_attention(&vault, vh.player_id).unwrap_or_default();
                                                        let selected_windows: Vec<_> = selected_indices.iter()
                                                            .filter_map(|&i| windows.get(i).cloned())
                                                            .collect();
                                                        if !selected_windows.is_empty() {
                                                            // Determine proof submitter from the proof at proof_idx
                                                            let proof_submitter = {
                                                                let proof_refs = intention.proofs(&vault).unwrap_or_default();
                                                                if let Some(pref) = proof_refs.get(_proof_idx) {
                                                                    if let Some(label) = &pref.label {
                                                                        let parts: Vec<&str> = label.splitn(3, ':').collect();
                                                                        if parts.len() >= 2 {
                                                                            let hex = parts[1];
                                                                            if hex.starts_with("02") { [2u8; 32] }
                                                                            else if hex.starts_with("03") { [3u8; 32] }
                                                                            else { [1u8; 32] }
                                                                        } else { [1u8; 32] }
                                                                    } else { [1u8; 32] }
                                                                } else { [1u8; 32] }
                                                            };
                                                            match intention.release_attention(&mut vault, &selected_windows, proof_submitter, now) {
                                                                Ok(tokens) => {
                                                                    tracing::info!(count = tokens.len(), "Released attention as tokens");
                                                                }
                                                                Err(e) => {
                                                                    tracing::error!(error = %e, "Failed to release attention");
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        },
                                        on_release_pledged: move |indices: Vec<usize>| {
                                            spawn(async move {
                                                if let Some(vh) = vault_handle.read().clone() {
                                                    let qd_clone = quest_data.read().clone();
                                                    if let Some(qd) = qd_clone {
                                                        let mut vault = vh.vault.lock().await;
                                                        let intention = Intention::from_id(qd.intention_id);
                                                        let now = chrono::Utc::now().timestamp_millis();
                                                        // Get pledge refs and map indices to token IDs
                                                        let pledge_refs = intention.pledged_tokens(&vault).unwrap_or_default();
                                                        let token_ids: Vec<_> = indices.iter()
                                                            .filter_map(|&i| pledge_refs.get(i).map(|r| r.artifact_id))
                                                            .collect();
                                                        // Determine a proof submitter (use first proof's author)
                                                        let proof_submitter = {
                                                            let proof_refs = intention.proofs(&vault).unwrap_or_default();
                                                            if let Some(pref) = proof_refs.first() {
                                                                if let Some(label) = &pref.label {
                                                                    let parts: Vec<&str> = label.splitn(3, ':').collect();
                                                                    if parts.len() >= 2 {
                                                                        let hex = parts[1];
                                                                        if hex.starts_with("02") { [2u8; 32] }
                                                                        else if hex.starts_with("03") { [3u8; 32] }
                                                                        else { [1u8; 32] }
                                                                    } else { [1u8; 32] }
                                                                } else { [1u8; 32] }
                                                            } else { [1u8; 32] }
                                                        };
                                                        if !token_ids.is_empty() {
                                                            match intention.release_pledged_tokens(&mut vault, &token_ids, proof_submitter, now) {
                                                                Ok(()) => {
                                                                    tracing::info!(count = token_ids.len(), "Released pledged tokens");
                                                                }
                                                                Err(e) => {
                                                                    tracing::error!(error = %e, "Failed to release pledged tokens");
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        },
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

                                        // Store contact in ArtifactIndex and rebuild sidebar
                                        let contact_aid = contact_tree.id;
                                        drop(vault);
                                        if let Some(home) = home_realm_handle.read().as_ref().cloned() {
                                            if let Ok(doc) = home.artifact_index().await {
                                                let entry = HomeArtifactEntry {
                                                    id: contact_aid,
                                                    name: peer_display.clone(),
                                                    artifact_type: "contact".to_string(),
                                                    mime_type: None,
                                                    size: 0,
                                                    created_at: now,
                                                    encrypted_key: None,
                                                    status: ArtifactStatus::Active,
                                                    grants: vec![],
                                                    provenance: None,
                                                    location: None,
                                                    content: None,
                                                };
                                                let _ = doc.update(|index| { index.store(entry); }).await;
                                            }
                                            let net = network_handle.read().as_ref().map(|nh| nh.network.clone());
                                            if let Some(net) = net {
                                                rebuild_sidebar_from_index(&home, &net, workspace, realm_map).await;
                                            }
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

                // Intention creation modal
                {
                    let peer_options: Vec<PeerOption> = workspace.read().peers.entries.iter().map(|p| {
                        PeerOption {
                            player_id: p.player_id,
                            name: p.name.clone(),
                            selected: true,
                        }
                    }).collect();
                    rsx! {
                        IntentionCreateOverlay {
                            visible: *intention_create_open.read(),
                            peers: peer_options,
                            on_close: move |_| intention_create_open.set(false),
                            on_create: move |(title, description, audience): (String, String, Vec<[u8; 32]>)| {
                                intention_create_open.set(false);
                                let vh_signal = vault_handle;
                                let mut ws_signal = workspace;
                                let nh_signal = network_handle;
                                let mut rm_signal = realm_map;
                                let hr_signal = home_realm_handle;
                                spawn(async move {
                                    let vh = match vh_signal.read().clone() {
                                        Some(h) => h,
                                        None => return,
                                    };
                                    let mut vault = vh.vault.lock().await;
                                    let now = chrono::Utc::now().timestamp_millis();

                                    // Always include self in audience
                                    let mut full_audience = audience;
                                    if !full_audience.contains(&vh.player_id) {
                                        full_audience.insert(0, vh.player_id);
                                    }

                                    let audience_for_sharing = full_audience.clone();
                                    let intention = match Intention::create(&mut vault, &description, full_audience, now) {
                                        Ok(i) => i,
                                        Err(e) => {
                                            tracing::error!("Failed to create intention: {}", e);
                                            return;
                                        }
                                    };

                                    // Add to root with title label
                                    let root_id = vault.root.id.clone();
                                    let root_for_pos = match vault.get_artifact(&root_id) {
                                        Ok(Some(a)) => a,
                                        _ => return,
                                    };
                                    let position = root_for_pos.references.len() as u64;
                                    drop(root_for_pos);
                                    if let Err(e) = vault.compose(&root_id, intention.id, position, Some(title.clone())) {
                                        tracing::error!("Failed to add intention to root: {}", e);
                                        return;
                                    }

                                    // Create a network Realm for this intention
                                    let tree_node_id = format!("{:?}", intention.id);
                                    drop(vault);
                                    let net = {
                                        let guard = nh_signal.read();
                                        guard.as_ref().map(|nh| nh.network.clone())
                                    };
                                    if let Some(net) = net {
                                        match net.create_realm(&title).await {
                                            Ok(realm) => {
                                                tracing::info!("Created realm for intention: {}", title);
                                                log_event(&mut ws_signal.write(), EventDirection::System, format!("Intention created: {}", title));

                                                // Extract invite code and artifact_id before moving realm into map
                                                let invite_str = realm.invite_code().map(|ic| ic.to_string());
                                                let realm_artifact_id = realm.artifact_id().cloned();
                                                rm_signal.write().insert(tree_node_id.clone(), realm);

                                                // Share with audience peers: grant access + send DM invite
                                                if let Some(invite_str) = invite_str {
                                                    for &peer_id in &audience_for_sharing {
                                                        if peer_id == vh.player_id { continue; }

                                                        // Grant peer access in HomeRealm artifact_index
                                                        // Use the realm's artifact_id (not intention.id) so reconcile()
                                                        // creates the correct sync interface and gossip topic
                                                        if let Some(ref realm_aid) = realm_artifact_id {
                                                            if let Some(hr) = hr_signal.read().as_ref() {
                                                                let _ = hr.grant_access(
                                                                    realm_aid, peer_id, AccessMode::Permanent,
                                                                ).await;
                                                            }
                                                        }

                                                        // Send realm invite via DM chat
                                                        if let Ok(dm_realm) = net.connect(peer_id).await {
                                                            if let Ok(chat_doc) = dm_realm.chat_doc().await {
                                                                let msg = EditableChatMessage::new(
                                                                    format!("realm-invite-{}", now),
                                                                    format!("{}", dm_realm.id()),
                                                                    vh.player_name.clone(),
                                                                    format!("Shared intention: {}", title),
                                                                    now as u64,
                                                                    EditableMessageType::RealmInvite {
                                                                        invite_code: invite_str.clone(),
                                                                        name: title.clone(),
                                                                        artifact_type: "Intention".to_string(),
                                                                        description: description.clone(),
                                                                    },
                                                                );
                                                                let _ = chat_doc.update(|doc| doc.add_message(msg)).await;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::warn!(error = %e, "Failed to create realm for intention (non-fatal)");
                                            }
                                        }
                                    }

                                    // Store in ArtifactIndex and rebuild sidebar
                                    if let Some(home) = hr_signal.read().as_ref().cloned() {
                                        if let Ok(doc) = home.artifact_index().await {
                                            let entry = HomeArtifactEntry {
                                                id: intention.id,
                                                name: title.clone(),
                                                artifact_type: "intention".to_string(),
                                                mime_type: None,
                                                size: 0,
                                                created_at: now,
                                                encrypted_key: None,
                                                status: ArtifactStatus::Active,
                                                grants: vec![],
                                                provenance: None,
                                                location: None,
                                                content: None,
                                            };
                                            let _ = doc.update(|index| { index.store(entry); }).await;
                                        }
                                        let net = nh_signal.read().as_ref().map(|nh| nh.network.clone());
                                        if let Some(net) = net {
                                            rebuild_sidebar_from_index(&home, &net, ws_signal, rm_signal).await;
                                        }
                                    }
                                });
                            },
                        }
                    }
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
    heat: f32,
    attention_peers: Vec<AttentionPeerSummary>,
    total_attention_duration: String,
    pledged_tokens: Vec<PledgedToken>,
    stewardship_chain: Vec<StewardshipChainEntry>,
    attention_items: Vec<AttentionItem>,
    intention_id: indras_artifacts::ArtifactId,
}

/// Resolve a player ID to display info (name, letter, CSS class).
fn peer_display_info(player_id: indras_artifacts::PlayerId, player_name: &str) -> (String, String, String) {
    if player_id == [1u8; 32] {
        (player_name.to_string(), player_name.chars().next().unwrap_or('N').to_string(), "peer-dot-self".into())
    } else if player_id == [2u8; 32] {
        ("Sage".into(), "S".into(), "peer-dot-sage".into())
    } else if player_id == [3u8; 32] {
        ("Zephyr".into(), "Z".into(), "peer-dot-zeph".into())
    } else {
        let hex: String = player_id.iter().take(4).map(|b| format!("{b:02x}")).collect();
        (hex.clone(), hex.chars().next().unwrap_or('?').to_string(), String::new())
    }
}
