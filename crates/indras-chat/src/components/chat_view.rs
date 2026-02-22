//! Chat view — displays messages for the active conversation.

use dioxus::prelude::*;
use futures::StreamExt;
use indras_network::{Content, RealmId};
use indras_network::chat_message::{TypingIndicator, TYPING_EXTENSION_TYPE};
use crate::state::ChatContext;
use super::message_bubble::DeliveryStatus;

/// A snapshot of a chat message for display.
#[derive(Clone, Debug)]
struct MessageSnapshot {
    id: String,
    author: String,
    content: String,
    created_at: u64,
    is_deleted: bool,
    is_edited: bool,
    reply_preview: Option<(String, String)>,
    reactions: Vec<(String, usize)>,
}

/// Chat view component — shows empty state or the active conversation.
#[component]
pub fn ChatView() -> Element {
    let ctx = use_context::<ChatContext>();
    let active_chat = ctx.active_chat.read().clone();

    match active_chat {
        None => rsx! {
            div { class: "chat-view",
                div { class: "chat-empty",
                    div { class: "chat-empty-icon", "💬" }
                    div { class: "chat-empty-text", "Select a conversation to start chatting" }
                }
            }
        },
        Some(realm_id) => rsx! {
            ActiveChat { realm_id }
        },
    }
}

