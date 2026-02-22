//! Add contact via identity code.

use dioxus::prelude::*;
use crate::state::ChatContext;

/// Contact add modal component.
#[component]
pub fn ContactAdd(on_close: EventHandler<()>) -> Element {
    let ctx = use_context::<ChatContext>();
    let handle = ctx.handle.read().clone();
    let mut peer_code = use_signal(String::new);
    let mut status = use_signal(|| None::<String>);
    let mut connecting = use_signal(|| false);
    let my_uri = handle.network.identity_uri();

    rsx! {
        div { class: "contact-add-overlay",
            div { class: "contact-add-modal",
                div { class: "contact-add-header",
                    h3 { "Add Contact" }
                    button {
                        class: "close-button",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                div { class: "contact-add-section",
                    h4 { "Your Identity Code" }
                    div { class: "identity-code", "{my_uri}" }
                }

                div { class: "contact-add-section",
                    h4 { "Enter Peer's Code" }
                    input {
                        class: "contact-input",
                        placeholder: "Paste identity code (indra1...)...",
                        value: "{peer_code}",
                        oninput: move |evt| peer_code.set(evt.value()),
                    }

                    if let Some(ref s) = *status.read() {
                        div { class: "contact-status", "{s}" }
                    }

                    button {
                        class: "setup-button",
                        disabled: peer_code.read().trim().is_empty() || *connecting.read(),
                        onclick: {
                            let handle = handle.clone();
                            move |_| {
                                let code = peer_code.read().trim().to_string();
                                if code.is_empty() {
                                    return;
                                }
                                let handle = handle.clone();
                                connecting.set(true);
                                status.set(Some("Connecting...".to_string()));
                                spawn(async move {
                                    match handle.network.connect_by_code(&code).await {
                                        Ok(_realm) => {
                                            status.set(Some("Connected! You can close this dialog.".to_string()));
                                            connecting.set(false);
                                        }
                                        Err(e) => {
                                            status.set(Some(format!("Error: {}", e)));
                                            connecting.set(false);
                                        }
                                    }
                                });
                            }
                        },
                        if *connecting.read() { "Connecting..." } else { "Connect" }
                    }
                }
            }
        }
    }
}
