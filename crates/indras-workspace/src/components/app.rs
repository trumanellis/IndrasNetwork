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
use crate::state::editor::{EditorState, DocumentMeta};

use indras_artifacts::{Artifact, TreeType, LeafType};
use indras_ui::{
    IdentityRow, PeerStrip,
    VaultSidebar, TreeNode,
    SlashMenu, SlashAction,
    DetailPanel, PropertyRow, AudienceMember, HeatEntry, TrailEvent, ReferenceItem, SyncEntry,
    MarkdownPreviewOverlay, PreviewFile, PreviewViewMode,
    ContactInviteOverlay,
};
use indras_ui::PeerDisplayInfo as UiPeerDisplayInfo;
use indras_network::IdentityCode;

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
    let mut contact_invite_uri = use_signal(String::new);
    let mut contact_display_name_sig = use_signal(String::new);
    let mut contact_member_id_short_sig = use_signal(String::new);

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

                            // Log new contacts
                            for entry in &entries {
                                let already_known = workspace.read().peers.entries.iter().any(|p| p.player_id == entry.player_id);
                                if !already_known {
                                    log_event(&mut workspace.write(), EventDirection::Received, format!("Contact confirmed: {}", entry.name));
                                }
                            }

                            workspace.write().peers.entries = entries;
                        }
                    }
                }
            }
        });
    });

    // --- Event handlers ---

    let on_tree_click = {
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
                        spawn(async move {
                            let vault = vh.vault.lock().await;
                            let _now = chrono::Utc::now().timestamp_millis();

                            // Record navigation attention event
                            // (vault is borrowed immutably, skip navigate_to here
                            //  since we need &mut; will be done when we add write support)

                            if let Ok(Some(artifact)) = vault.get_artifact(&artifact_id) {
                                if let Artifact::Tree(tree) = artifact {
                                    let audience_count = tree.audience.len();
                                    let steward_is_self = tree.steward == vh.player_id;
                                    let steward_name = if steward_is_self {
                                        vh.player_name.clone()
                                    } else {
                                        // Look up peer name
                                        vault.peers().iter()
                                            .find(|p| p.peer_id == tree.steward)
                                            .and_then(|p| p.display_name.clone())
                                            .unwrap_or_else(|| "Unknown".to_string())
                                    };

                                    match vt {
                                        ViewType::Settings => {}
                                        ViewType::Document => {
                                            // Load blocks from tree references
                                            let mut blocks = Vec::new();
                                            for child_ref in &tree.references {
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
                                            for (i, child_ref) in tree.references.iter().enumerate() {
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
                                                });
                                            }
                                            story_messages.set(msgs);

                                            // Also set editor meta for topbar steward display
                                            workspace.write().editor.meta.steward_name = steward_name;
                                            workspace.write().editor.meta.audience_count = audience_count;
                                            workspace.write().editor.title = label.clone();
                                        }
                                        ViewType::Quest => {
                                            let kind = match tree.artifact_type {
                                                TreeType::Need => QuestKind::Need,
                                                TreeType::Offering => QuestKind::Offering,
                                                TreeType::Intention => QuestKind::Intention,
                                                _ => QuestKind::Quest,
                                            };

                                            // Build description from first leaf if any
                                            let description = if let Some(first_ref) = tree.references.first() {
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
                        });
                    }
                }
            }
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
            // Clear active state
            for node in &mut workspace.write().nav.vault_tree {
                node.active = false;
            }
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
                            SlashAction::Document => TreeType::Document,
                            SlashAction::Story => TreeType::Story,
                            SlashAction::Quest => TreeType::Quest,
                            SlashAction::Need => TreeType::Need,
                            SlashAction::Offering => TreeType::Offering,
                            SlashAction::Intention => TreeType::Intention,
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
                            Ok(Some(Artifact::Tree(t))) => t,
                            _ => return,
                        };
                        let position = root_for_pos.references.len() as u64;
                        drop(root_for_pos);
                        if let Err(e) = vault.compose(&root_id, tree.id.clone(), position, Some(label.to_string())) {
                            tracing::error!("Failed to add tree to root: {}", e);
                            return;
                        }

                        // Rebuild sidebar tree (read from store, not stale root field)
                        let mut nodes = Vec::new();
                        let rebuilt_root = match vault.get_artifact(&vault.root.id) {
                            Ok(Some(Artifact::Tree(t))) => t,
                            _ => return,
                        };
                        let root_refs = &rebuilt_root.references;
                        let mut quest_section_added = false;
                        let mut exchange_section_added = false;
                        let mut tokens_section_added = false;

                        for aref in root_refs {
                            if let Ok(Some(artifact)) = vault.get_artifact(&aref.artifact_id) {
                                if let Artifact::Tree(tree) = &artifact {
                                    let view_type_str = NavigationState::view_type_for_tree(&tree.artifact_type);
                                    let icon = NavigationState::icon_for_tree_type(&tree.artifact_type);
                                    let heat_val = vault.heat(&aref.artifact_id, now).unwrap_or(0.0);
                                    let heat_lvl = indras_ui::heat_level(heat_val);
                                    let label_str = aref.label.clone().unwrap_or_default();

                                    let is_quest_type = matches!(
                                        tree.artifact_type,
                                        TreeType::Quest | TreeType::Need | TreeType::Offering | TreeType::Intention
                                    );
                                    let section = if nodes.is_empty() {
                                        Some("Vault".to_string())
                                    } else if is_quest_type && !quest_section_added {
                                        quest_section_added = true;
                                        Some("Intentions & Quests".to_string())
                                    } else if tree.artifact_type == TreeType::Exchange && !exchange_section_added {
                                        exchange_section_added = true;
                                        Some("Exchanges".to_string())
                                    } else if tree.artifact_type == TreeType::Collection && !tokens_section_added {
                                        tokens_section_added = true;
                                        Some("Tokens".to_string())
                                    } else {
                                        None
                                    };

                                    let node_id = format!("{:?}", aref.artifact_id);
                                    let has_children = !tree.references.is_empty();

                                    nodes.push(VaultTreeNode {
                                        id: node_id.clone(),
                                        artifact_id: Some(aref.artifact_id.clone()),
                                        label: label_str.clone(),
                                        icon: icon.to_string(),
                                        heat_level: heat_lvl,
                                        depth: 0,
                                        has_children,
                                        expanded: has_children,
                                        active: false,
                                        section,
                                        view_type: view_type_str.to_string(),
                                    });

                                    if has_children {
                                        for child_ref in &tree.references {
                                            if let Ok(Some(child_artifact)) = vault.get_artifact(&child_ref.artifact_id) {
                                                if let Artifact::Tree(child_tree) = &child_artifact {
                                                    let child_vt = NavigationState::view_type_for_tree(&child_tree.artifact_type);
                                                    let child_icon = NavigationState::icon_for_tree_type(&child_tree.artifact_type);
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
                                                        active: false,
                                                        section: None,
                                                        view_type: child_vt.to_string(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        ws_signal.write().nav.vault_tree = nodes;

                        // Navigate to the new tree
                        let new_node_id = format!("{:?}", tree.id);
                        ws_signal.write().nav.navigate_to(new_node_id.clone(), label.to_string());

                        // Mark as active
                        for node in &mut ws_signal.write().nav.vault_tree {
                            node.active = node.id == new_node_id;
                        }

                        // Log artifact creation
                        log_event(&mut ws_signal.write(), EventDirection::System, format!("Created: {}", label));
                    }

                    // Leaf block actions - add to currently active tree
                    SlashAction::Text | SlashAction::Heading | SlashAction::Code |
                    SlashAction::Callout | SlashAction::Todo | SlashAction::Image | SlashAction::Divider => {
                        // Find the active tree
                        let active_tree_id = {
                            let tree_nodes = ws_signal.read().nav.vault_tree.clone();
                            tree_nodes.iter()
                                .find(|n| n.active)
                                .and_then(|n| n.artifact_id.clone())
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
                        let leaf = match vault.place_leaf(content.as_bytes(), LeafType::Message, now) {
                            Ok(l) => l,
                            Err(e) => {
                                tracing::error!("Failed to create leaf: {}", e);
                                return;
                            }
                        };

                        // Get position in the active tree
                        let position = if let Ok(Some(artifact)) = vault.get_artifact(&active_tree_id) {
                            if let Artifact::Tree(tree) = artifact {
                                tree.references.len() as u64
                            } else {
                                0
                            }
                        } else {
                            0
                        };

                        // Add to active tree
                        if let Err(e) = vault.compose(&active_tree_id, leaf.id.clone(), position, Some(label_str.to_string())) {
                            tracing::error!("Failed to add leaf to tree: {}", e);
                            return;
                        }

                        // Reload the document's blocks
                        if let Ok(Some(artifact)) = vault.get_artifact(&active_tree_id) {
                            if let Artifact::Tree(tree) = artifact {
                                let mut blocks = Vec::new();
                                for child_ref in &tree.references {
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

    // Convert VaultTreeNode -> indras_ui::TreeNode for sidebar
    let sidebar_nodes: Vec<TreeNode> = ws.nav.vault_tree.iter().map(|n| {
        TreeNode {
            id: n.id.clone(),
            label: n.label.clone(),
            icon: n.icon.clone(),
            heat_level: n.heat_level,
            depth: n.depth,
            has_children: n.has_children,
            expanded: n.expanded,
            active: n.active,
            section: n.section.clone(),
            view_type: n.view_type.clone(),
        }
    }).collect();

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
        let peer_ids = ["indra1m4kp...a7n2", "indra1j9wx...d5e8"];
        let peer_roles = ["syncing", "peer"];
        for (i, peer) in ws.peers.entries.iter().enumerate() {
            members.push(AudienceMember {
                name: peer.name.clone(),
                letter: peer.letter.clone(),
                color_class: peer.color_class.clone(),
                role: peer_roles.get(i).unwrap_or(&"peer").to_string(),
                short_id: peer_ids.get(i).unwrap_or(&"").to_string(),
            });
        }
        members
    };

    let references = vec![
        ReferenceItem { icon: "\u{1F4C4}".into(), name: "The new ontology reduces...".into(), ref_type: "msg".into() },
        ReferenceItem { icon: "\u{1F4BB}".into(), name: "pub enum Artifact { ... }".into(), ref_type: "code".into() },
        ReferenceItem { icon: "\u{1F5FA}".into(), name: "system_diagram.png".into(), ref_type: "image".into() },
    ];
    let refs_count = 14usize;

    let sync_entries = vec![
        SyncEntry { name: "Nova (local)".into(), status: "synced".into(), status_text: "up to date".into() },
        SyncEntry { name: "Sage".into(), status: "syncing".into(), status_text: "syncing 3 refs".into() },
        SyncEntry { name: "Zephyr".into(), status: "offline".into(), status_text: "last seen 4m ago".into() },
    ];

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
                                    }
                                }
                            }
                        }
                        ViewType::Story => {
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
                                        preview_file.set(Some(PreviewFile {
                                            name: aref.name.clone(),
                                            content: format!("# {}\n\nType: {}", aref.name, aref.artifact_type),
                                            raw_content: format!("# {}\n\nType: {}", aref.name, aref.artifact_type),
                                            mime_type: "text/markdown".to_string(),
                                            data_url: None,
                                        }));
                                        preview_view_mode.set(PreviewViewMode::Rendered);
                                        preview_open.set(true);
                                    },
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
                        artifact_id_display: "Doc(7f2a...e891)".to_string(),
                        refs_count: refs_count,
                        steward_name: player_name.clone(),
                        steward_letter: player_letter.clone(),
                        is_own_steward: true,
                        sync_entries: sync_entries,
                        combined_heat: 0.56,
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
                                match vault.place_tree(TreeType::Custom("Contact".to_string()), audience, now) {
                                    Ok(contact_tree) => {
                                        let contact_payload = serde_json::json!({
                                            "identity_code": uri,
                                            "display_name": peer_name,
                                        }).to_string();

                                        if let Ok(leaf) = vault.place_leaf(
                                            contact_payload.as_bytes(),
                                            LeafType::Custom("ContactCard".to_string()),
                                            now,
                                        ) {
                                            let label = peer_name.clone().unwrap_or_else(|| "Unknown".to_string());
                                            let _ = vault.compose(
                                                &contact_tree.id,
                                                leaf.id,
                                                0,
                                                Some(label),
                                            );
                                        }

                                        let root_id = vault.root.id.clone();
                                        let position = match vault.get_artifact(&root_id) {
                                            Ok(Some(Artifact::Tree(t))) => t.references.len() as u64,
                                            _ => 0,
                                        };
                                        let contact_label = peer_name.unwrap_or_else(|| "Unknown Contact".to_string());
                                        let _ = vault.compose(
                                            &root_id,
                                            contact_tree.id,
                                            position,
                                            Some(contact_label),
                                        );
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
                                Ok(_realm) => {
                                    tracing::info!("on_connect: connection established with {}", peer_display);
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