/// Active chat conversation display.
#[component]
fn ActiveChat(realm_id: RealmId) -> Element {
    let ctx = use_context::<ChatContext>();
    let handle = ctx.handle.read().clone();
    let mut messages = use_signal(Vec::<MessageSnapshot>::new);
    let mut chat_name = use_signal(|| "Chat".to_string());
    let mut send_error = use_signal(|| None::<String>);

    // Hex-encode our own identity for "is_mine" detection.
    // MemberId is [u8; 32] via indras_artifacts::PlayerId (Deref to [u8; 32]).
    let my_id = hex::encode(handle.network.id().as_ref());
    let my_id_for_effect = my_id.clone();

    // Load messages and subscribe to changes whenever realm_id changes.
    use_effect(move || {
        let handle = handle.clone();
        let _my_id_inner = my_id_for_effect.clone();

        spawn(async move {
            let realm = match handle.network.get_realm_by_id(&realm_id) {
                Some(r) => r,
                None => return,
            };

            // Set chat name
            if let Some(name) = realm.name() {
                chat_name.set(name.to_string());
            }

            // Load chat document
            let doc = match realm.chat_doc().await {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Failed to load chat doc: {}", e);
                    return;
                }
            };

            // Load initial messages
            {
                let state = doc.read().await;
                let snapshots = build_snapshots(&*state);
                messages.set(snapshots);
            }

            // Subscribe to changes
            let mut changes = doc.changes();
            while let Some(change) = changes.next().await {
                let snapshots = build_snapshots(&change.new_state);
                messages.set(snapshots);
            }
        });
    });

    // Listen for typing indicators from peers
    use_effect(move || {
        let handle = ctx.handle.read().clone();
        let mut typing_peers = ctx.typing_peers;
        spawn(async move {
            let realm = match handle.network.get_realm_by_id(&realm_id) {
                Some(r) => r,
                None => return,
            };
            let msg_stream = realm.messages();
            let mut msg_stream = std::pin::pin!(msg_stream);
            while let Some(msg) = msg_stream.next().await {
                if let Content::Extension { ref type_id, ref payload } = msg.content {
                    if type_id == TYPING_EXTENSION_TYPE {
                        if let Ok(indicator) = serde_json::from_slice::<TypingIndicator>(payload) {
                            let name = msg.sender.name();
                            if indicator.is_typing {
                                let mut peers = typing_peers.read().clone();
                                if !peers.contains(&name) {
                                    peers.push(name.clone());
                                    typing_peers.set(peers);
                                }
                                // Auto-dismiss after 5 seconds
                                let dismiss_name = name;
                                spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                    let mut peers = typing_peers.read().clone();
                                    peers.retain(|n| n != &dismiss_name);
                                    typing_peers.set(peers);
                                });
                            } else {
                                let mut peers = typing_peers.read().clone();
                                peers.retain(|n| n != &name);
                                typing_peers.set(peers);
                            }
                        }
                    }
                }
            }
        });
    });

    let current_messages = messages.read().clone();
    let chat_title = chat_name.read().clone();
    let my_id_display = my_id.clone();

    rsx! {
        div { class: "chat-view",
            // Header
            div { class: "chat-header",
                div { class: "chat-header-avatar",
                    {chat_title.chars().next().unwrap_or('?').to_uppercase().to_string()}
                }
                div {
                    div { class: "chat-header-name", "{chat_title}" }
                    {
                        let typing = ctx.typing_peers.read().clone();
                        if !typing.is_empty() {
                            let names = typing.join(", ");
                            rsx! {
                                div { class: "typing-indicator", "{names} typing..." }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }
            }

            // Messages
            div { class: "chat-messages",
                for msg in current_messages.iter() {
                    {
                        let is_mine = msg.author == my_id_display
                            || ctx.handle.read().network.display_name()
                                .is_some_and(|n| n == msg.author);
                        let status = if is_mine {
                            DeliveryStatus::Sent
                        } else {
                            DeliveryStatus::Delivered
                        };

                        if msg.is_deleted {
                            rsx! {
                                div {
                                    key: "{msg.id}",
                                    class: "message-bubble theirs",
                                    div { class: "message-deleted", "This message was deleted" }
                                }
                            }
                        } else {
                            rsx! {
                                super::message_bubble::MessageBubble {
                                    key: "{msg.id}",
                                    content: msg.content.clone(),
                                    author: msg.author.clone(),
                                    is_mine,
                                    timestamp: msg.created_at,
                                    status,
                                    is_edited: msg.is_edited,
                                    reply_preview: msg.reply_preview.clone(),
                                    reactions: msg.reactions.clone(),
                                }
                            }
                        }
                    }
                }
            }

            // Error display
            if let Some(ref err) = *send_error.read() {
                div { class: "setup-error", "{err}" }
            }

            // Message input
            super::message_input::MessageInput {
                on_send: move |text: String| {
                    let handle = ctx.handle.read().clone();
                    send_error.set(None);
                    spawn(async move {
                        let realm = match handle.network.get_realm_by_id(&realm_id) {
                            Some(r) => r,
                            None => {
                                send_error.set(Some("Realm not found".to_string()));
                                return;
                            }
                        };
                        let author = handle.network.display_name()
                            .unwrap_or("Anonymous")
                            .to_string();
                        if let Err(e) = realm.chat_send(&author, text).await {
                            send_error.set(Some(format!("Send failed: {}", e)));
                        }
                    });
                },
                on_typing: move |_: ()| {
                    let handle = ctx.handle.read().clone();
                    spawn(async move {
                        let realm = match handle.network.get_realm_by_id(&realm_id) {
                            Some(r) => r,
                            None => return,
                        };
                        let indicator = TypingIndicator {
                            is_typing: true,
                            realm_id: format!("{:?}", realm_id),
                        };
                        let payload = serde_json::to_vec(&indicator).unwrap_or_default();
                        let _ = realm.send(Content::Extension {
                            type_id: TYPING_EXTENSION_TYPE.to_string(),
                            payload,
                        }).await;
                    });
                },
            }
        }
    }
}

/// Build display snapshots from the chat document state.
fn build_snapshots(state: &indras_network::RealmChatDocument) -> Vec<MessageSnapshot> {
    let sorted = state.visible_messages();
    sorted.iter().map(|msg| {
        let reply_preview = msg.reply_to.as_ref()
            .and_then(|id| state.reply_preview(id));
        let reactions: Vec<(String, usize)> = msg.reactions.iter()
            .map(|(emoji, authors)| (emoji.clone(), authors.len()))
            .collect();
        MessageSnapshot {
            id: msg.id.clone(),
            author: msg.author.clone(),
            content: msg.current_content.clone(),
            created_at: msg.created_at,
            is_deleted: msg.is_deleted,
            is_edited: msg.is_edited(),
            reply_preview,
            reactions,
        }
    }).collect()
}
