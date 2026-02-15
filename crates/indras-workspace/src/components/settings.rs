//! Settings view with identity display, connect, PassStory trigger, and theme switcher.

use dioxus::prelude::*;
use indras_ui::ThemeSwitcher;
use crate::bridge::network_bridge::NetworkHandle;

#[component]
pub fn SettingsView(
    player_name: String,
    player_letter: String,
    player_short_id: String,
    identity_uri: Option<String>,
    network_handle: Signal<Option<NetworkHandle>>,
    on_open_pass_story: EventHandler<()>,
) -> Element {
    let mut connect_input = use_signal(String::new);
    let mut connect_status = use_signal(|| None::<String>);
    let mut copied = use_signal(|| false);

    rsx! {
        div {
            class: "view active",
            div {
                class: "content-scroll",
                div {
                    class: "content-body",
                    div { class: "doc-title", "Settings" }

                    // Identity section
                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Identity" }
                        div {
                            class: "settings-identity",
                            div {
                                class: "settings-identity-avatar",
                                "{player_letter}"
                            }
                            div {
                                class: "settings-identity-info",
                                div { class: "settings-identity-name", "{player_name}" }
                                div { class: "settings-identity-id", "{player_short_id}" }
                            }
                        }
                    }

                    // Share Identity section
                    if let Some(ref uri) = identity_uri {
                        div {
                            class: "settings-section",
                            div { class: "settings-section-title", "Share Identity" }
                            div {
                                class: "settings-uri-display",
                                code { class: "settings-uri-code", "{uri}" }
                                button {
                                    class: "settings-copy-btn",
                                    onclick: move |_| {
                                        copied.set(true);
                                        // Reset after 2 seconds
                                        spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                            copied.set(false);
                                        });
                                    },
                                    if *copied.read() { "Copied" } else { "Copy" }
                                }
                            }
                        }
                    }

                    // Connect section
                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Connect" }
                        div {
                            class: "settings-connect-row",
                            input {
                                class: "settings-connect-input",
                                placeholder: "Paste identity URI...",
                                value: "{connect_input}",
                                oninput: move |evt| connect_input.set(evt.value()),
                            }
                            button {
                                class: "settings-connect-btn",
                                disabled: connect_input.read().trim().is_empty(),
                                onclick: move |_| {
                                    let code = connect_input.read().trim().to_string();
                                    if code.is_empty() {
                                        return;
                                    }
                                    let nh_signal = network_handle;
                                    spawn(async move {
                                        let nh = nh_signal.read().clone();
                                        if let Some(nh) = nh {
                                            match nh.network.connect_by_code(&code).await {
                                                Ok(_) => connect_status.set(Some("Connected!".into())),
                                                Err(e) => connect_status.set(Some(format!("Error: {}", e))),
                                            }
                                        }
                                    });
                                },
                                "Connect"
                            }
                        }
                        if let Some(ref status) = *connect_status.read() {
                            div { class: "settings-connect-status", "{status}" }
                        }
                    }

                    // Security section
                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Security" }
                        button {
                            class: "settings-action-btn",
                            onclick: move |_| on_open_pass_story.call(()),
                            "Protect Identity with PassStory"
                        }
                    }

                    // Appearance section
                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Appearance" }
                        ThemeSwitcher {}
                    }
                }
            }
        }
    }
}
