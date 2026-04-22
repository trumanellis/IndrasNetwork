//! Peer bar — monospace "peers" label, identity dot (you) first, then
//! every contact, an "+ add contact" pill, and an online/offline counter.
//!
//! Matches `design/braid-prototype.html` `.peerbar` exactly.

use dioxus::prelude::*;

use crate::state::PeerDisplayInfo;

/// Peer bar row: [label] [you-dot] [peer-dots…] [add-contact] [counter].
#[component]
pub fn PeerBar(
    player_name: String,
    peers: Vec<PeerDisplayInfo>,
    on_add_contact: EventHandler<()>,
) -> Element {
    let letter = player_name.chars().next().unwrap_or('?').to_string();
    let online = peers.iter().filter(|p| p.online).count();
    let offline = peers.len().saturating_sub(online);
    let counter_text = if peers.is_empty() {
        "no peers yet".to_string()
    } else if offline == 0 {
        format!("{online} online")
    } else {
        format!("{online} online · {offline} offline")
    };

    rsx! {
        div { class: "peer-bar",
            span { class: "peer-bar-label", "peers" }
            span {
                class: "peer-dot self identity-love online",
                title: "{player_name}",
                "{letter}"
            }
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
                class: "peer-add-dot",
                title: "Add Contact",
                onclick: move |_| on_add_contact.call(()),
                "+"
            }
            span { class: "peer-bar-counter", "{counter_text}" }
        }
    }
}
