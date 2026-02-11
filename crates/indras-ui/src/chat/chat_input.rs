//! Chat input with textarea and action menu.

use dioxus::prelude::*;

use super::chat_state::ChatStatus;

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
                    onclick: move |_| {
                        // Error will be cleared by parent
                    },
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
                    // Backdrop to close menu on outside click
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
                placeholder: "Type a message...",
                value: "{draft}",
                rows: "1",
                oninput: move |evt| {
                    on_draft_change.call(evt.value());
                },
                onkeydown: {
                    let draft_clone = draft.clone();
                    move |evt: KeyboardEvent| {
                        if evt.key() == Key::Enter && !evt.modifiers().shift() {
                            evt.prevent_default();
                            let text = draft_clone.clone();
                            if !text.trim().is_empty() {
                                on_send.call(text);
                            }
                        }
                    }
                },
            }

            button {
                class: "chat-send-btn",
                disabled: draft_empty || is_sending,
                onclick: {
                    let draft_clone = draft.clone();
                    move |_| {
                        if !draft_clone.trim().is_empty() {
                            on_send.call(draft_clone.clone());
                        }
                    }
                },
                "Send"
            }
        }
    }
}
