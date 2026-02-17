//! Telegram-style chat bubble component.
//!
//! Renders messages as left/right aligned bubbles with avatar,
//! sender name, reply preview, reactions, and hover actions.

use dioxus::prelude::*;

use super::chat_state::{
    ChatMessageView, ChatViewType, DeliveryStatus, ReactionView, ReplyPreview,
};

/// Telegram-style message bubble.
#[component]
pub fn ChatBubble(
    msg: ChatMessageView,
    is_grouped: bool,
    on_reply: EventHandler<String>,
    on_react: EventHandler<(String, String)>,
    on_edit_start: EventHandler<(String, String)>,
    on_edit_save: EventHandler<(String, String)>,
    on_edit_cancel: EventHandler<()>,
    on_delete: EventHandler<String>,
    editing_id: Option<String>,
    edit_draft: String,
    on_edit_draft_change: EventHandler<String>,
    on_reaction_picker_toggle: EventHandler<String>,
    reaction_picker_open: bool,
) -> Element {
    let is_editing = editing_id.as_deref() == Some(&msg.id);
    let row_class = if msg.is_me { "chat-bubble-row bubble-right" } else { "chat-bubble-row bubble-left" };
    let bubble_class = if msg.is_me { "chat-bubble chat-bubble-sent" } else { "chat-bubble chat-bubble-received" };

    // System messages render centered, not as bubbles
    if matches!(msg.message_type, ChatViewType::System) {
        return rsx! {
            div {
                class: "chat-bubble-row bubble-center",
                div {
                    class: "chat-system-msg",
                    "{msg.content}"
                }
            }
        };
    }

    // Deleted messages
    if matches!(msg.message_type, ChatViewType::Deleted) {
        return rsx! {
            div {
                class: "{row_class}",
                div {
                    class: "{bubble_class} bubble-deleted",
                    span { class: "bubble-deleted-text", "This message was deleted" }
                }
            }
        };
    }

    let msg_id = msg.id.clone();
    let msg_id_reply = msg.id.clone();
    let msg_id_react = msg.id.clone();
    let msg_id_delete = msg.id.clone();
    let msg_id_edit = msg.id.clone();
    let msg_id_save = msg.id.clone();
    let content_for_edit = msg.content.clone();

    rsx! {
        div {
            key: "{msg.id}",
            class: "{row_class}",

            // Avatar (left side, received messages, not grouped)
            if !msg.is_me && !is_grouped {
                div {
                    class: "bubble-avatar {msg.author_color_class}",
                    "{msg.author_letter}"
                }
            }
            // Spacer for grouped messages (keeps alignment)
            if !msg.is_me && is_grouped {
                div { class: "bubble-avatar-spacer" }
            }

            div {
                class: "{bubble_class}",

                // Hover action bar
                div {
                    class: "bubble-actions",
                    button {
                        class: "bubble-action-btn",
                        title: "Reply",
                        onclick: move |_| on_reply.call(msg_id_reply.clone()),
                        "\u{21a9}"
                    }
                    button {
                        class: "bubble-action-btn",
                        title: "React",
                        onclick: move |_| on_reaction_picker_toggle.call(msg_id_react.clone()),
                        "\u{263a}"
                    }
                    if msg.is_me {
                        button {
                            class: "bubble-action-btn",
                            title: "Edit",
                            onclick: move |_| on_edit_start.call((msg_id_edit.clone(), content_for_edit.clone())),
                            "\u{270e}"
                        }
                        button {
                            class: "bubble-action-btn bubble-action-delete",
                            title: "Delete",
                            onclick: move |_| on_delete.call(msg_id_delete.clone()),
                            "\u{2717}"
                        }
                    }
                }

                // Sender name (first in group only, received messages)
                if !msg.is_me && !is_grouped {
                    div {
                        class: "bubble-sender {msg.author_color_class}",
                        "{msg.author_name}"
                    }
                }

                // Reply preview bar
                if let Some(ref preview) = msg.reply_preview {
                    ReplyPreviewBar {
                        preview: preview.clone(),
                        on_scroll_to: move |id: String| {
                            // Scroll to the original message
                            let js = format!(
                                "document.querySelector('[data-msg-id=\"{}\"]')?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                                id
                            );
                            document::eval(&js);
                        },
                    }
                }

                // Content area
                if is_editing {
                    div {
                        class: "bubble-edit-form",
                        textarea {
                            class: "bubble-edit-input",
                            value: "{edit_draft}",
                            rows: "2",
                            oninput: move |evt| on_edit_draft_change.call(evt.value()),
                            onkeydown: {
                                let save_id = msg_id_save.clone();
                                let draft = edit_draft.clone();
                                move |evt: KeyboardEvent| {
                                    if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                        evt.prevent_default();
                                        on_edit_save.call((save_id.clone(), draft.clone()));
                                    } else if evt.key() == Key::Escape {
                                        on_edit_cancel.call(());
                                    }
                                }
                            },
                        }
                        div {
                            class: "bubble-edit-actions",
                            button {
                                class: "bubble-edit-save",
                                onclick: {
                                    let save_id = msg_id.clone();
                                    let draft = edit_draft.clone();
                                    move |_| on_edit_save.call((save_id.clone(), draft.clone()))
                                },
                                "Save"
                            }
                            button {
                                class: "bubble-edit-cancel",
                                onclick: move |_| on_edit_cancel.call(()),
                                "Cancel"
                            }
                        }
                    }
                } else {
                    {render_content(&msg)}
                }

                // Footer: timestamp + edited + delivery status
                div {
                    class: "bubble-footer",
                    span { class: "bubble-time", "{msg.timestamp_display}" }
                    if msg.is_edited {
                        span { class: "bubble-edited", "edited" }
                    }
                    if msg.is_me {
                        span {
                            class: "bubble-delivery",
                            {match msg.delivery_status {
                                DeliveryStatus::Read => "\u{2713}\u{2713}",
                                DeliveryStatus::Sent => "\u{2713}",
                                DeliveryStatus::None => "",
                            }}
                        }
                    }
                }
            }

            // Reaction pills below bubble
            if !msg.reactions.is_empty() {
                div {
                    class: "bubble-reactions",
                    for reaction in msg.reactions.iter() {
                        ReactionPill {
                            reaction: reaction.clone(),
                            msg_id: msg.id.clone(),
                            on_react: on_react,
                        }
                    }
                }
            }

            // Reaction picker popup
            if reaction_picker_open {
                div {
                    class: "bubble-reaction-picker",
                    {["👍", "❤️", "😂", "😮", "😢", "🙏", "🎉", "🔥"].iter().map(|emoji| {
                        let emoji_str = emoji.to_string();
                        let mid = msg.id.clone();
                        rsx! {
                            button {
                                key: "{emoji}",
                                class: "reaction-picker-btn",
                                onclick: move |_| {
                                    on_react.call((mid.clone(), emoji_str.clone()));
                                },
                                "{emoji}"
                            }
                        }
                    })}
                }
            }
        }
    }
}

