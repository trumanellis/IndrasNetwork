//! Top bar showing local identity, connected peer dots, and add-contact button.

use dioxus::prelude::*;
use indras_network::member::MemberId;

use crate::data::PeerDisplayInfo;

/// Peer bar component — identity + connected peer dots + add contact button.
#[component]
pub fn PeerBar(
    player_name: String,
    member_id: MemberId,
    peers: Vec<PeerDisplayInfo>,
    on_add_contact: EventHandler<()>,
    on_profile: EventHandler<()>,
    relay_status: Option<String>,
) -> Element {
    let letter = player_name.chars().next().unwrap_or('?').to_string();

    rsx! {
        div { class: "peer-bar",
            div {
                class: "peer-bar-identity",
                style: "cursor: pointer;",
                onclick: move |_| on_profile.call(()),
                div { class: "peer-dot peer-dot-self", "{letter}" }
                span { "{player_name}" }
            }
            div { class: "peer-strip",
                span { class: "peer-strip-label", "Peers" }
                for (i, peer) in peers.iter().enumerate() {
                    if i < 8 {
                        {
                            let online_class = if peer.online { " online" } else { "" };
                            let class_str = format!("peer-dot {}{}", peer.color_class, online_class);
                            rsx! {
                                div {
                                    class: "{class_str}",
                                    key: "{peer.name}-{i}",
                                    title: "{peer.name}",
                                    "{peer.letter}"
                                }
                            }
                        }
                    }
                }
                button {
                    class: "peer-add-btn",
                    title: "Make Contact",
                    onclick: move |_| on_add_contact.call(()),
                    "+"
                }
            }
            if let Some(ref status) = relay_status {
                div { class: "relay-status",
                    span { class: "relay-dot relay-connected" }
                    span { class: "relay-label", "{status}" }
                }
            }
            div { class: "peer-bar-brand",
                "Gift Cycle"
            }
        }
    }
}
