//! Peer profile popup — read-only view of a connection's shared profile.
//!
//! Reads fields the peer has mirrored into the shared DM realm. Only fields
//! the peer has granted us access to ever reach this view (grant filtering
//! happens at write time in `profile_mirror.rs`).

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_sync_engine::HomepageField;

use crate::profile_bridge::{field_label, load_peer_profile_from_dm};
use crate::state::{AppState, PeerDisplayInfo};

/// Overlay showing the peer's shared profile fields.
///
/// Resolves the peer's display info from the `peers` list for the avatar /
/// header, then loads the mirrored profile doc from the DM realm. Empty
/// mirror → renders a muted "No profile info shared yet" line.
#[component]
pub fn PeerProfilePopup(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peers: Signal<Vec<PeerDisplayInfo>>,
) -> Element {
    let target = state.read().profile_popup_target;
    let Some((peer_id, realm_id)) = target else { return rsx! {} };

    let peer_info = peers
        .read()
        .iter()
        .find(|p| p.member_id == peer_id)
        .cloned();
    let display_name = peer_info
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| hex_prefix(&peer_id));
    let avatar_letter = peer_info
        .as_ref()
        .map(|p| p.letter.clone())
        .unwrap_or_else(|| display_name.chars().next().unwrap_or('?').to_string());
    let avatar_color = peer_info
        .as_ref()
        .map(|p| p.color_class.clone())
        .unwrap_or_else(|| "peer-dot-sage".to_string());

    // Local state for the loaded mirror fields.
    let mut loaded = use_signal(Vec::<HomepageField>::new);
    let mut loading = use_signal(|| true);

    // Re-load whenever the popup target changes.
    use_effect(use_reactive!(|(peer_id, realm_id)| {
        let net = network.read().clone();
        spawn(async move {
            loading.set(true);
            let fields = match net {
                Some(n) => load_peer_profile_from_dm(&n, peer_id, realm_id).await,
                None => Vec::new(),
            };
            loaded.set(fields);
            loading.set(false);
        });
    }));

    let fields_snapshot = loaded.read().clone();
    let is_loading = *loading.read();

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
                        div { class: "profile-avatar {avatar_color}", "{avatar_letter}" }
                        h2 { "{display_name}" }
                    }
                    button {
                        class: "contact-invite-close",
                        "aria-label": "Close",
                        onclick: move |_| state.write().profile_popup_target = None,
                        "\u{00d7}"
                    }
                }

                div { class: "contact-invite-content peer-profile-content",
                    if is_loading {
                        div { class: "peer-profile-empty", "Loading\u{2026}" }
                    } else if fields_snapshot.is_empty() {
                        div { class: "peer-profile-empty", "No profile info shared yet." }
                    } else {
                        for field in fields_snapshot.iter() {
                            {
                                let label = field_label(&field.name);
                                let value = field.value.clone();
                                if value.is_empty() {
                                    rsx! {}
                                } else {
                                    rsx! {
                                        div { class: "peer-profile-field",
                                            div { class: "peer-profile-field-label", "{label}" }
                                            div { class: "peer-profile-field-value", "{value}" }
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
