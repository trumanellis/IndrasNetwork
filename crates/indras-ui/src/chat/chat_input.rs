//! Chat input with textarea and action menu.

use dioxus::prelude::*;

use super::chat_state::{ChatStatus, ReplyPreview};

/// Chat input component with textarea, action menu, and send button.
#[component]
pub fn ChatInput(
    draft: String,
    status: ChatStatus,
    error: Option<String>,
    on_send: EventHandler<String>,
    on_draft_change: EventHandler<String>,
    action_menu_open: bool,
    on_action_toggle: EventHandler<()>,
    on_action_close: EventHandler<()>,
    replying_to: Option<ReplyPreview>,
    on_send_reply: EventHandler<(String, String)>,
    on_cancel_reply: EventHandler<()>,
    emoji_picker_open: bool,
    on_emoji_toggle: EventHandler<()>,
    on_emoji_select: EventHandler<String>,
) -> Element {
    let draft_empty = draft.trim().is_empty();
    let is_sending = status == ChatStatus::Sending;

    rsx! {
        // Error toast
        if let Some(ref err) = error {
            div {
                class: "chat-error-toast",
                span { "{err}" }
                button {
                    class: "chat-error-dismiss",
                    onclick: move |_| {},
                    "\u{2717}"
                }
            }
        }

        // Sending indicator
        if is_sending {
            div {
                class: "chat-sending-indicator",
                "Sending..."
            }
        }

        // Reply compose bar
        if let Some(ref reply) = replying_to {
            div {
                class: "reply-compose-bar",
                div {
                    class: "reply-compose-preview",
                    div {
                        class: "reply-compose-author {reply.author_color_class}",
                        "{reply.author_name}"
                    }
                    div {
                        class: "reply-compose-snippet",
                        "{reply.content_snippet}"
                    }
                }
                button {
                    class: "reply-compose-cancel",
                    onclick: move |_| on_cancel_reply.call(()),
                    "\u{2717}"
                }
            }
        }

        div {
            class: "chat-input-container",

            div {
                class: "chat-input-wrapper",

                button {
                    class: "chat-action-btn",
                    onclick: move |_| {
                        on_action_toggle.call(());
                    },
                    "+"
                }

                if action_menu_open {
                    div {
                        class: "chat-action-backdrop",
                        onclick: move |_| {
                            on_action_close.call(());
                        },
                    }

                    div {
                        class: "chat-action-menu",

                        button {
                            class: "action-menu-item",
                            onclick: move |_| {
                                on_action_close.call(());
                            },
                            "\u{1f4ce} Artifact"
                        }
                        button {
                            class: "action-menu-item",
                            onclick: move |_| {
                                on_action_close.call(());
                            },
                            "\u{1f4c4} Document"
                        }
                        button {
                            class: "action-menu-item",
                            onclick: move |_| {
                                on_action_close.call(());
                            },
                            "\u{2713} Proof of Service"
                        }
                    }
                }
            }

            textarea {
                class: "chat-input chat-textarea",
                placeholder: if replying_to.is_some() { "Reply..." } else { "Type a message..." },
                value: "{draft}",
                rows: "1",
                oninput: move |evt| {
                    on_draft_change.call(evt.value());
                },
                onkeydown: {
                    let draft_clone = draft.clone();
                    let has_reply = replying_to.is_some();
                    let reply_id = replying_to.as_ref().map(|r| r.original_id.clone());
                    move |evt: KeyboardEvent| {
                        if evt.key() == Key::Enter && !evt.modifiers().shift() {
                            evt.prevent_default();
                            let text = draft_clone.clone();
                            if !text.trim().is_empty() {
                                if has_reply {
                                    if let Some(ref rid) = reply_id {
                                        on_send_reply.call((text, rid.clone()));
                                    }
                                } else {
                                    on_send.call(text);
                                }
                            }
                        }
                    }
                },
            }

            // Emoji picker button
            button {
                class: if emoji_picker_open { "chat-emoji-btn chat-emoji-btn-active" } else { "chat-emoji-btn" },
                onclick: move |_| on_emoji_toggle.call(()),
                "\u{263a}"
            }

            // Emoji picker popover
            if emoji_picker_open {
                div {
                    class: "emoji-picker",
                    {["👍", "❤️", "😂", "😮", "😢", "🙏", "🎉", "🔥"].iter().map(|emoji| {
                        let e = emoji.to_string();
                        rsx! {
                            button {
                                key: "{emoji}",
                                class: "emoji-picker-btn",
                                onclick: move |_| on_emoji_select.call(e.clone()),
                                "{emoji}"
                            }
                        }
                    })}
                }
            }

            button {
                class: "chat-send-btn",
                disabled: draft_empty || is_sending,
                onclick: {
                    let draft_clone = draft.clone();
                    let has_reply = replying_to.is_some();
                    let reply_id = replying_to.as_ref().map(|r| r.original_id.clone());
                    move |_| {
                        if !draft_clone.trim().is_empty() {
                            if has_reply {
                                if let Some(ref rid) = reply_id {
                                    on_send_reply.call((draft_clone.clone(), rid.clone()));
                                }
                            } else {
                                on_send.call(draft_clone.clone());
                            }
                        }
                    }
                },
                "Send"
            }
        }
    }
}
