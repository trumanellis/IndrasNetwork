//! Peer realm chat screen - 1:1 messaging with a contact.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{Content, IndrasNetwork, Message};
use indras_sync_engine::SyncContent;
use indras_ui::member_color_class;

use crate::state::{
    GenesisState, GenesisStep, PeerMessageType, PeerMessageView,
};

/// Convert a network Message to a PeerMessageView for rendering.
fn convert_message(msg: &Message, my_id: &[u8; 32]) -> PeerMessageView {
    let sender_id = msg.sender.id();
    let sender_id_short: String = sender_id
        .iter()
        .take(8)
        .map(|b| format!("{:02x}", b))
        .collect();
    let is_me = sender_id == *my_id;

    let message_type = match &msg.content {
        Content::Text(s) => PeerMessageType::Text {
            content: s.clone(),
        },
        Content::Image {
            mime_type,
            data,
            filename,
            alt_text,
            ..
        } => PeerMessageType::Image {
            data_url: Some(format!("data:{};base64,{}", mime_type, data)),
            filename: filename.clone(),
            alt_text: alt_text.clone(),
        },
        Content::System(s) => PeerMessageType::System {
            content: s.clone(),
        },
        Content::Artifact(r) => PeerMessageType::Artifact {
            name: r.name.clone(),
            size: r.size,
            mime_type: r.mime_type.clone(),
        },
        Content::Extension { .. } => {
            match SyncContent::from_content(&msg.content) {
                Some(SyncContent::ProofSubmitted {
                    quest_id,
                    claimant,
                    ..
                }) => PeerMessageType::ProofSubmitted {
                    quest_id_short: quest_id
                        .iter()
                        .take(4)
                        .map(|b| format!("{:02x}", b))
                        .collect(),
                    claimant_name: claimant
                        .iter()
                        .take(4)
                        .map(|b| format!("{:02x}", b))
                        .collect(),
                },
                Some(SyncContent::BlessingGiven {
                    claimant,
                    ..
                }) => PeerMessageType::BlessingGiven {
                    claimant_name: claimant
                        .iter()
                        .take(4)
                        .map(|b| format!("{:02x}", b))
                        .collect(),
                    duration: String::new(),
                },
                Some(SyncContent::ProofFolderSubmitted {
                    narrative_preview,
                    artifact_count,
                    ..
                }) => PeerMessageType::ProofFolderSubmitted {
                    narrative_preview: narrative_preview.clone(),
                    artifact_count,
                },
                _ => PeerMessageType::System {
                    content: "[unknown extension message]".to_string(),
                },
            }
        },
        Content::Gallery { title, items, .. } => PeerMessageType::Gallery {
            title: title.clone(),
            item_count: items.len(),
        },
        Content::Reaction { emoji, .. } => PeerMessageType::Reaction {
            emoji: emoji.clone(),
        },
        _ => PeerMessageType::System {
            content: "[unsupported message type]".to_string(),
        },
    };

    PeerMessageView {
        sender_name: msg.sender.name(),
        sender_id_short,
        is_me,
        timestamp: msg.timestamp.format("%H:%M").to_string(),
        message_type,
    }
}

/// Join or get a peer realm, safely handling the blocking_read() inside realm().
///
/// The `IndrasNetwork::realm()` method internally calls `contacts_list()` which
/// uses `tokio::sync::RwLock::blocking_read()`. This panics when called from an
/// async context. We spawn a raw std::thread (which has no tokio runtime context)
/// and use `Handle::block_on` to drive the future. On this thread, blocking_read()
/// is safe because there's no async runtime context to conflict with.
async fn get_peer_realm(
    net: &Arc<IndrasNetwork>,
    peers: Vec<[u8; 32]>,
) -> Result<indras_network::Realm, indras_network::IndraError> {
    let net = Arc::clone(net);
    let handle = tokio::runtime::Handle::current();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let result = handle.block_on(net.realm(peers));
        let _ = tx.send(result);
    });
    rx.await.unwrap_or_else(|_| {
        Err(indras_network::IndraError::InvalidOperation(
            "realm join thread failed".to_string(),
        ))
    })
}

