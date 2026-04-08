//! Top bar showing local identity, connected peer dots, and add-contact button.
//!
//! Adapted from `indras-gift-cycle/src/components/peer_bar.rs` with simplified
//! props (no relay status, no profile handler).

use dioxus::prelude::*;

use crate::state::PeerDisplayInfo;

/// Peer bar — identity dot + connected peer dots + "+" add contact button.
#[component]
pub fn PeerBar(
    player_name: String,
    peers: Vec<PeerDisplayInfo>,
    on_add_contact: EventHandler<()>,
) -> Element {
    let letter = player_name.chars().next().unwrap_or('?').to_string();

    rsx! {
        div { class: "peer-bar",
            div { class: "peer-bar-identity",
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
                    title: "Add Contact",
                    onclick: move |_| on_add_contact.call(()),
                    "+"
                }
            }
        }
    }
}
