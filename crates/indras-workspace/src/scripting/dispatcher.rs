//! Action dispatcher — polls the ActionBus and dispatches actions in RootApp.
//!
//! Also handles QueryBus replies and EventBus emissions.

use dioxus::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::action::Action;
use super::channels::AppTestChannels;
use super::event::AppEvent;
use super::query::{Query, QueryResult};
use crate::state::workspace::{WorkspaceState, ViewType, AppPhase, EventDirection, log_event};

/// Spawn the dispatcher as a Dioxus future.
/// This should be called inside the RootApp component where signals are available.
///
/// The dispatcher:
/// 1. Polls ActionBus for actions from Lua and dispatches them
/// 2. Polls QueryBus for queries from Lua and replies with state snapshots
pub fn spawn_dispatcher(
    channels: Arc<Mutex<AppTestChannels>>,
    mut workspace: Signal<WorkspaceState>,
    mut contact_invite_open: Signal<bool>,
    mut contact_invite_input: Signal<String>,
) {
    // Action dispatcher
    let channels_for_actions = Arc::clone(&channels);
    spawn(async move {
        loop {
            let action = {
                let mut ch = channels_for_actions.lock().await;
                ch.action_rx.recv().await
            };

            let Some(action) = action else {
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

                        // Emit events
                        let ch = channels_for_actions.lock().await;
                        let _ = ch.event_tx.send(AppEvent::SidebarItemActive(label));
                        let view_str = match view_type_str.as_str() {
                            "story" => "story",
                            "quest" => "quest",
                            _ => "document",
                        };
                        let _ = ch.event_tx.send(AppEvent::ViewChanged(view_str.to_string()));
                    } else {
                        tracing::warn!("ClickSidebar: no node with label '{}'", label);
                        let ch = channels_for_actions.lock().await;
                        let _ = ch.event_tx.send(AppEvent::ActionFailed {
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
                    let ch = channels_for_actions.lock().await;
                    let _ = ch.event_tx.send(AppEvent::ViewChanged(tab));
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
                    let ch = channels_for_actions.lock().await;
                    let _ = ch.event_tx.send(AppEvent::OverlayOpened("contacts".into()));
                }

                Action::PasteConnectCode(code) => {
                    contact_invite_input.set(code);
                }

                Action::ClickConnect => {
                    // The actual connection is handled by the on_connect callback in app.rs.
                    // We just trigger it by reading the current input value.
                    // For now, emit an event that the click happened.
                    tracing::info!("ClickConnect dispatched — connection will be handled by on_connect");
                }

                Action::ClickPeerDot(name) => {
                    // Navigate to peer's contact in sidebar
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
                    let ch = channels_for_actions.lock().await;
                    let _ = ch.event_tx.send(AppEvent::OverlayClosed("any".into()));
                }

                Action::TypeMessage(text) => {
                    // Will be connected to compose_text signal when chat is wired
                    log_event(&mut workspace.write(), EventDirection::System, format!("Lua: type_message(\"{}\")", text));
                }

                Action::SendMessage => {
                    log_event(&mut workspace.write(), EventDirection::System, "Lua: send_message()".to_string());
                }

                Action::ClickBlock(idx) => {
                    log_event(&mut workspace.write(), EventDirection::System, format!("Lua: click_block({})", idx));
                }

                Action::TypeInBlock(idx, text) => {
                    // Update block content
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

                Action::Wait(secs) => {
                    tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
                }
            }
        }
    });

    // Query dispatcher
    let channels_for_queries = channels;
    spawn(async move {
        loop {
            let query = {
                let mut ch = channels_for_queries.lock().await;
                ch.query_rx.recv().await
            };

            let Some((query, reply)) = query else {
                tracing::info!("QueryBus closed — Lua script finished");
                break;
            };

            tracing::debug!("Handling query: {:?}", query);

            let result = match query {
                Query::Identity => {
                    // Extract identity name from event log (logged at boot as "Identity loaded: X")
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
                    // Placeholder — will be wired when chat state is accessible
                    QueryResult::Number(0.0)
                }

                Query::ChatMessages => {
                    // Placeholder
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

                Query::Custom(name) => {
                    QueryResult::Error(format!("Unknown query: {}", name))
                }
            };

            let _ = reply.send(result);
        }
    });
}