/// Load all messages from a realm and update state.
async fn load_messages(
    net: &Arc<IndrasNetwork>,
    peer_id: [u8; 32],
    state: &mut Signal<GenesisState>,
) {
    let my_id = net.id();
    let peers = vec![my_id, peer_id];
    match get_peer_realm(net, peers).await {
        Ok(realm) => {
            match realm.messages_since(0).await {
                Ok(messages) => {
                    let count = messages.len();
                    let views: Vec<PeerMessageView> = messages
                        .iter()
                        .map(|m| convert_message(m, &my_id))
                        .collect();
                    let mut s = state.write();
                    s.peer_realm_messages = views;
                    s.peer_realm_message_count = count;
                    s.peer_realm_last_seq = count as u64;
                }
                Err(e) => {
                    tracing::error!("load_messages: messages_since failed: {}", e);
                }
            }
        }
        Err(e) => {
            tracing::error!("load_messages: realm() failed: {}", e);
        }
    }
}

#[component]
pub fn PeerRealmScreen(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: [u8; 32],
) -> Element {
    // Initial message load
    use_effect(move || {
        spawn(async move {
            let net = {
                let guard = network.read();
                guard.as_ref().cloned()
            };
            if let Some(net) = net {
                load_messages(&net, peer_id, &mut state).await;
            }
        });
    });

    // 3-second polling loop for new messages
    use_effect(move || {
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let net = {
                    let guard = network.read();
                    guard.as_ref().cloned()
                };
                if let Some(net) = net {
                    // Only poll if we're still on this peer realm
                    let current_step = state.read().step.clone();
                    if current_step != GenesisStep::PeerRealm(peer_id) {
                        break;
                    }
                    load_messages(&net, peer_id, &mut state).await;
                }
            }
        });
    });

    // Read state for rendering
    let s = state.read();
    let messages = s.peer_realm_messages.clone();
    let message_count = s.peer_realm_message_count;
    let contact_name = s
        .peer_realm_contact_name
        .clone()
        .unwrap_or_else(|| "Contact".to_string());
    let draft = s.peer_realm_draft.clone();
    let action_menu_open = s.peer_realm_action_menu_open;
    drop(s);

    let draft_empty = draft.trim().is_empty();

    rsx! {
        div {
            class: "genesis-screen peer-realm-screen",

            // Header
            div {
                class: "peer-realm-header",

                button {
                    class: "genesis-btn-secondary",
                    onclick: move |_| {
                        state.write().step = GenesisStep::HomeRealm;
                    },
                    "\u{2190} Back"
                }

                h2 {
                    class: "peer-realm-title",
                    "Chat with {contact_name}"
                }

                span {
                    class: "peer-realm-stats",
                    "{message_count} messages"
                }
            }

            // Messages area
            div {
                class: "chat-messages",

                if messages.is_empty() {
                    div {
                        class: "panel-empty",
                        "No messages yet. Send the first message!"
                    }
                }

                for (i, msg) in messages.iter().enumerate() {
                    {render_chat_message(msg, i)}
                }
            }

            // Input bar
            div {
                class: "chat-input-container",

                div {
                    class: "chat-input-wrapper",

                    button {
                        class: "chat-action-btn",
                        onclick: move |_| {
                            let mut s = state.write();
                            s.peer_realm_action_menu_open = !s.peer_realm_action_menu_open;
                        },
                        "+"
                    }

                    if action_menu_open {
                        div {
                            class: "chat-action-menu",

                            button {
                                class: "action-menu-item",
                                onclick: move |_| {
                                    state.write().peer_realm_action_menu_open = false;
                                },
                                "\u{1f4ce} Artifact"
                            }
                            button {
                                class: "action-menu-item",
                                onclick: move |_| {
                                    state.write().peer_realm_action_menu_open = false;
                                },
                                "\u{1f4c4} Document"
                            }
                            button {
                                class: "action-menu-item",
                                onclick: move |_| {
                                    state.write().peer_realm_action_menu_open = false;
                                },
                                "\u{2713} Proof of Service"
                            }
                        }
                    }
                }

                input {
                    class: "chat-input",
                    r#type: "text",
                    placeholder: "Type a message...",
                    value: "{draft}",
                    oninput: move |evt| {
                        state.write().peer_realm_draft = evt.value();
                    },
                    onkeypress: move |evt| {
                        if evt.key() == Key::Enter {
                            let draft = state.read().peer_realm_draft.clone();
                            if !draft.trim().is_empty() {
                                send_message(state, network, peer_id);
                            }
                        }
                    },
                }

                button {
                    class: "chat-send-btn",
                    disabled: draft_empty,
                    onclick: move |_| {
                        send_message(state, network, peer_id);
                    },
                    "Send"
                }
            }
        }
    }
}

