//! Contact invite overlay — share identity URI, connect by URI, and encounter codes.
//!
//! Adapted from `indras-gift-cycle/src/components/contact_invite.rs`.
//! Takes `Arc<IndrasNetwork>` directly instead of `GiftCycleBridge`.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{IdentityCode, IndrasNetwork};

/// Overlay for sharing and accepting contact invite links.
///
/// Three sections: Share Your Link, Connect with Someone, Quick Connect (encounter codes).
///
/// Takes `Signal<Option<Arc<IndrasNetwork>>>` to avoid `PartialEq` requirement on `Arc<IndrasNetwork>`.
#[component]
pub fn ContactInviteOverlay(
    network: Signal<Option<Arc<IndrasNetwork>>>,
    player_name: String,
    member_id: [u8; 32],
    mut is_open: Signal<bool>,
) -> Element {
    let Some(network) = network.read().clone() else {
        return rsx! {};
    };
    let identity_uri = network.identity_uri();
    let member_hex: String = member_id.iter().take(4).map(|b| format!("{b:02x}")).collect();

    // State
    let mut copied = use_signal(|| false);
    let mut connect_input = use_signal(String::new);
    let mut connect_status = use_signal(|| None::<String>);
    let mut parsed_inviter_name = use_signal(|| None::<String>);

    let mut encounter_code = use_signal(|| None::<String>);
    let mut encounter_status = use_signal(|| None::<String>);
    let mut join_input = use_signal(String::new);

    if !is_open() {
        return rsx! {};
    }

    let uri = identity_uri.clone();
    let input_val = connect_input();
    let status = connect_status();
    let parsed_name = parsed_inviter_name();

    // Determine status display
    let status_class = match &status {
        Some(s) if s.starts_with("error:") => Some("contact-invite-status-error"),
        Some(s) if s.starts_with("success:") => Some("contact-invite-status-success"),
        _ => None,
    };
    let status_text = match &status {
        Some(s) if s.starts_with("error:") => Some(s.strip_prefix("error:").unwrap_or(s).to_string()),
        Some(s) if s.starts_with("success:") => Some(s.strip_prefix("success:").unwrap_or(s).to_string()),
        _ => None,
    };

    // Live preview when typing/pasting a URI
    let mut on_parse_input = move |val: String| {
        let trimmed = val.trim().to_string();
        if trimmed.is_empty() {
            parsed_inviter_name.set(None);
            return;
        }
        match IdentityCode::parse_uri(&trimmed) {
            Ok((_code, name)) => parsed_inviter_name.set(name),
            Err(_) => parsed_inviter_name.set(None),
        }
    };

    // Connect by URI — closes overlay after feedback
    let net_connect = network.clone();
    let on_connect = move |_| {
        let uri_val = connect_input();
        if uri_val.trim().is_empty() {
            return;
        }
        connect_status.set(None);
        let net = net_connect.clone();
        spawn(async move {
            match net.connect_by_code(&uri_val).await {
                Ok(_realm) => {
                    connect_status.set(Some("success:Connected!".to_string()));
                    connect_input.set(String::new());
                    parsed_inviter_name.set(None);
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    is_open.set(false);
                    connect_status.set(None);
                }
                Err(e) => {
                    connect_status.set(Some(format!("error:Connection failed: {e}")));
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    is_open.set(false);
                    connect_status.set(None);
                }
            }
        });
    };

    // Copy URI to clipboard — closes overlay after feedback
    let uri_for_copy = identity_uri.clone();
    let on_copy = move |_| {
        let copy_uri = uri_for_copy.clone();
        let js = format!(
            "navigator.clipboard.writeText('{}')",
            copy_uri.replace('\'', "\\'")
        );
        document::eval(&js);
        copied.set(true);
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            copied.set(false);
            is_open.set(false);
        });
    };

    // Create encounter
    let net_create = network.clone();
    let on_create_encounter = move |_| {
        let net = net_create.clone();
        spawn(async move {
            match net.create_encounter().await {
                Ok((code, _handle)) => {
                    encounter_code.set(Some(code));
                    encounter_status.set(None);
                }
                Err(e) => {
                    encounter_status.set(Some(format!("error:{e}")));
                }
            }
        });
    };

    // Join encounter — closes overlay after feedback
    let net_join = network.clone();
    let on_join_encounter = move |_| {
        let code = join_input.read().clone();
        if code.trim().is_empty() {
            return;
        }
        let net = net_join.clone();
        spawn(async move {
            match net.join_encounter(&code).await {
                Ok(_peer_id) => {
                    encounter_status.set(Some("success:Joined! Peer discovered.".to_string()));
                    join_input.set(String::new());
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    is_open.set(false);
                    encounter_status.set(None);
                }
                Err(e) => {
                    encounter_status.set(Some(format!("error:{e}")));
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    is_open.set(false);
                    encounter_status.set(None);
                }
            }
        });
    };

    // Encounter status display
    let enc_status = encounter_status();
    let enc_status_class = match &enc_status {
        Some(s) if s.starts_with("error:") => Some("contact-invite-status-error"),
        Some(s) if s.starts_with("success:") => Some("contact-invite-status-success"),
        _ => None,
    };
    let enc_status_text = match &enc_status {
        Some(s) if s.starts_with("error:") => Some(s.strip_prefix("error:").unwrap_or(s).to_string()),
        Some(s) if s.starts_with("success:") => Some(s.strip_prefix("success:").unwrap_or(s).to_string()),
        _ => None,
    };

    let copy_label = if copied() { "Copied!" } else { "Copy Link" };

    rsx! {
        div {
            class: "contact-invite-overlay",
            onclick: move |_| is_open.set(false),

            div {
                class: "contact-invite-dialog",
                role: "dialog",
                "aria-modal": "true",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "contact-invite-header",
                    h2 { "Connections" }
                    button {
                        class: "contact-invite-close",
                        "aria-label": "Close",
                        onclick: move |_| is_open.set(false),
                        "\u{00d7}"
                    }
                }

                // Content
                div {
                    class: "contact-invite-content",

                    // ── Share section ──
                    section {
                        class: "contact-invite-share",
                        h3 { "Share Your Link" }
                        p {
                            class: "contact-invite-identity",
                            span { class: "contact-invite-name", "{player_name}" }
                            " "
                            span { class: "contact-invite-mid", "{member_hex}\u{2026}" }
                        }
                        div {
                            class: "contact-invite-uri",
                            "{uri}"
                        }
                        button {
                            class: "contact-invite-copy-btn",
                            onclick: on_copy,
                            "{copy_label}"
                        }
                    }

                    // ── Connect section ──
                    section {
                        class: "contact-invite-connect",
                        h3 { "Connect with Someone" }
                        input {
                            class: "contact-invite-input",
                            r#type: "text",
                            placeholder: "Paste an indra1... identity code",
                            "aria-label": "Paste invite link",
                            value: "{input_val}",
                            oninput: move |evt| {
                                let val = evt.value();
                                connect_input.set(val.clone());
                                on_parse_input(val);
                            },
                        }

                        if let Some(ref inviter) = parsed_name {
                            div {
                                class: "contact-invite-preview",
                                "Invite from: {inviter}"
                            }
                        }

                        if let (Some(cls), Some(txt)) = (status_class, &status_text) {
                            div {
                                class: "{cls}",
                                role: "alert",
                                "{txt}"
                            }
                        }

                        button {
                            class: "contact-invite-connect-btn",
                            disabled: input_val.trim().is_empty(),
                            onclick: on_connect,
                            "Connect"
                        }
                    }

                    // ── Encounter section ──
                    section {
                        class: "contact-invite-encounter",
                        h3 { "Quick Connect" }
                        div { class: "contact-invite-encounter-row",
                            button {
                                class: "contact-invite-copy-btn",
                                onclick: on_create_encounter,
                                "Create Code"
                            }
                            if let Some(code) = encounter_code() {
                                div { class: "encounter-code", "{code}" }
                            }
                        }
                        div { class: "contact-invite-encounter-row",
                            input {
                                class: "contact-invite-input encounter-input",
                                r#type: "text",
                                placeholder: "Enter 6-digit code",
                                "aria-label": "Enter encounter code",
                                maxlength: "6",
                                value: "{join_input}",
                                oninput: move |evt: Event<FormData>| join_input.set(evt.value().clone()),
                            }
                            button {
                                class: "contact-invite-connect-btn",
                                disabled: join_input.read().trim().len() != 6,
                                onclick: on_join_encounter,
                                "Join"
                            }
                        }

                        if let (Some(cls), Some(txt)) = (enc_status_class, &enc_status_text) {
                            div {
                                class: "{cls}",
                                role: "alert",
                                "{txt}"
                            }
                        }
                    }
                }
            }
        }
    }
}
