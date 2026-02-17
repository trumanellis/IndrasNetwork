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
    let mut encounter_code = use_signal(|| None::<String>);
    let mut encounter_status = use_signal(|| None::<String>);
    let mut encounter_input = use_signal(String::new);
    let mut export_status = use_signal(|| None::<String>);

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

                    // Quick Connect (Encounter)
                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Quick Connect" }
                        div {
                            class: "settings-connect-row",
                            button {
                                class: "settings-action-btn",
                                onclick: move |_| {
                                    let nh_signal = network_handle;
                                    spawn(async move {
                                        let nh = nh_signal.read().clone();
                                        if let Some(nh) = nh {
                                            match nh.network.create_encounter().await {
                                                Ok((code, _handle)) => {
                                                    encounter_code.set(Some(code));
                                                    encounter_status.set(Some("Share this code with your peer".into()));
                                                }
                                                Err(e) => {
                                                    encounter_status.set(Some(format!("Error: {}", e)));
                                                }
                                            }
                                        }
                                    });
                                },
                                "Create Code"
                            }
                        }
                        if let Some(ref code) = *encounter_code.read() {
                            div {
                                class: "settings-uri-display",
                                code { class: "settings-uri-code", style: "font-size: 1.5rem; letter-spacing: 0.2em;", "{code}" }
                            }
                        }
                        div {
                            class: "settings-connect-row",
                            input {
                                class: "settings-connect-input",
                                placeholder: "Enter 6-digit code...",
                                maxlength: "6",
                                value: "{encounter_input}",
                                oninput: move |evt| encounter_input.set(evt.value()),
                            }
                            button {
                                class: "settings-connect-btn",
                                disabled: encounter_input.read().trim().len() < 6,
                                onclick: move |_| {
                                    let code = encounter_input.read().trim().to_string();
                                    if code.len() < 6 {
                                        return;
                                    }
                                    let nh_signal = network_handle;
                                    spawn(async move {
                                        let nh = nh_signal.read().clone();
                                        if let Some(nh) = nh {
                                            match nh.network.join_encounter(&code).await {
                                                Ok(peer_id) => {
                                                    let short = peer_id.iter().take(4).map(|b| format!("{:02x}", b)).collect::<String>();
                                                    encounter_status.set(Some(format!("Connected to peer {}", short)));
                                                    encounter_input.set(String::new());
                                                }
                                                Err(e) => {
                                                    encounter_status.set(Some(format!("Error: {}", e)));
                                                }
                                            }
                                        }
                                    });
                                },
                                "Join"
                            }
                        }
                        if let Some(ref status) = *encounter_status.read() {
                            div { class: "settings-connect-status", "{status}" }
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

                    // Identity Backup section
                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Identity Backup" }
                        div {
                            class: "settings-connect-row",
                            button {
                                class: "settings-action-btn",
                                onclick: move |_| {
                                    let nh_signal = network_handle;
                                    spawn(async move {
                                        let nh = nh_signal.read().clone();
                                        if let Some(nh) = nh {
                                            match nh.network.export_identity().await {
                                                Ok(backup) => {
                                                    export_status.set(Some(format!("{} bytes exported", backup.len())));
                                                }
                                                Err(e) => {
                                                    export_status.set(Some(format!("Export failed: {}", e)));
                                                }
                                            }
                                        }
                                    });
                                },
                                "Export Identity"
                            }
                        }
                        if let Some(ref status) = *export_status.read() {
                            div { class: "settings-connect-status", "{status}" }
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
