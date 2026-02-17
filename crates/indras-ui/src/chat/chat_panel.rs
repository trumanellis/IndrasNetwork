//! Top-level chat panel container.
//!
//! Creates `Signal<ChatState>`, spawns stream listener for real-time updates,
//! owns send/edit/delete handlers.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_network::direct_connect::dm_realm_id;
use indras_sync_engine::RealmChat;
use tracing::debug;

use super::chat_input::ChatInput;
use super::chat_messages::ChatMessageList;
use super::chat_state::{convert_editable_to_view, ChatState, ChatStatus};

/// Re-read messages from the chat document and update the signal.
///
/// Called after own mutations (send/edit/delete) to work around Document
/// instance isolation — each `chat_document()` call creates a new instance
/// with its own broadcast channel, so the stream listener never sees our
/// own changes.
async fn reload_messages(
    chat: &mut Signal<ChatState>,
    realm: &indras_network::Realm,
    my_id_hex: &str,
    peer_name: &str,
) {
    if let Ok(doc) = realm.chat_document().await {
        let data = doc.read().await;
        let views: Vec<_> = data
            .visible_messages()
            .iter()
            .map(|m| convert_editable_to_view(m, my_id_hex, peer_name))
            .collect();
        let mut s = chat.write();
        s.messages = views;
        s.should_scroll_bottom = true;
    }
}

/// Get or lazily create the DM realm for a peer.
///
/// First checks the in-memory cache. If the realm isn't there (e.g. after
/// restart), falls back to `connect()` which is idempotent — it re-creates
/// the DM realm from the deterministic seed and re-caches it.
async fn get_or_create_peer_realm(
    net: &Arc<IndrasNetwork>,
    my_id: [u8; 32],
    peer_id: [u8; 32],
) -> Result<indras_network::Realm, indras_network::IndraError> {
    let dm_id = dm_realm_id(my_id, peer_id);
    if let Some(realm) = net.get_realm_by_id(&dm_id) {
        return Ok(realm);
    }
    // DM realm not in memory — create it (handles restart + race conditions)
    net.connect(peer_id).await
}