/// Send the current draft as a text message.
fn send_message(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: [u8; 32],
) {
    let draft = state.read().peer_realm_draft.clone();
    if draft.trim().is_empty() {
        return;
    }
    spawn(async move {
        let net = {
            let guard = network.read();
            guard.as_ref().cloned()
        };
        if let Some(net) = net {
            let my_id = net.id();
            let peers = vec![my_id, peer_id];
            if let Ok(realm) = get_peer_realm(&net, peers).await {
                if realm.send(draft.as_str()).await.is_ok() {
                    state.write().peer_realm_draft.clear();
                    // Refresh messages
                    load_messages(&net, peer_id, &mut state).await;
                }
            }
        }
    });
}

/// Render a single chat message based on its type.
fn render_chat_message(msg: &PeerMessageView, index: usize) -> Element {
    let color_class = member_color_class(&msg.sender_id_short);
    let sender = msg.sender_name.clone();
    let timestamp = msg.timestamp.clone();
    let is_me = msg.is_me;

    match &msg.message_type {
        PeerMessageType::Text { content } => {
            let content = content.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message text-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-content", "{content}" }
                    }
                }
            }
        }

        PeerMessageType::Image {
            data_url,
            filename,
            alt_text,
        } => {
            let alt = alt_text
                .clone()
                .or_else(|| filename.clone())
                .unwrap_or_else(|| "Image".to_string());
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message image-message",

                    div {
                        class: "chat-message-header",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-content", "shared an image" }
                    }

                    if let Some(url) = data_url {
                        div {
                            class: "chat-image-container",
                            img {
                                class: "chat-inline-image",
                                src: "{url}",
                                alt: "{alt}",
                            }
                        }
                    } else {
                        div {
                            class: "chat-image-placeholder",
                            "\u{1f5bc} {alt}"
                        }
                    }
                }
            }
        }

        PeerMessageType::System { content } => {
            let content = content.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message system-message",
                    span { class: "chat-content", "{content}" }
                }
            }
        }

        PeerMessageType::Artifact {
            name,
            size,
            mime_type,
        } => {
            let name = name.clone();
            let size_str = format_size(*size);
            let mime = mime_type
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message proof-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-icon", "\u{1f4ce}" }
                        span { class: "chat-content", "{name} ({size_str}, {mime})" }
                    }
                }
            }
        }

        PeerMessageType::ProofSubmitted {
            quest_id_short,
            claimant_name,
        } => {
            let quest_id_short = quest_id_short.clone();
            let claimant_name = claimant_name.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message proof-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span { class: "chat-icon", "\u{1f4ce}" }
                        span { class: "chat-content", "Proof submitted for quest {quest_id_short} by {claimant_name}" }
                    }
                }
            }
        }

        PeerMessageType::BlessingGiven {
            claimant_name,
            duration,
        } => {
            let claimant_name = claimant_name.clone();
            let duration = duration.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message blessing-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span { class: "chat-icon", "\u{2728}" }
                        span { class: "chat-content",
                            if duration.is_empty() {
                                "Blessing given to {claimant_name}"
                            } else {
                                "Blessing given to {claimant_name} ({duration})"
                            }
                        }
                    }
                }
            }
        }

        PeerMessageType::ProofFolderSubmitted {
            narrative_preview,
            artifact_count,
        } => {
            let preview = narrative_preview.clone();
            let count = *artifact_count;
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message proof-folder-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span { class: "chat-icon", "\u{1f4cb}" }
                        span { class: "chat-content", "{preview}" }
                    }
                    span { class: "proof-artifact", "{count} attachment(s)" }
                }
            }
        }

        PeerMessageType::Gallery { title, item_count } => {
            let title_str = title
                .clone()
                .unwrap_or_else(|| "Gallery".to_string());
            let count = *item_count;
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message gallery-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-icon", "\u{1f5bc}" }
                        span { class: "gallery-title", "{title_str}" }
                        span { class: "gallery-count", "({count} items)" }
                    }
                }
            }
        }

        PeerMessageType::Reaction { emoji } => {
            let emoji = emoji.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message text-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-content", "{emoji}" }
                    }
                }
            }
        }
    }
}

/// Format a byte size into a human-readable string.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
