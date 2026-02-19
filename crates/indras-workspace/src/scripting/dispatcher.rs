//! Action dispatcher — polls the ActionBus and dispatches actions in RootApp.
//!
//! Also handles QueryBus replies and EventBus emissions.
//!
//! **Important**: The action and query dispatchers each own their receiver
//! directly (no shared mutex). This avoids a deadlock where one dispatcher
//! holds the mutex while awaiting, blocking the other from processing.

use dioxus::prelude::*;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::action::Action;
use super::event::AppEvent;
use super::query::{Query, QueryResult};
use crate::state::workspace::{WorkspaceState, ViewType, AppPhase, EventDirection, log_event};
use crate::bridge::network_bridge::NetworkHandle;
use indras_network::{
    AccessMode, ArtifactProvenance, ArtifactStatus, GeoLocation,
    HomeArtifactMetadata, HomeRealm, ProvenanceType,
    artifact_index::HomeArtifactEntry,
};

/// Spawn the dispatcher as a Dioxus future.
/// This should be called inside the RootApp component where signals are available.
///
/// The dispatcher:
/// 1. Polls ActionBus for actions from Lua and dispatches them
/// 2. Polls QueryBus for queries from Lua and replies with state snapshots
///
/// Each receiver is owned directly by its task — no shared mutex needed.
pub fn spawn_dispatcher(
    mut action_rx: mpsc::Receiver<Action>,
    event_tx: broadcast::Sender<AppEvent>,
    mut query_rx: mpsc::Receiver<(Query, oneshot::Sender<QueryResult>)>,
    mut workspace: Signal<WorkspaceState>,
    mut contact_invite_open: Signal<bool>,
    mut contact_invite_input: Signal<String>,
    home_realm_handle: Signal<Option<HomeRealm>>,
    mut user_location: Signal<Option<GeoLocation>>,
    network_handle: Signal<Option<NetworkHandle>>,
) {
    // Action dispatcher — owns action_rx directly
    let action_event_tx = event_tx.clone();
    spawn(async move {
        loop {
            let Some(action) = action_rx.recv().await else {
                tracing::info!("ActionBus closed — Lua script finished");
                break;
            };

            tracing::debug!("Dispatching action: {:?}", action);

            match action {
                Action::ClickSidebar(label) => {
                    let tree = workspace.read().nav.vault_tree.clone();
                    if let Some(node) = tree.iter().find(|n| n.label == label) {
                        let node_id = node.id.clone();
                        let node_label = node.label.clone();
                        let view_type_str = node.view_type.clone();

                        workspace.write().nav.navigate_to(node_id.clone(), node_label);

                        let vt = match view_type_str.as_str() {
                            "story" => ViewType::Story,
                            "quest" => ViewType::Quest,
                            _ => ViewType::Document,
                        };
                        workspace.write().ui.active_view = vt;

                        let _ = action_event_tx.send(AppEvent::SidebarItemActive(label));
                        let view_str = match view_type_str.as_str() {
                            "story" => "story",
                            "quest" => "quest",
                            _ => "document",
                        };
                        let _ = action_event_tx.send(AppEvent::ViewChanged(view_str.to_string()));
                    } else {
                        tracing::warn!("ClickSidebar: no node with label '{}'", label);
                        let _ = action_event_tx.send(AppEvent::ActionFailed {
                            action: "click_sidebar".into(),
                            error: format!("No sidebar item with label '{}'", label),
                        });
                    }
                }

                Action::ClickTab(tab) => {
                    let vt = match tab.as_str() {
                        "settings" => ViewType::Settings,
                        "quest" => ViewType::Quest,
                        "story" => ViewType::Story,
                        _ => ViewType::Document,
                    };
                    workspace.write().ui.active_view = vt;
                    let _ = action_event_tx.send(AppEvent::ViewChanged(tab));
                }

                Action::ClickBreadcrumb(idx) => {
                    let breadcrumbs = workspace.read().nav.breadcrumbs.clone();
                    if let Some(crumb) = breadcrumbs.get(idx) {
                        let id = crumb.id.clone();
                        let label = crumb.label.clone();
                        workspace.write().nav.navigate_to(id, label);
                    }
                }

                Action::OpenContacts => {
                    contact_invite_open.set(true);
                    let _ = action_event_tx.send(AppEvent::OverlayOpened("contacts".into()));
                }

                Action::PasteConnectCode(code) => {
                    contact_invite_input.set(code);
                }

                Action::ClickConnect => {
                    tracing::info!("ClickConnect dispatched — connection will be handled by on_connect");
                }

                Action::ClickPeerDot(name) => {
                    let tree = workspace.read().nav.vault_tree.clone();
                    if let Some(node) = tree.iter().find(|n| n.label == name) {
                        let node_id = node.id.clone();
                        let node_label = node.label.clone();
                        workspace.write().nav.navigate_to(node_id, node_label);
                    }
                }

                Action::CloseOverlay => {
                    contact_invite_open.set(false);
                    workspace.write().ui.slash_menu_open = false;
                    let _ = action_event_tx.send(AppEvent::OverlayClosed("any".into()));
                }

                Action::TypeMessage(text) => {
                    log_event(&mut workspace.write(), EventDirection::System, format!("Lua: type_message(\"{}\")", text));
                }

                Action::SendMessage => {
                    log_event(&mut workspace.write(), EventDirection::System, "Lua: send_message()".to_string());
                }

                Action::ClickBlock(idx) => {
                    log_event(&mut workspace.write(), EventDirection::System, format!("Lua: click_block({})", idx));
                }

                Action::TypeInBlock(idx, text) => {
                    let mut ws = workspace.write();
                    if let Some(block) = ws.editor.blocks.get(idx) {
                        let new_block = block.with_content(text);
                        ws.editor.blocks[idx] = new_block;
                    }
                }

                Action::AddBlock(block_type) => {
                    log_event(&mut workspace.write(), EventDirection::System, format!("Lua: add_block(\"{}\")", block_type));
                }

                Action::OpenSlashMenu => {
                    workspace.write().ui.slash_menu_open = true;
                }

                Action::SelectSlashAction(action_name) => {
                    workspace.write().ui.slash_menu_open = false;
                    log_event(&mut workspace.write(), EventDirection::System, format!("Lua: select_slash_action(\"{}\")", action_name));
                }

                Action::SetDisplayName(name) => {
                    log_event(&mut workspace.write(), EventDirection::System, format!("Lua: set_display_name(\"{}\")", name));
                }

                Action::ClickCreateIdentity => {
                    log_event(&mut workspace.write(), EventDirection::System, "Lua: click_create_identity()".to_string());
                }

                Action::ConnectToPeer { uri } => {
                    let net = {
                        let guard = network_handle.read();
                        guard.as_ref().map(|nh| nh.network.clone())
                    };
                    if let Some(net) = net {
                        match net.connect_by_code(&uri).await {
                            Ok(_realm) => {
                                let peer_name = indras_network::IdentityCode::parse_uri(&uri)
                                    .ok()
                                    .and_then(|(_, name)| name)
                                    .unwrap_or_else(|| "peer".to_string());
                                tracing::info!("Lua: connected to {}", peer_name);
                                let _ = action_event_tx.send(AppEvent::PeerConnected(peer_name));
                            }
                            Err(e) => {
                                tracing::error!("Lua: connect_to failed: {}", e);
                                let _ = action_event_tx.send(AppEvent::ActionFailed {
                                    action: "connect_to".into(),
                                    error: e.to_string(),
                                });
                            }
                        }
                    } else {
                        tracing::warn!("Lua: connect_to called but network is None");
                    }
                }

                Action::SetUserLocation { lat, lng } => {
                    user_location.set(Some(GeoLocation { lat, lng }));
                    tracing::info!("User location set to ({}, {})", lat, lng);
                }

                Action::StoreArtifact { name, mime, size, lat, lng, from_peer } => {
                    let home = {
                        let hr = home_realm_handle.read();
                        hr.as_ref().cloned()
                    };
                    if let Some(home) = home {
                        // Deterministic dummy data so both peers get the same BLAKE3 ArtifactId
                        let dummy_data = format!("mock:{}:{}:{}", name, mime, size).into_bytes();

                        // Use real HomeRealm API for content-addressed ArtifactId
                        match home.share_artifact(
                            dummy_data,
                            HomeArtifactMetadata {
                                name: name.clone(),
                                mime_type: Some(mime.clone()),
                                size,
                            },
                        ).await {
                            Ok(id) => {
                                let location = match (lat, lng) {
                                    (Some(lat), Some(lng)) => Some(GeoLocation { lat, lng }),
                                    _ => None,
                                };
                                let provenance = from_peer.as_ref().map(|peer_name| {
                                    let member_id = workspace.read().peers.entries.iter()
                                        .find(|p| p.name == *peer_name)
                                        .map(|p| p.player_id)
                                        .unwrap_or_else(|| {
                                            tracing::warn!("Peer '{}' not in contacts, using fallback ID", peer_name);
                                            let mut id = [0u8; 32];
                                            use std::hash::{Hash, Hasher};
                                            let mut h = std::collections::hash_map::DefaultHasher::new();
                                            peer_name.hash(&mut h);
                                            let bytes = h.finish().to_le_bytes();
                                            id[..8].copy_from_slice(&bytes);
                                            id
                                        });
                                    ArtifactProvenance {
                                        original_steward: member_id,
                                        received_from: member_id,
                                        received_at: chrono::Utc::now().timestamp_millis(),
                                        received_via: ProvenanceType::CoStewardship,
                                    }
                                });

                                if let Ok(doc) = home.artifact_index().await {
                                    let entry = HomeArtifactEntry {
                                        id,
                                        name: name.clone(),
                                        mime_type: Some(mime),
                                        size,
                                        created_at: chrono::Utc::now().timestamp_millis(),
                                        encrypted_key: None,
                                        status: ArtifactStatus::Active,
                                        grants: vec![],
                                        provenance,
                                        location,
                                    };
                                    let _ = doc.update(|index| { index.store(entry); }).await;
                                }

                                let _ = action_event_tx.send(AppEvent::ArtifactStored(name));
                            }
                            Err(e) => {
                                tracing::error!("Failed to store artifact '{}': {}", name, e);
                                let _ = action_event_tx.send(AppEvent::ActionFailed {
                                    action: "store_artifact".into(),
                                    error: e.to_string(),
                                });
                            }
                        }
                    } else {
                        tracing::warn!("StoreArtifact: home realm not initialized");
                    }
                }

                Action::GrantArtifact { artifact_name, peer_name } => {
                    let home = {
                        let hr = home_realm_handle.read();
                        hr.as_ref().cloned()
                    };
                    if let Some(home) = home {
                        let peer_member_id = workspace.read().peers.entries.iter()
                            .find(|p| p.name == peer_name)
                            .map(|p| p.player_id);

                        if let Some(peer_id) = peer_member_id {
                            // Find artifact by name in index
                            let artifact_id = if let Ok(doc) = home.artifact_index().await {
                                let data = doc.read().await;
                                data.active_artifacts()
                                    .find(|e| e.name == artifact_name)
                                    .map(|e| e.id)
                            } else {
                                None
                            };

                            if let Some(id) = artifact_id {
                                match home.grant_access(&id, peer_id, AccessMode::Revocable).await {
                                    Ok(()) => {
                                        tracing::info!("Granted '{}' access to '{}'", peer_name, artifact_name);
                                        let _ = action_event_tx.send(AppEvent::ArtifactGranted {
                                            artifact_name,
                                            peer_name,
                                        });
                                    }
                                    Err(e) => {
                                        let err_str = e.to_string();
                                        if err_str.contains("already has access") {
                                            tracing::debug!("'{}' already has access to '{}'", peer_name, artifact_name);
                                        } else {
                                            tracing::error!("Grant failed for '{}': {}", artifact_name, e);
                                        }
                                    }
                                }
                            } else {
                                tracing::warn!("GrantArtifact: '{}' not found in index", artifact_name);
                            }
                        } else {
                            tracing::warn!("GrantArtifact: peer '{}' not in contacts", peer_name);
                        }
                    }
                }

                Action::Wait(secs) => {
                    tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
                }
            }
        }
    });

    // Query dispatcher — owns query_rx directly
    spawn(async move {
        loop {
            let Some((query, reply)) = query_rx.recv().await else {
                tracing::info!("QueryBus closed — Lua script finished");
                break;
            };

            tracing::debug!("Handling query: {:?}", query);

            let result = match query {
                Query::Identity => {
                    let name = workspace.read().event_log.iter()
                        .find_map(|e| {
                            e.message.strip_prefix("Identity loaded: ")
                                .or_else(|| e.message.strip_prefix("Identity created: "))
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| "Unknown".to_string());
                    QueryResult::Json(serde_json::json!({
                        "name": name,
                    }))
                }

                Query::AppPhase => {
                    let phase = workspace.read().phase.clone();
                    let phase_str = match phase {
                        AppPhase::Loading => "loading",
                        AppPhase::Setup => "setup",
                        AppPhase::Workspace => "workspace",
                    };
                    QueryResult::String(phase_str.to_string())
                }

                Query::ActiveView => {
                    let view = workspace.read().ui.active_view.clone();
                    let view_str = match view {
                        ViewType::Document => "document",
                        ViewType::Story => "story",
                        ViewType::Quest => "quest",
                        ViewType::Settings => "settings",
                        ViewType::Artifacts => "artifacts",
                    };
                    QueryResult::String(view_str.to_string())
                }

                Query::ActiveSidebarItem => {
                    let tree = workspace.read().nav.vault_tree.clone();
                    let active = tree.iter().find(|n| n.active).map(|n| n.label.clone());
                    match active {
                        Some(label) => QueryResult::String(label),
                        None => QueryResult::String(String::new()),
                    }
                }

                Query::PeerCount => {
                    let count = workspace.read().peers.entries.len() as f64;
                    QueryResult::Number(count)
                }

                Query::PeerNames => {
                    let names: Vec<String> = workspace
                        .read()
                        .peers
                        .entries
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    QueryResult::StringList(names)
                }

                Query::ChatMessageCount => {
                    QueryResult::Number(0.0)
                }

                Query::ChatMessages => {
                    QueryResult::Json(serde_json::json!([]))
                }

                Query::SidebarItems => {
                    let items: Vec<serde_json::Value> = workspace
                        .read()
                        .nav
                        .vault_tree
                        .iter()
                        .map(|n| {
                            serde_json::json!({
                                "label": n.label,
                                "icon": n.icon,
                            })
                        })
                        .collect();
                    QueryResult::Json(serde_json::Value::Array(items))
                }

                Query::EventLog => {
                    let entries: Vec<serde_json::Value> = workspace
                        .read()
                        .event_log
                        .iter()
                        .map(|e| {
                            let dir = match &e.direction {
                                EventDirection::Sent => "sent",
                                EventDirection::Received => "received",
                                EventDirection::System => "system",
                            };
                            serde_json::json!({
                                "direction": dir,
                                "text": e.message,
                            })
                        })
                        .collect();
                    QueryResult::Json(serde_json::Value::Array(entries))
                }

                Query::OverlayOpen => {
                    let ci_open = *contact_invite_open.read();
                    let slash_open = workspace.read().ui.slash_menu_open;
                    if ci_open {
                        QueryResult::String("contacts".into())
                    } else if slash_open {
                        QueryResult::String("slash_menu".into())
                    } else {
                        QueryResult::String(String::new())
                    }
                }

                Query::IdentityUri => {
                    let guard = network_handle.read();
                    if let Some(ref nh) = *guard {
                        QueryResult::String(nh.network.identity_uri())
                    } else {
                        QueryResult::Error("Network not initialized".into())
                    }
                }

                Query::ArtifactCount => {
                    let hr = home_realm_handle.read();
                    if let Some(ref home) = *hr {
                        if let Ok(doc) = home.artifact_index().await {
                            let data = doc.read().await;
                            QueryResult::Number(data.active_count() as f64)
                        } else {
                            QueryResult::Number(0.0)
                        }
                    } else {
                        QueryResult::Number(0.0)
                    }
                }

                Query::Artifacts => {
                    let hr = home_realm_handle.read();
                    if let Some(ref home) = *hr {
                        if let Ok(doc) = home.artifact_index().await {
                            let data = doc.read().await;
                            let items: Vec<serde_json::Value> = data.active_artifacts().map(|e| {
                                serde_json::json!({
                                    "name": e.name,
                                    "mime": e.mime_type,
                                    "size": e.size,
                                    "grant_count": e.grants.len(),
                                    "has_location": e.location.is_some(),
                                    "from_peer": e.provenance.is_some(),
                                })
                            }).collect();
                            QueryResult::Json(serde_json::Value::Array(items))
                        } else {
                            QueryResult::Json(serde_json::json!([]))
                        }
                    } else {
                        QueryResult::Json(serde_json::json!([]))
                    }
                }

                Query::Custom(name) => {
                    QueryResult::Error(format!("Unknown query: {}", name))
                }
            };

            let _ = reply.send(result);
        }
    });
}
