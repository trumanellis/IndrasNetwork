//! Chat message list with auto-scroll and message grouping.

use dioxus::prelude::*;

use super::chat_message::ChatMessageItem;
use super::chat_state::{ChatMessageView, ChatStatus};

/// Message list component with auto-scroll and grouping.
#[component]
pub fn ChatMessageList(
    messages: Vec<ChatMessageView>,
    status: ChatStatus,
    on_edit_start: EventHandler<(String, String)>,
    on_edit_save: EventHandler<(String, String)>,
    on_edit_cancel: EventHandler<()>,
    on_delete: EventHandler<String>,
    editing_id: Option<String>,
    edit_draft: String,
    on_edit_draft_change: EventHandler<String>,
    should_scroll_bottom: bool,
) -> Element {
    // Auto-scroll effect
    use_effect(move || {
        if should_scroll_bottom {
            spawn(async move {
                // Small delay to let DOM update
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
                    // Message grouping: same author within 2 minutes
                    let is_grouped = if i > 0 {
                        let prev = &messages[i - 1];
                        prev.author_id == msg.author_id
                            && msg.timestamp_millis.saturating_sub(prev.timestamp_millis) < 120_000
                    } else {
                        false
                    };

                    rsx! {
                        ChatMessageItem {
                            key: "{msg.id}",
                            msg: msg.clone(),
                            is_grouped,
                            on_edit_start: on_edit_start,
                            on_edit_save: on_edit_save,
                            on_edit_cancel: on_edit_cancel,
                            on_delete: on_delete,
                            editing_id: editing_id.clone(),
                            edit_draft: edit_draft.clone(),
                            on_edit_draft_change: on_edit_draft_change,
                        }
                    }
                }
            }

            // Scroll anchor
            div { id: "chat-scroll-anchor" }
        }
    }
}
