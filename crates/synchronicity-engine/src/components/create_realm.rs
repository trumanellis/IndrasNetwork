//! Create realm overlay — name input for creating group or world vaults.
//!
//! Groups additionally show connected peers to invite.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{group_tree_id, IndrasNetwork};

use crate::state::PeerDisplayInfo;
use crate::vault_manager::VaultManager;

/// Minimum peers (beyond self) required to create a group.
const GROUP_MIN_PEERS: usize = 2;

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
    vault_manager: Signal<Option<Arc<VaultManager>>>,
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
    let selected_count = selected_peers.read().len();
    let has_enough_peers = selected_count >= GROUP_MIN_PEERS;
    let in_flight = status.read().is_some();
    let can_create = !in_flight && !name_val.trim().is_empty() && (!is_group || has_enough_peers);

    let on_create = move |_| {
        // Guard against double-submit: if a create is already in flight, ignore.
        if status.read().is_some() {
            return;
        }
        let name = name_input.read().trim().to_string();
        if name.is_empty() {
            return;
        }
        let invitees: Vec<[u8; 32]> = if is_group {
            selected_peers.read().clone()
        } else {
            Vec::new()
        };
        let net = network.clone();
        // For groups, derive a deterministic tree id from (creator, member set)
        // so double-click / retry converges on the same realm instead of forking.
        let deterministic_id = if is_group {
            Some(group_tree_id(net.id(), &invitees))
        } else {
            None
        };
        status.set(Some("Creating...".to_string()));
        spawn(async move {
            let create_result = match deterministic_id {
                Some(aid) => net.create_realm_with_artifact(aid, &name).await,
                None => net.create_realm(&name).await,
            };
            match create_result {
                Ok(realm) => {
                    // Start vault sync for the new realm
                    if let Some(ref vm) = *vault_manager.read() {
                        if let Err(e) = vm.ensure_vault(&net, &realm, Some(name.as_str())).await {
                            tracing::warn!("Failed to init vault for new realm: {e}");
                        }
                    }

                    // Invite selected peers: grant them access on our side so our
                    // sync registry will push mutations to them, and send a
                    // GroupInvite to each peer's inbox so they mirror the artifact
                    // into their own home realm and materialize the sync interface.
                    if let Some(artifact_id) = realm.artifact_id().copied() {
                        if !invitees.is_empty() {
                            // Full member set including creator — both sides use the
                            // same set so each peer grants to every other.
                            let mut all_members = Vec::with_capacity(invitees.len() + 1);
                            all_members.push(net.id());
                            all_members.extend_from_slice(&invitees);

                            if let Ok(home) = net.home_realm().await {
                                for peer_id in &invitees {
                                    if let Err(e) = home
                                        .grant_access(
                                            &artifact_id,
                                            *peer_id,
                                            indras_artifacts::AccessMode::Permanent,
                                        )
                                        .await
                                    {
                                        tracing::warn!(error = %e, "Local grant to invitee failed");
                                    }
                                }
                            }

                            for peer_id in &invitees {
                                net.invite_peer_to_group(
                                    *peer_id,
                                    artifact_id,
                                    &name,
                                    all_members.clone(),
                                )
                                .await;
                            }
                        }
                    } else {
                        tracing::warn!("New realm has no artifact id; cannot invite peers");
                    }

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
                        h3 { if is_group { "Group Name" } else { "Vault Name" } }
                        input {
                            class: "contact-invite-input",
                            r#type: "text",
                            placeholder: if is_group { "e.g. Project Alpha" } else { "e.g. My World Notes" },
                            "aria-label": if is_group { "Group name" } else { "Vault name" },
                            value: "{name_val}",
                            oninput: move |evt| name_input.set(evt.value()),
                        }
                    }

                    // Peer selection (groups only)
                    if is_group {
                        section {
                            class: "contact-invite-connect",
                            h3 { "Invite Members" }
                            if peers.len() < GROUP_MIN_PEERS {
                                div {
                                    class: "contact-invite-preview",
                                    "Groups need at least {GROUP_MIN_PEERS} other people. Add contacts first."
                                }
                            } else {
                                div { class: "peer-select-list",
                                    for peer in &peers {
                                        {
                                            let mid = peer.member_id;
                                            let is_selected = selected_peers.read().contains(&mid);
                                            let item_class = if is_selected {
                                                "peer-select-item selected"
                                            } else {
                                                "peer-select-item"
                                            };
                                            let dot_class = if peer.online {
                                                format!("peer-dot {} online", peer.color_class)
                                            } else {
                                                format!("peer-dot {}", peer.color_class)
                                            };
                                            rsx! {
                                                div {
                                                    class: "{item_class}",
                                                    role: "checkbox",
                                                    "aria-checked": if is_selected { "true" } else { "false" },
                                                    tabindex: "0",
                                                    onclick: move |_| {
                                                        let mut sel = selected_peers.write();
                                                        if let Some(pos) = sel.iter().position(|id| *id == mid) {
                                                            sel.remove(pos);
                                                        } else {
                                                            sel.push(mid);
                                                        }
                                                    },
                                                    span {
                                                        class: "{dot_class}",
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
                                {
                                    let counter_class = if has_enough_peers {
                                        "peer-select-counter ready"
                                    } else {
                                        "peer-select-counter"
                                    };
                                    rsx! {
                                        div {
                                            class: "{counter_class}",
                                            "aria-live": "polite",
                                            "{selected_count}/{GROUP_MIN_PEERS} selected — groups need at least {GROUP_MIN_PEERS}"
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
                        if in_flight { "Creating\u{2026}" } else { "Create" }
                    }
                }
            }
        }
    }
}
