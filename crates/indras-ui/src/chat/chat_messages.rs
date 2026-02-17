//! Chat message list with auto-scroll, message grouping, and typing indicator.

use dioxus::prelude::*;

use super::chat_bubble::ChatBubble;
use super::chat_state::{ChatMessageView, ChatStatus, TypingPeerView};

/// Message list component with auto-scroll, bubble layout, and typing indicator.
#[component]
pub fn ChatMessageList(
    messages: Vec<ChatMessageView>,
    status: ChatStatus,
    on_reply: EventHandler<String>,
    on_react: EventHandler<(String, String)>,
    on_edit_start: EventHandler<(String, String)>,
    on_edit_save: EventHandler<(String, String)>,
    on_edit_cancel: EventHandler<()>,
    on_delete: EventHandler<String>,
    editing_id: Option<String>,
    edit_draft: String,
    on_edit_draft_change: EventHandler<String>,
    should_scroll_bottom: bool,
    typing_peers: Vec<TypingPeerView>,
    reaction_picker_msg_id: Option<String>,
    on_reaction_picker_toggle: EventHandler<String>,
) -> Element {
    // Auto-scroll effect
    use_effect(move || {
        if should_scroll_bottom {
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let js = r#"document.getElementById('chat-scroll-anchor')?.scrollIntoView({behavior:'smooth'})"#;
                document::eval(js);
            });
        }
    });

    rsx! {
        div {
            class: "chat-messages",

            if status == ChatStatus::Loading {
                div {
                    class: "chat-loading",
                    "Loading messages..."
                }
            }

            if messages.is_empty() && status != ChatStatus::Loading {
                div {
                    class: "panel-empty",
                    "No messages yet. Send the first message!"
                }
            }

            for (i, msg) in messages.iter().enumerate() {
                {
                    let is_grouped = if i > 0 {
                        let prev = &messages[i - 1];
                        prev.author_id == msg.author_id
                            && msg.timestamp_millis.saturating_sub(prev.timestamp_millis) < 120_000
                    } else {
                        false
                    };

                    let picker_open = reaction_picker_msg_id.as_deref() == Some(&msg.id);

                    rsx! {
                        ChatBubble {
                            key: "{msg.id}",
                            msg: msg.clone(),
                            is_grouped,
                            on_reply: on_reply,
                            on_react: on_react,
                            on_edit_start: on_edit_start,
                            on_edit_save: on_edit_save,
                            on_edit_cancel: on_edit_cancel,
                            on_delete: on_delete,
                            editing_id: editing_id.clone(),
                            edit_draft: edit_draft.clone(),
                            on_edit_draft_change: on_edit_draft_change,
                            on_reaction_picker_toggle: on_reaction_picker_toggle,
                            reaction_picker_open: picker_open,
                        }
                    }
                }
            }

            // Typing indicator
            if !typing_peers.is_empty() {
                div {
                    class: "typing-indicator",
                    for peer in typing_peers.iter() {
                        span {
                            class: "typing-name {peer.color_class}",
                            "{peer.name}"
                        }
                    }
                    span { class: "typing-text",
                        if typing_peers.len() == 1 { " is typing" } else { " are typing" }
                    }
                    span { class: "typing-dots", "..." }
                }
            }

            // Scroll anchor
            div { id: "chat-scroll-anchor" }
        }
    }
}
