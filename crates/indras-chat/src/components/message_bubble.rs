//! Individual message bubble with delivery status badges.

use dioxus::prelude::*;

/// Delivery status of a message.
#[derive(Clone, Debug, PartialEq)]
pub enum DeliveryStatus {
    Sending,
    Sent,
    Delivered,
    Read,
}

/// Message bubble component.
#[component]
pub fn MessageBubble(
    content: String,
    author: String,
    is_mine: bool,
    timestamp: u64,
    status: DeliveryStatus,
    #[props(default = false)]
    is_edited: bool,
    #[props(default)]
    reply_preview: Option<(String, String)>,
    #[props(default)]
    reactions: Vec<(String, usize)>,
) -> Element {
    let bubble_class = if is_mine { "message-bubble mine" } else { "message-bubble theirs" };
    let status_icon = match status {
        DeliveryStatus::Sending => "🕐",
        DeliveryStatus::Sent => "✓",
        DeliveryStatus::Delivered => "✓✓",
        DeliveryStatus::Read => "✓✓",
    };
    let status_class = if status == DeliveryStatus::Read { "status-read" } else { "status" };
    let time_display = format!("#{}", timestamp);

    rsx! {
        div { class: "{bubble_class}",
            // Reply preview bar
            if let Some((reply_author, reply_text)) = &reply_preview {
                div { class: "message-reply-bar",
                    span { class: "message-author", "{reply_author}" }
                    " {reply_text}"
                }
            }

            // Author name (only for others' messages)
            if !is_mine {
                div { class: "message-author", "{author}" }
            }

            // Message content
            div { class: "message-content", "{content}" }

            // Reactions
            if !reactions.is_empty() {
                div { class: "message-reactions",
                    for (emoji, count) in reactions.iter() {
                        div { class: "reaction-pill", "{emoji} {count}" }
                    }
                }
            }

            // Meta: time + edited + status
            div { class: "message-meta",
                if is_edited {
                    span { class: "message-edited", "edited" }
                }
                span { class: "message-time", "{time_display}" }
                if is_mine {
                    span { class: "{status_class}", "{status_icon}" }
                }
            }
        }
    }
}
