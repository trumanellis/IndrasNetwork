//! Full detail view for a single intention with attention timer.

use dioxus::prelude::*;
use indras_network::member::MemberId;
use indras_sync_engine::IntentionId;

use crate::bridge::GiftCycleBridge;
use crate::data::IntentionViewData;

/// Intention detail component.
#[component]
pub fn IntentionDetail(
    view_data: Option<IntentionViewData>,
    intention_id: IntentionId,
    is_creator: bool,
    bridge: GiftCycleBridge,
    on_back: EventHandler<()>,
    on_submit_proof: EventHandler<IntentionId>,
    on_bless: EventHandler<(IntentionId, MemberId)>,
) -> Element {
    // Extract source_realm_id from view data for attention routing
    let source_realm_id = view_data.as_ref().and_then(|d| d.source_realm_id);

    // Focus attention on mount
    let bridge_focus = bridge.clone();
    let _focus = use_resource(move || {
        let b = bridge_focus.clone();
        async move {
            if let Err(e) = b.focus_attention(intention_id, source_realm_id).await {
                tracing::warn!(error = %e, "Failed to focus attention");
            }
        }
    });

    // Clear attention on unmount via drop
    let bridge_clear = bridge.clone();
    use_drop(move || {
        let b = bridge_clear.clone();
        tokio::spawn(async move {
            let _ = b.clear_attention(source_realm_id).await;
        });
    });

    // Attention timer (seconds since component mounted)
    let mut elapsed_secs = use_signal(|| 0u64);
    let _timer = use_resource(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            elapsed_secs += 1;
        }
    });

    let Some(data) = view_data else {
        return rsx! {
            div { class: "detail-loading",
                "Loading intention..."
            }
        };
    };

    let timer_m = elapsed_secs() / 60;
    let timer_s = elapsed_secs() % 60;

    let status_class = match data.status.as_str() {
        "Fulfilled" => "status-fulfilled",
        "Verified" => "status-verified",
        "Proven" => "status-proven",
        _ => "status-open",
    };

    rsx! {
        div { class: "intention-detail",
            // Back button
            div { class: "detail-nav",
                button {
                    class: "gc-btn gc-btn-outline",
                    onclick: move |_| on_back.call(()),
                    "\u{2190} Back"
                }
            }

            // Header
            div { class: "intention-detail-header",
                div { class: "creator-avatar {data.creator_color_class}", "{data.creator_letter}" }
                div { class: "intention-card-meta",
                    div { class: "intention-card-creator", "{data.creator_name}" }
                    div { class: "intention-card-time", "{data.posted_ago}" }
                }
                div { class: "status-badge {status_class}", "{data.status}" }
            }

            h2 { class: "intention-detail-title", "{data.title}" }

            // Attention timer
            div { class: "attn-timer",
                div { class: "attn-pulse" }
                span { class: "attn-label", "Attention active" }
                span { class: "attn-value", "{timer_m}:{timer_s:02}" }
            }

            // Total attention from all peers
            if !data.attention_peers.is_empty() {
                div { class: "detail-section",
                    div { class: "section-header",
                        span { class: "section-title", "Attention" }
                        span { class: "section-count", "{data.total_attention_duration}" }
                    }
                    for peer in &data.attention_peers {
                        div { class: "attn-peer-row",
                            div { class: "creator-avatar {peer.peer_color_class}", "{peer.peer_letter}" }
                            span { class: "attn-peer-name", "{peer.peer_name}" }
                            div { class: "attn-bar-track",
                                div {
                                    class: "attn-bar-fill",
                                    style: "width: {peer.bar_fraction * 100.0}%",
                                }
                            }
                            span { class: "attn-peer-dur", "{peer.total_duration}" }
                        }
                    }
                }
            }

            // Tagged tokens
            if !data.pledged_tokens.is_empty() {
                div { class: "detail-section",
                    div { class: "section-header",
                        span { class: "section-title", "Staked Gratitude" }
                        span { class: "section-count", "{data.pledged_tokens.len()} tokens" }
                    }
                    for token in &data.pledged_tokens {
                        div { class: "tagged-badge",
                            span { class: "tagged-icon", "\u{1fa99}" }
                            span { class: "tagged-label", "{token.token_label}" }
                            span { class: "tagged-val", "from {token.from_name}" }
                        }
                    }
                }
            }

            // Description
            div { class: "detail-section",
                div { class: "section-header",
                    span { class: "section-title", "Description" }
                }
                p { class: "detail-description", "{data.description}" }
            }

            // Claims / Proofs
            if !data.proofs.is_empty() {
                div { class: "detail-section",
                    div { class: "section-header",
                        span { class: "section-title", "Claims" }
                        span { class: "section-count", "{data.proofs.len()}" }
                    }
                    for proof in &data.proofs {
                        div { class: "proof-card",
                            div { class: "proof-thumb",
                                div { class: "creator-avatar {proof.author_color_class}",
                                    "{proof.author_letter}"
                                }
                            }
                            div { class: "proof-info",
                                div { class: "proof-name", "{proof.author_name}" }
                                div { class: "proof-body", "{proof.body}" }
                                div { class: "proof-meta", "{proof.time_ago}" }
                            }
                            if is_creator && !proof.is_verified {
                                {
                                    let claimant = proof.claimant;
                                    rsx! {
                                        button {
                                            class: "gc-btn gc-btn-warm",
                                            onclick: move |_| on_bless.call((intention_id, claimant)),
                                            "\u{2728} Verify & Bless"
                                        }
                                    }
                                }
                            }
                            if proof.is_verified {
                                div { class: "proof-verified-badge", "\u{2705} Verified" }
                            }
                        }
                    }
                }
            }

            // Action buttons
            div { class: "intention-detail-actions",
                if !is_creator {
                    button {
                        class: "gc-btn gc-btn-success",
                        onclick: move |_| on_submit_proof.call(intention_id),
                        "\u{1f932} Claim & Submit Proof"
                    }
                }
            }
        }
    }
}
