//! Peer profile popup — read-only view of a connection's shared profile.
//!
//! Reads the typed `ProfileIdentityDocument` the peer mirrored into the
//! shared DM realm. Empty fields mean "not granted to me" or "not set."
//! Online + verified-contact indicators are derived locally on this side
//! (`Realm::is_member_online`, `ContactsRealm::get_status`) — they don't
//! cross the wire.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_sync_engine::ProfileIdentityDocument;

use crate::profile_bridge::load_peer_profile_from_dm;
use crate::state::{AppState, PeerDisplayInfo};

/// Loaded popup state — rolled into one signal so the render is one read.
#[derive(Clone, Default)]
struct PopupSnapshot {
    loading: bool,
    profile: Option<ProfileIdentityDocument>,
    online: bool,
}

/// Overlay showing the peer's shared profile fields.
#[component]
pub fn PeerProfilePopup(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peers: Signal<Vec<PeerDisplayInfo>>,
) -> Element {
    let target = state.read().profile_popup_target;
    let Some((peer_id, realm_id)) = target else { return rsx! {} };

    let peer_info = peers.read().iter().find(|p| p.member_id == peer_id).cloned();
    let avatar_letter = peer_info
        .as_ref()
        .map(|p| p.letter.clone())
        .unwrap_or_else(|| "?".to_string());
    let avatar_color = peer_info
        .as_ref()
        .map(|p| p.color_class.clone())
        .unwrap_or_else(|| "peer-dot-sage".to_string());
    let header_name = peer_info
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| hex_prefix(&peer_id));

    let mut snapshot = use_signal(|| PopupSnapshot {
        loading: true,
        ..Default::default()
    });

    use_effect(use_reactive!(|(peer_id, realm_id)| {
        let net = network.read().clone();
        spawn(async move {
            snapshot.set(PopupSnapshot { loading: true, ..Default::default() });
            let Some(net) = net else {
                snapshot.set(PopupSnapshot::default());
                return;
            };

            let profile = load_peer_profile_from_dm(&net, peer_id, realm_id).await;

            // Online: real transport connection state, not sticky gossip
            // membership.
            let _ = realm_id; // realm scoping no longer needed for online check
            let online = net.is_peer_connected(&peer_id).await;

            snapshot.set(PopupSnapshot {
                loading: false,
                profile,
                online,
            });
        });
    }));

    let snap = snapshot.read().clone();
    let online_class = if snap.online { "peer-status-online" } else { "peer-status-offline" };
    let online_label = if snap.online { "Online" } else { "Offline" };

    rsx! {
        div {
            class: "contact-invite-overlay",
            onclick: move |_| state.write().profile_popup_target = None,

            div {
                class: "contact-invite-dialog peer-profile-dialog",
                role: "dialog",
                "aria-modal": "true",
                onclick: move |e| e.stop_propagation(),

                div { class: "contact-invite-header",
                    div { class: "peer-profile-header-identity",
                        div { class: "profile-avatar {avatar_color}",
                            "{avatar_letter}"
                            span { class: "peer-status-dot {online_class}", title: "{online_label}" }
                        }
                        h2 { "{header_name}" }
                    }
                    button {
                        class: "contact-invite-close",
                        "aria-label": "Close",
                        onclick: move |_| state.write().profile_popup_target = None,
                        "\u{00d7}"
                    }
                }

                div { class: "contact-invite-content peer-profile-content",
                    if snap.loading {
                        div { class: "peer-profile-empty", "Loading\u{2026}" }
                    } else {
                        {
                            let badges = rsx! {
                                div { class: "peer-profile-badges",
                                    span { class: "peer-profile-badge {online_class}", "{online_label}" }
                                }
                            };
                            match snap.profile {
                                None => rsx! {
                                    {badges}
                                    div { class: "peer-profile-empty", "No profile info shared yet." }
                                },
                                Some(p) => {
                                    let display = p.display_name.clone();
                                    let username = p.username.clone();
                                    let bio_text = p.bio.clone().unwrap_or_default();
                                    let public_key = p.public_key.clone();
                                    let nothing_visible = display.is_empty()
                                        && username.is_empty()
                                        && bio_text.is_empty()
                                        && public_key.is_empty();
                                    rsx! {
                                        {badges}
                                        if nothing_visible {
                                            div { class: "peer-profile-empty",
                                                "Connection has shared no profile fields with you."
                                            }
                                        } else {
                                            if !display.is_empty() {
                                                div { class: "peer-profile-field",
                                                    div { class: "peer-profile-field-label", "Display Name" }
                                                    div { class: "peer-profile-field-value", "{display}" }
                                                }
                                            }
                                            if !username.is_empty() {
                                                div { class: "peer-profile-field",
                                                    div { class: "peer-profile-field-label", "Username" }
                                                    div { class: "peer-profile-field-value", "{username}" }
                                                }
                                            }
                                            if !bio_text.is_empty() {
                                                div { class: "peer-profile-field",
                                                    div { class: "peer-profile-field-label", "Bio" }
                                                    div { class: "peer-profile-field-value", "{bio_text}" }
                                                }
                                            }
                                            if !public_key.is_empty() {
                                                div { class: "peer-profile-field",
                                                    div { class: "peer-profile-field-label", "Public Key" }
                                                    div { class: "peer-profile-field-value", "{public_key}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn hex_prefix(member_id: &[u8; 32]) -> String {
    member_id.iter().take(4).map(|b| format!("{b:02x}")).collect()
}
