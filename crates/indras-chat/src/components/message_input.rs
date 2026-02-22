//! Message compose bar with send button.

use dioxus::prelude::*;

/// Message input component.
#[component]
pub fn MessageInput(
    on_send: EventHandler<String>,
    #[props(default)]
    on_typing: Option<EventHandler<()>>,
) -> Element {
    let mut text = use_signal(String::new);

    let can_send = !text.read().trim().is_empty();

    rsx! {
        div { class: "message-input-bar",
            textarea {
                class: "message-input",
                placeholder: "Write a message...",
                value: "{text}",
                oninput: move |evt| {
                    text.set(evt.value());
                    if let Some(ref handler) = on_typing {
                        handler.call(());
                    }
                },
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Enter && !evt.modifiers().shift() && can_send {
                        evt.prevent_default();
                        let msg = text.read().trim().to_string();
                        text.set(String::new());
                        on_send.call(msg);
                    }
                },
            }
            button {
                class: "send-button",
                disabled: !can_send,
                onclick: move |_| {
                    if can_send {
                        let msg = text.read().trim().to_string();
                        text.set(String::new());
                        on_send.call(msg);
                    }
                },
                "➤"
            }
        }
    }
}
