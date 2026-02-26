//! Chat view — displays messages for the active conversation.

use dioxus::prelude::*;
use futures::StreamExt;
use indras_network::RealmId;
use crate::state::{ChatContext, SystemEventSnapshot};
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

/// A timeline entry — either a user message or a system event.
#[derive(Clone, Debug)]
enum TimelineEntry {
    Message(MessageSnapshot),
    System(SystemEventSnapshot),
}

impl TimelineEntry {
    fn timestamp(&self) -> u64 {
        match self {
            Self::Message(m) => m.created_at,
            Self::System(s) => s.timestamp,
        }
    }
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
    let mut timeline = use_signal(Vec::<TimelineEntry>::new);
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

            // Load initial messages + persisted system events
            {
                let state = doc.read().await;
                let snapshots = build_snapshots(&*state);
                let mut entries: Vec<TimelineEntry> = snapshots.into_iter().map(TimelineEntry::Message).collect();
                // Restore persisted system events for this realm
                if let Some(saved) = ctx.system_events.read().get(&realm_id) {
                    entries.extend(saved.iter().cloned().map(TimelineEntry::System));
                }
                entries.sort_by_key(|e| e.timestamp());
                timeline.set(entries);
            }

            // Subscribe to changes
            let mut changes = doc.changes();
            while let Some(change) = changes.next().await {
                let snapshots = build_snapshots(&change.new_state);
                let mut current = timeline.read().clone();
                // Keep system events, replace messages
                current.retain(|e| matches!(e, TimelineEntry::System(_)));
                current.extend(snapshots.into_iter().map(TimelineEntry::Message));
                current.sort_by_key(|e| e.timestamp());
                timeline.set(current);
            }
        });
    });

    // Listen for system events (transport, gossip, sync)
    use_effect(move || {
        let handle = ctx.handle.read().clone();
        let mut sys_events = ctx.system_events;
        spawn(async move {
            let realm = match handle.network.get_realm_by_id(&realm_id) {
                Some(r) => r,
                None => return,
            };
            let mut events = std::pin::pin!(realm.system_events());
            let mut counter = 0u64;
            while let Some(evt) = events.next().await {
                counter += 1;
                let snapshot = SystemEventSnapshot {
                    id: format!("sys-{}-{}", evt.timestamp(), counter),
                    text: evt.display_text(),
                    timestamp: evt.timestamp(),
                };

                // Persist to context (survives chat switching)
                {
                    let mut map = sys_events.write();
                    let realm_events = map.entry(realm_id).or_insert_with(Vec::new);
                    realm_events.push(snapshot.clone());
                    // Cap at 200 per realm
                    if realm_events.len() > 200 {
                        let drain_count = realm_events.len() - 200;
                        realm_events.drain(..drain_count);
                    }
                }

                // Update local timeline
                let mut current = timeline.read().clone();
                current.push(TimelineEntry::System(snapshot));
                current.sort_by_key(|e| e.timestamp());
                timeline.set(current);
            }
        });
    });

    let current_timeline = timeline.read().clone();
    let chat_title = chat_name.read().clone();
    let my_id_display = my_id.clone();

    rsx! {
        div { class: "chat-view",
            // Header
            div { class: "chat-panel-header",
                div { class: "bubble-avatar member-light",
                    {chat_title.chars().next().unwrap_or('?').to_uppercase().to_string()}
                }
                div {
                    h2 { class: "panel-title", "{chat_title}" }
                }
            }

            // Messages and system events
            div { class: "chat-messages",
                for entry in current_timeline.iter() {
                    {
                        match entry {
                            TimelineEntry::System(evt) => {
                                let ts_secs = evt.timestamp / 1000;
                                let mins = (ts_secs / 60) % 60;
                                let hours = (ts_secs / 3600) % 24;
                                let time_str = format!("{:02}:{:02}", hours, mins);
                                rsx! {
                                    div {
                                        key: "{evt.id}",
                                        class: "system-event-row",
                                        div { class: "system-event-bubble",
                                            span { class: "system-event-text", "{evt.text}" }
                                            span { class: "system-event-time", "{time_str}" }
                                        }
                                    }
                                }
                            }
                            TimelineEntry::Message(msg) => {
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
                                            class: "chat-bubble-row bubble-left",
                                            div { class: "chat-bubble chat-bubble-received bubble-deleted",
                                                div { class: "bubble-deleted-text", "This message was deleted" }
                                            }
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
                }
            }

            // Error display
            if let Some(ref err) = *send_error.read() {
                div { class: "chat-error-toast",
                    span { "{err}" }
                }
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
                on_typing: move |_: ()| {},
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
