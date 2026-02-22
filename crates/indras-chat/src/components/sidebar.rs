//! Sidebar — conversation list with identity display and contact management.

use dioxus::prelude::*;
use crate::state::ChatContext;

/// Sidebar component showing conversations.
#[component]
pub fn Sidebar() -> Element {
    let mut ctx = use_context::<ChatContext>();
    let handle = ctx.handle.read().clone();
    let conversations = ctx.conversations.read().clone();
    let active_chat = ctx.active_chat.read().clone();

    let display_name = handle.network.display_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Anonymous".to_string());
    let identity_code = handle.network.identity_code();

    rsx! {
        div { class: "sidebar",
            // Identity section
            div { class: "sidebar-identity",
                div { class: "sidebar-identity-name", "{display_name}" }
                div {
                    class: "sidebar-identity-code",
                    title: "Click to copy",
                    "{identity_code}"
                }
            }

            // Header + add contact button
            div { class: "sidebar-header",
                div { class: "sidebar-title", "Chats" }
            }

            button {
                class: "add-contact-button",
                onclick: move |_| ctx.show_add_contact.set(true),
                "+ Add Contact"
            }

            // Conversation list
            div { class: "sidebar-conversations",
                if conversations.is_empty() {
                    div { class: "sidebar-empty",
                        "No conversations yet.\nAdd a contact to start chatting!"
                    }
                } else {
                    for convo in conversations.iter() {
                        {
                            let realm_id = convo.realm_id;
                            let is_active = active_chat == Some(realm_id);
                            let item_class = if is_active {
                                "conversation-item active"
                            } else {
                                "conversation-item"
                            };
                            let initial = convo.display_name.chars().next()
                                .unwrap_or('?').to_uppercase().to_string();
                            let preview = convo.last_message.clone()
                                .unwrap_or_else(|| "No messages yet".to_string());
                            let time_str = convo.last_message_time
                                .map(format_time)
                                .unwrap_or_default();
                            let unread = convo.unread_count;
                            let name = convo.display_name.clone();

                            rsx! {
                                div {
                                    key: "{realm_id:?}",
                                    class: "{item_class}",
                                    onclick: move |_| {
                                        ctx.active_chat.set(Some(realm_id));
                                    },
                                    div { class: "conversation-avatar", "{initial}" }
                                    div { class: "conversation-info",
                                        div { class: "conversation-name", "{name}" }
                                        div { class: "conversation-preview", "{preview}" }
                                    }
                                    div { class: "conversation-meta",
                                        div { class: "conversation-time", "{time_str}" }
                                        if unread > 0 {
                                            div { class: "unread-badge", "{unread}" }
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
}

/// Format a tick timestamp into a human-readable label.
fn format_time(tick: u64) -> String {
    if tick == 0 {
        return String::new();
    }
    format!("#{}", tick)
}
