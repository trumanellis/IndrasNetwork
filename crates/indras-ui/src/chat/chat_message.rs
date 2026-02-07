//! Single chat message renderer with edit/delete support.

use dioxus::prelude::*;

use crate::identity::member_color_class;
use super::chat_state::{ChatMessageView, ChatViewType};

/// Props for a single chat message item.
#[component]
pub fn ChatMessageItem(
    msg: ChatMessageView,
    is_grouped: bool,
    on_edit_start: EventHandler<(String, String)>,
    on_edit_save: EventHandler<(String, String)>,
    on_edit_cancel: EventHandler<()>,
    on_delete: EventHandler<String>,
    editing_id: Option<String>,
    edit_draft: String,
    on_edit_draft_change: EventHandler<String>,
) -> Element {
    let color_class = member_color_class(&msg.author_id);
    let is_editing = editing_id.as_deref() == Some(&msg.id);
    let msg_id = msg.id.clone();
    let msg_id_for_delete = msg.id.clone();
    let msg_id_for_save = msg.id.clone();
    let content_for_edit = msg.content.clone();

    match &msg.message_type {
        ChatViewType::Text => {
            rsx! {
                div {
                    key: "{msg.id}",
                    class: if is_grouped { "chat-message text-message chat-message-grouped" } else { "chat-message text-message" },

                    div {
                        class: "chat-message-row",

                        if !is_grouped {
                            span { class: "chat-tick", "{msg.timestamp_display}" }
                            span {
                                class: "chat-sender {color_class}",
                                "{msg.author_name}"
                            }
                        }

                        if is_editing {
                            div {
                                class: "chat-edit-form",
                                textarea {
                                    class: "chat-edit-input",
                                    value: "{edit_draft}",
                                    rows: "2",
                                    oninput: move |evt| {
                                        on_edit_draft_change.call(evt.value());
                                    },
                                    onkeydown: {
                                        let msg_id_save = msg_id_for_save.clone();
                                        let edit_draft_val = edit_draft.clone();
                                        move |evt: KeyboardEvent| {
                                            if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                                evt.prevent_default();
                                                on_edit_save.call((msg_id_save.clone(), edit_draft_val.clone()));
                                            } else if evt.key() == Key::Escape {
                                                on_edit_cancel.call(());
                                            }
                                        }
                                    },
                                }
                                div {
                                    class: "chat-edit-actions",
                                    button {
                                        class: "chat-edit-save",
                                        onclick: {
                                            let msg_id_s = msg_id.clone();
                                            let draft = edit_draft.clone();
                                            move |_| {
                                                on_edit_save.call((msg_id_s.clone(), draft.clone()));
                                            }
                                        },
                                        "Save"
                                    }
                                    button {
                                        class: "chat-edit-cancel",
                                        onclick: move |_| {
                                            on_edit_cancel.call(());
                                        },
                                        "Cancel"
                                    }
                                }
                            }
                        } else {
                            span { class: "chat-content", "{msg.content}" }

                            if msg.is_edited {
                                span {
                                    class: "chat-edited-indicator",
                                    title: "Edited ({msg.version_count} versions)",
                                    "(edited)"
                                }
                            }

                            if msg.is_me {
                                div {
                                    class: "chat-message-actions",
                                    button {
                                        class: "chat-edit-btn",
                                        onclick: move |_| {
                                            on_edit_start.call((msg_id.clone(), content_for_edit.clone()));
                                        },
                                        "\u{270e}"
                                    }
                                    button {
                                        class: "chat-delete-btn",
                                        onclick: move |_| {
                                            on_delete.call(msg_id_for_delete.clone());
                                        },
                                        "\u{2717}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        ChatViewType::System => {
            rsx! {
                div {
                    key: "{msg.id}",
                    class: "chat-message system-message",
                    span { class: "chat-content", "{msg.content}" }
                }
            }
        }

        ChatViewType::Image { data_url, alt_text, .. } => {
            let alt = alt_text
                .clone()
                .unwrap_or_else(|| "Image".to_string());
            rsx! {
                div {
                    key: "{msg.id}",
                    class: "chat-message image-message",

                    div {
                        class: "chat-message-header",
                        span { class: "chat-tick", "{msg.timestamp_display}" }
                        span {
                            class: "chat-sender {color_class}",
                            "{msg.author_name}"
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

        ChatViewType::Gallery { title, item_count } => {
            let title_str = title
                .clone()
                .unwrap_or_else(|| "Gallery".to_string());
            rsx! {
                div {
                    key: "{msg.id}",
                    class: "chat-message gallery-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{msg.timestamp_display}" }
                        span {
                            class: "chat-sender {color_class}",
                            "{msg.author_name}"
                        }
                        span { class: "chat-icon", "\u{1f5bc}" }
                        span { class: "gallery-title", "{title_str}" }
                        span { class: "gallery-count", "({item_count} items)" }
                    }
                }
            }
        }

        ChatViewType::ProofSubmitted { quest_id } => {
            let qid_short = &quest_id[..8.min(quest_id.len())];
            rsx! {
                div {
                    key: "{msg.id}",
                    class: "chat-message proof-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{msg.timestamp_display}" }
                        span { class: "chat-icon", "\u{1f4ce}" }
                        span { class: "chat-content", "Proof submitted for quest {qid_short}" }
                    }
                }
            }
        }

        ChatViewType::BlessingGiven { claimant, .. } => {
            rsx! {
                div {
                    key: "{msg.id}",
                    class: "chat-message blessing-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{msg.timestamp_display}" }
                        span { class: "chat-icon", "\u{2728}" }
                        span { class: "chat-content", "Blessing given to {claimant}" }
                    }
                }
            }
        }

        ChatViewType::ArtifactRecalled => {
            rsx! {
                div {
                    key: "{msg.id}",
                    class: "chat-message system-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{msg.timestamp_display}" }
                        span { class: "chat-content", "{msg.content}" }
                    }
                }
            }
        }

        ChatViewType::Deleted => {
            rsx! {
                div {
                    key: "{msg.id}",
                    class: "chat-message deleted-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{msg.timestamp_display}" }
                        span { class: "chat-content chat-deleted-content", "This message was deleted" }
                    }
                }
            }
        }
    }
}
