//! Review a claim and bless flow — creator blesses a claimant's service.

use std::collections::HashMap;
use dioxus::prelude::*;
use indras_network::member::MemberId;
use indras_sync_engine::IntentionId;

use crate::bridge::GiftCycleBridge;
use crate::data::{member_display, IntentionViewData};

/// Blessing view — review claim and bless.
#[component]
pub fn BlessingView(
    intention_id: IntentionId,
    claimant: MemberId,
    view_data: Option<IntentionViewData>,
    bridge: GiftCycleBridge,
    peer_names: HashMap<MemberId, String>,
    on_blessed: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut blessing = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    let Some(data) = view_data else {
        return rsx! {
            div { class: "detail-loading", "Loading..." }
        };
    };

    let (claimant_name, claimant_letter, _claimant_color) =
        member_display(&claimant, &bridge.member_id, &bridge.player_name, 1, &peer_names);

    // Find the proof entry for this claimant
    let proof = data
        .proofs
        .iter()
        .find(|p| p.claimant == claimant);

    let unblessed_count = data.unblessed_event_indices.len();

    rsx! {
        div { class: "blessing-view",
            div { class: "form-header",
                h2 { class: "form-title", "Verify & Bless" }
                button {
                    class: "gc-btn gc-btn-outline",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }

            // Intention context
            div { class: "blessing-context",
                h3 { class: "detail-title", "{data.title}" }
                p { class: "detail-description", "{data.description}" }
            }

            // Claimant info
            if let Some(p) = proof {
                div { class: "proof-card",
                    div { class: "proof-thumb",
                        div { class: "creator-avatar {p.author_color_class}", "{p.author_letter}" }
                    }
                    div { class: "proof-info",
                        div { class: "proof-name", "{p.author_name}" }
                        div { class: "proof-body", "{p.body}" }
                        div { class: "proof-meta", "{p.time_ago}" }
                    }
                }
            }

            // Blessing flow visualization
            div { class: "blessing-visual",
                div { class: "blessing-flow",
                    div {
                        class: "blessing-avatar",
                        style: "background: rgba(129,140,248,0.15); color: var(--ac);",
                        "{data.creator_letter}"
                    }
                    div { class: "blessing-arrow", "\u{2192}" }
                    div { class: "blessing-label",
                        if !data.total_attention_duration.is_empty() {
                            "{data.total_attention_duration} attention"
                        } else {
                            "attention"
                        }
                    }
                    div { class: "blessing-arrow", "\u{2192}" }
                    div {
                        class: "blessing-avatar",
                        style: "background: rgba(52,211,153,0.15); color: var(--ok);",
                        "{claimant_letter}"
                    }
                }
                div { class: "blessing-names",
                    span { "{data.creator_name}" }
                    span { class: "blessing-names-arrow", "\u{2192}" }
                    span { "{claimant_name}" }
                }
            }

            // Unblessed attention events
            div { class: "blessing-events",
                div { class: "section-header",
                    span { class: "section-title", "Attention events to bless" }
                    span { class: "section-count", "{unblessed_count}" }
                }
                if unblessed_count == 0 {
                    div { class: "gc-hint", "No unblessed attention events available." }
                }
            }

            if let Some(err) = error_msg() {
                div { class: "form-error", "{err}" }
            }

            div { class: "form-actions",
                button {
                    class: "gc-btn gc-btn-glow",
                    disabled: blessing() || unblessed_count == 0,
                    onclick: move |_| {
                        let b = bridge.clone();
                        let indices = data.unblessed_event_indices.clone();
                        blessing.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            match b.bless_claim(intention_id, claimant, indices).await {
                                Ok(_) => on_blessed.call(()),
                                Err(e) => {
                                    error_msg.set(Some(format!("{e}")));
                                    blessing.set(false);
                                }
                            }
                        });
                    },
                    if blessing() {
                        "Blessing..."
                    } else {
                        "\u{2728} Verify & Bless"
                    }
                }
            }
        }
    }
}