/// Hex-encode a 32-byte ID.
fn hex32(id: &[u8; 32]) -> String {
    id.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Get current tick (millis since epoch).
fn current_tick() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Top-level chat panel component.
///
/// Manages its own `ChatState` signal. Spawns a stream listener for
/// real-time document updates, with a 30-second fallback poll.
#[component]
pub fn ChatPanel(
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: [u8; 32],
    my_id: [u8; 32],
    peer_name: String,
) -> Element {
    let mut chat = use_signal(ChatState::default);
    let my_id_hex = hex32(&my_id);
    let peer_name_for_load = peer_name.clone();

    // Initial load + stream listener
    use_effect({
        let my_id_hex = my_id_hex.clone();
        let peer_name = peer_name_for_load.clone();
        move || {
            let my_id_hex = my_id_hex.clone();
            let peer_name = peer_name.clone();
            spawn(async move {
                let net = {
                    let guard = network.read();
                    guard.as_ref().cloned()
                };
                let Some(net) = net else { return; };

                let realm = match get_or_create_peer_realm(&net, my_id, peer_id).await {
                    Ok(r) => r,
                    Err(e) => {
                        debug!(error = %e, "ChatPanel: failed to get peer realm");
                        chat.write().status = ChatStatus::Idle;
                        return;
                    }
                };

                let doc = match realm.chat_document().await {
                    Ok(d) => d,
                    Err(e) => {
                        debug!(error = %e, "ChatPanel: failed to get chat document");
                        chat.write().status = ChatStatus::Idle;
                        return;
                    }
                };

                // Initial load - refresh from storage/peers
                let _ = doc.refresh().await;
                {
                    let data = doc.read().await;
                    let views: Vec<_> = data
                        .visible_messages()
                        .iter()
                        .map(|m| convert_editable_to_view(m, &my_id_hex, &peer_name))
                        .collect();
                    let mut s = chat.write();
                    s.messages = views;
                    s.should_scroll_bottom = true;
                    s.status = ChatStatus::Idle;
                }

                // Stream-based updates
                let mut changes = doc.changes();
                use futures::StreamExt;

                // Also spawn a fallback refresh every 30 seconds
                let doc_clone = doc.clone();
                let refresh_handle = tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        let _ = doc_clone.refresh().await;
                    }
                });

                while let Some(change) = changes.next().await {
                    let views: Vec<_> = change
                        .new_state
                        .visible_messages()
                        .iter()
                        .map(|m| convert_editable_to_view(m, &my_id_hex, &peer_name))
                        .collect();
                    let mut s = chat.write();
                    s.messages = views;
                    s.should_scroll_bottom = true;
                }

                refresh_handle.abort();
            });
        }
    });

    // Error auto-dismiss after 5 seconds
    let has_error = chat.read().error.is_some();
    use_effect(move || {
        if has_error {
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                chat.write().error = None;
            });
        }
    });

    // Read state for rendering
    let s = chat.read();
    let messages = s.messages.clone();
    let message_count = messages.len();
    let draft = s.draft.clone();
    let status = s.status.clone();
    let error = s.error.clone();
    let action_menu_open = s.action_menu_open;
    let editing_id = s.editing_id.clone();
    let edit_draft = s.edit_draft.clone();
    let should_scroll = s.should_scroll_bottom;
    drop(s);

    // Event handlers
    let on_send = {
        let my_id_hex = my_id_hex.clone();
        let peer_name = peer_name.clone();
        move |text: String| {
            let my_id_hex = my_id_hex.clone();
            let peer_name = peer_name.clone();
            spawn(async move {
                chat.write().status = ChatStatus::Sending;
                chat.write().draft.clear();

                let net = {
                    let guard = network.read();
                    guard.as_ref().cloned()
                };
                let Some(net) = net else {
                    chat.write().status = ChatStatus::Idle;
                    return;
                };

                let realm = match get_or_create_peer_realm(&net, my_id, peer_id).await {
                    Ok(r) => r,
                    Err(e) => {
                        chat.write().draft = text;
                        chat.write().error = Some(e.to_string());
                        chat.write().status = ChatStatus::Idle;
                        return;
                    }
                };

                let dm_id = dm_realm_id(my_id, peer_id);
                let realm_id_hex: String = dm_id.as_bytes().iter().map(|b| format!("{:02x}", b)).collect();
                let tick = current_tick();

                match realm.send_chat(&realm_id_hex, &my_id_hex, text.clone(), tick).await {
                    Ok(_) => {
                        reload_messages(&mut chat, &realm, &my_id_hex, &peer_name).await;
                        chat.write().status = ChatStatus::Idle;
                    }
                    Err(e) => {
                        chat.write().draft = text;
                        chat.write().error = Some(e.to_string());
                        chat.write().status = ChatStatus::Idle;
                    }
                }
            });
        }
    };

    let on_edit_start = move |(id, content): (String, String)| {
        let mut s = chat.write();
        s.editing_id = Some(id);
        s.edit_draft = content;
    };

    let on_edit_save = {
        let my_id_hex = my_id_hex.clone();
        let peer_name = peer_name.clone();
        move |(msg_id, new_content): (String, String)| {
            let my_id_hex = my_id_hex.clone();
            let peer_name = peer_name.clone();
            spawn(async move {
                let net = {
                    let guard = network.read();
                    guard.as_ref().cloned()
                };
                let Some(net) = net else { return; };

                let realm = match get_or_create_peer_realm(&net, my_id, peer_id).await {
                    Ok(r) => r,
                    Err(e) => {
                        chat.write().error = Some(e.to_string());
                        return;
                    }
                };

                let tick = current_tick();
                match realm.edit_chat(&msg_id, &my_id_hex, new_content, tick).await {
                    Ok(true) => {
                        reload_messages(&mut chat, &realm, &my_id_hex, &peer_name).await;
                        let mut s = chat.write();
                        s.editing_id = None;
                        s.edit_draft.clear();
                    }
                    Ok(false) => {
                        chat.write().error = Some("Edit failed".into());
                    }
                    Err(e) => {
                        chat.write().error = Some(e.to_string());
                    }
                }
            });
        }
    };

    let on_edit_cancel = move |_: ()| {
        let mut s = chat.write();
        s.editing_id = None;
        s.edit_draft.clear();
    };

    let on_delete = {
        let my_id_hex = my_id_hex.clone();
        let peer_name = peer_name.clone();
        move |msg_id: String| {
            let my_id_hex = my_id_hex.clone();
            let peer_name = peer_name.clone();
            spawn(async move {
                let net = {
                    let guard = network.read();
                    guard.as_ref().cloned()
                };
                let Some(net) = net else { return; };

                let realm = match get_or_create_peer_realm(&net, my_id, peer_id).await {
                    Ok(r) => r,
                    Err(e) => {
                        chat.write().error = Some(e.to_string());
                        return;
                    }
                };

                let tick = current_tick();
                match realm.delete_chat(&msg_id, &my_id_hex, tick).await {
                    Ok(_) => {
                        reload_messages(&mut chat, &realm, &my_id_hex, &peer_name).await;
                    }
                    Err(e) => {
                        chat.write().error = Some(e.to_string());
                    }
                }
            });
        }
    };

    let on_draft_change = move |text: String| {
        chat.write().draft = text;
    };

    let on_edit_draft_change = move |text: String| {
        chat.write().edit_draft = text;
    };

    let on_action_toggle = move |_: ()| {
        let mut s = chat.write();
        s.action_menu_open = !s.action_menu_open;
    };

    let on_action_close = move |_: ()| {
        chat.write().action_menu_open = false;
    };

    rsx! {
        div {
            class: "chat-panel-header",
            h2 { class: "panel-title", "Chat" }
            span { class: "panel-count", "{message_count}" }
        }

        ChatMessageList {
            messages,
            status: status.clone(),
            on_edit_start,
            on_edit_save,
            on_edit_cancel,
            on_delete,
            editing_id,
            edit_draft,
            on_edit_draft_change,
            should_scroll_bottom: should_scroll,
        }

        ChatInput {
            draft,
            status,
            error,
            on_send,
            on_draft_change,
            action_menu_open,
            on_action_toggle,
            on_action_close,
        }
    }
}
