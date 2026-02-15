//! Peer presence strip showing connected peers with online indicators.

use dioxus::prelude::*;

/// Display info for a peer in the strip.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerDisplayInfo {
    pub name: String,
    pub letter: String,
    pub color_class: String,
    pub online: bool,
}

/// Horizontal strip of peer avatars with online indicators.
#[component]
pub fn PeerStrip(
    peers: Vec<PeerDisplayInfo>,
    #[props(optional)]
    on_add_contact: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div {
            class: "peer-strip",
            span { class: "peer-strip-label", "Peers" }
            for peer in peers.iter() {
                {
                    let online_class = if peer.online { " online" } else { "" };
                    let class_str = format!("peer-dot {}{}", peer.color_class, online_class);
                    rsx! {
                        div {
                            class: "{class_str}",
                            title: "{peer.name}",
                            "{peer.letter}"
                        }
                    }
                }
            }
            if let Some(handler) = &on_add_contact {
                {
                    let handler = handler.clone();
                    rsx! {
                        button {
                            class: "peer-add-btn",
                            title: "Make Contact",
                            onclick: move |_| handler.call(()),
                            "+"
                        }
                    }
                }
            }
        }
    }
}