/// Render message content based on type.
fn render_content(msg: &ChatMessageView) -> Element {
    match &msg.message_type {
        ChatViewType::Text => rsx! {
            div { class: "bubble-content", "{msg.content}" }
        },
        ChatViewType::Image { data_url, alt_text, .. } => {
            let alt = alt_text.clone().unwrap_or_else(|| "Image".to_string());
            rsx! {
                div { class: "bubble-content" ,
                    if let Some(url) = data_url {
                        div {
                            class: "bubble-image-container",
                            img {
                                class: "bubble-inline-image",
                                src: "{url}",
                                alt: "{alt}",
                            }
                        }
                    } else {
                        div {
                            class: "bubble-image-placeholder",
                            "\u{1f5bc} {alt}"
                        }
                    }
                }
            }
        }
        ChatViewType::Gallery { title, item_count } => {
            let title_str = title.clone().unwrap_or_else(|| "Gallery".to_string());
            rsx! {
                div { class: "bubble-content bubble-gallery",
                    span { class: "bubble-gallery-icon", "\u{1f5bc}" }
                    span { class: "bubble-gallery-title", "{title_str}" }
                    span { class: "bubble-gallery-count", "({item_count} items)" }
                }
            }
        }
        ChatViewType::ProofSubmitted { quest_id } => {
            let qid_short = &quest_id[..8.min(quest_id.len())];
            rsx! {
                div { class: "bubble-content bubble-proof",
                    "\u{1f4ce} Proof submitted for quest {qid_short}"
                }
            }
        }
        ChatViewType::BlessingGiven { claimant, .. } => rsx! {
            div { class: "bubble-content bubble-blessing",
                "\u{2728} Blessing given to {claimant}"
            }
        },
        ChatViewType::ArtifactRecalled => rsx! {
            div { class: "bubble-content", "{msg.content}" }
        },
        // System and Deleted handled at the top
        _ => rsx! {
            div { class: "bubble-content", "{msg.content}" }
        },
    }
}

/// Reply preview bar inside a bubble.
#[component]
fn ReplyPreviewBar(
    preview: ReplyPreview,
    on_scroll_to: EventHandler<String>,
) -> Element {
    let original_id = preview.original_id.clone();
    rsx! {
        div {
            class: "bubble-reply-preview",
            onclick: move |_| on_scroll_to.call(original_id.clone()),
            div {
                class: "bubble-reply-author {preview.author_color_class}",
                "{preview.author_name}"
            }
            div {
                class: "bubble-reply-snippet",
                "{preview.content_snippet}"
            }
        }
    }
}

/// Single reaction pill.
#[component]
fn ReactionPill(
    reaction: ReactionView,
    msg_id: String,
    on_react: EventHandler<(String, String)>,
) -> Element {
    let pill_class = if reaction.includes_me {
        "reaction-pill reaction-mine"
    } else {
        "reaction-pill"
    };
    let emoji = reaction.emoji.clone();
    let mid = msg_id.clone();
    let tooltip = reaction.author_names.join(", ");

    rsx! {
        button {
            class: "{pill_class}",
            title: "{tooltip}",
            onclick: move |_| on_react.call((mid.clone(), emoji.clone())),
            span { class: "reaction-emoji", "{reaction.emoji}" }
            span { class: "reaction-count", "{reaction.count}" }
        }
    }
}
