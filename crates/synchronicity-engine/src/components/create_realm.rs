//! Create realm overlay — name input for creating group or world vaults.
//!
//! Groups additionally show connected peers to invite.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::PeerDisplayInfo;

/// Whether we are creating a group (multi-peer) or world vault.
#[derive(Clone, Copy, PartialEq)]
pub enum CreateRealmKind {
    /// Shared vault with selected peers.
    Group,
    /// World vault visible to anyone.
    World,
}

/// Overlay for creating a new group or world vault.
#[component]
pub fn CreateRealmOverlay(
    network: Signal<Option<Arc<IndrasNetwork>>>,
    kind: CreateRealmKind,
    peers: Vec<PeerDisplayInfo>,
    mut is_open: Signal<bool>,
) -> Element {
    let Some(network) = network.read().clone() else {
        return rsx! {};
    };

    let mut name_input = use_signal(String::new);
    let mut status = use_signal(|| None::<String>);
    let mut selected_peers = use_signal(Vec::<[u8; 32]>::new);

    if !is_open() {
        return rsx! {};
    }

    let is_group = kind == CreateRealmKind::Group;
    let title = if is_group { "New Group" } else { "New World Vault" };
    let name_val = name_input();
    let can_create = !name_val.trim().is_empty();

    let on_create = move |_| {
        let name = name_input.read().trim().to_string();
        if name.is_empty() {
            return;
        }
        let net = network.clone();
        status.set(Some("Creating...".to_string()));
        spawn(async move {
            match net.create_realm(&name).await {
                Ok(_realm) => {
                    // TODO: for groups, invite selected peers once the API exists
                    status.set(Some(format!("success:Created \"{}\"!", name)));
                    name_input.set(String::new());
                    selected_peers.write().clear();
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    is_open.set(false);
                    status.set(None);
                }
                Err(e) => {
                    status.set(Some(format!("error:Failed: {e}")));
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    is_open.set(false);
                    status.set(None);
                }
            }
        });
    };

    let status_val = status();
    let status_class = match &status_val {
        Some(s) if s.starts_with("error:") => Some("contact-invite-status-error"),
        Some(s) if s.starts_with("success:") => Some("contact-invite-status-success"),
        _ => None,
    };
    let status_text = match &status_val {
        Some(s) if s.starts_with("error:") => Some(s.strip_prefix("error:").unwrap_or(s).to_string()),
        Some(s) if s.starts_with("success:") => Some(s.strip_prefix("success:").unwrap_or(s).to_string()),
        Some(s) => Some(s.clone()),
        _ => None,
    };

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
                    h2 { "{title}" }
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

                    // Name input
                    section {
                        class: "contact-invite-share",
                        h3 { "Vault Name" }
                        input {
                            class: "contact-invite-input",
                            r#type: "text",
                            placeholder: if is_group { "e.g. Project Alpha" } else { "e.g. My World Notes" },
                            "aria-label": "Vault name",
                            value: "{name_val}",
                            oninput: move |evt| name_input.set(evt.value()),
                        }
                    }

                    // Peer selection (groups only)
                    if is_group {
                        section {
                            class: "contact-invite-connect",
                            h3 { "Invite Members" }
                            if peers.is_empty() {
                                div {
                                    class: "contact-invite-preview",
                                    "No contacts yet. Add a contact first."
                                }
                            } else {
                                div { class: "peer-select-list",
                                    for peer in &peers {
                                        {
                                            let mid = peer.member_id;
                                            let is_selected = selected_peers.read().contains(&mid);
                                            let check_class = if is_selected {
                                                "peer-select-item selected"
                                            } else {
                                                "peer-select-item"
                                            };
                                            rsx! {
                                                div {
                                                    class: "{check_class}",
                                                    onclick: move |_| {
                                                        let mut sel = selected_peers.write();
                                                        if let Some(pos) = sel.iter().position(|id| *id == mid) {
                                                            sel.remove(pos);
                                                        } else {
                                                            sel.push(mid);
                                                        }
                                                    },
                                                    span {
                                                        class: "peer-dot {peer.color_class}",
                                                        "{peer.letter}"
                                                    }
                                                    span { class: "peer-select-name", "{peer.name}" }
                                                    if is_selected {
                                                        span { class: "peer-select-check", "\u{2713}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Status
                    if let (Some(cls), Some(txt)) = (status_class, &status_text) {
                        div {
                            class: "{cls}",
                            role: "alert",
                            "{txt}"
                        }
                    }

                    // Create button
                    button {
                        class: "contact-invite-connect-btn",
                        disabled: !can_create,
                        onclick: on_create,
                        "Create"
                    }
                }
            }
        }
    }
}
