//! Token wallet — list of tokens held by the local member.

use dioxus::prelude::*;

use crate::bridge::GiftCycleBridge;
use crate::data::{IntentionCardData, TokenCardData};

/// Token wallet component.
#[component]
pub fn TokenWallet(
    tokens: Vec<TokenCardData>,
    bridge: GiftCycleBridge,
    my_cards: Vec<IntentionCardData>,
    on_pledge: EventHandler<()>,
    on_back: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "token-wallet",
            div { class: "feed-header",
                h2 { class: "feed-title", "\u{1fa99} Token Wallet" }
                button {
                    class: "gc-btn gc-btn-outline",
                    onclick: move |_| on_back.call(()),
                    "\u{2190} Back"
                }
            }

            if tokens.is_empty() {
                div { class: "feed-empty",
                    "No tokens yet. Tokens are minted when a creator blesses your service."
                }
            }

            // Value formula
            if !tokens.is_empty() {
                div { class: "formula",
                    "value = attention \u{00d7} liveness"
                }
            }

            for token in &tokens {
                TokenCard {
                    token: token.clone(),
                    my_cards: my_cards.clone(),
                    bridge: bridge.clone(),
                    on_pledge: move |_| on_pledge.call(()),
                }
            }
        }
    }
}

/// A single token card in the wallet.
#[component]
fn TokenCard(
    token: TokenCardData,
    my_cards: Vec<IntentionCardData>,
    bridge: GiftCycleBridge,
    on_pledge: EventHandler<()>,
) -> Element {
    let mut show_pledge = use_signal(|| false);
    let mut pledging = use_signal(|| false);

    let is_pledged = token.pledged_to.is_some();
    let token_id_short = &token.id[..8.min(token.id.len())];

    rsx! {
        div { class: "token-card",
            // Header
            div { class: "token-header",
                span { class: "token-id", "tok:{token_id_short}" }
                if is_pledged {
                    span { class: "token-state", "Pledged" }
                } else {
                    span { class: "token-state", "Available" }
                }
            }

            // Info rows
            div { class: "token-row",
                span { class: "token-key", "Backed by" }
                span { class: "token-val", "{token.attention_duration}" }
            }
            div { class: "token-row",
                span { class: "token-key", "Blesser" }
                span { class: "token-val",
                    span { class: "peer-avatar-sm {token.blesser_color_class}", "{token.blesser_letter}" }
                    " {token.blesser_name}"
                }
            }
            div { class: "token-row",
                span { class: "token-key", "Source" }
                span { class: "token-val", "{token.source_intention_title}" }
            }
            div { class: "token-row",
                span { class: "token-key", "Holder" }
                span { class: "token-val",
                    span { class: "peer-avatar-sm {token.current_holder_color_class}", "{token.current_holder_letter}" }
                    " {token.current_holder_name}"
                }
            }
            if let Some(ref pledged) = token.pledged_to {
                div { class: "token-row",
                    span { class: "token-key", "Pledged to" }
                    span { class: "token-val", "{pledged}" }
                }
            }
            div { class: "token-row",
                span { class: "token-key", "Created" }
                span { class: "token-val", "{token.created_ago}" }
            }

            // Steward chain
            if !token.steward_chain.is_empty() {
                div { class: "steward-chain",
                    span { class: "steward-chain-label", "Chain" }
                    for (i, dot) in token.steward_chain.iter().enumerate() {
                        if i > 0 {
                            span { class: "steward-arrow", "\u{2192}" }
                        }
                        div {
                            class: "steward-dot {dot.color_class}",
                            title: "{dot.name}",
                            "{dot.letter}"
                        }
                    }
                }
            }

            // Actions
            div { class: "token-actions",
                if !is_pledged {
                    button {
                        class: "gc-btn gc-btn-outline",
                        onclick: move |_| show_pledge.toggle(),
                        "Pledge"
                    }
                } else {
                    {
                        let b = bridge.clone();
                        let raw_id = token.raw_id;
                        rsx! {
                            button {
                                class: "gc-btn gc-btn-outline",
                                onclick: move |_| {
                                    let b = b.clone();
                                    pledging.set(true);
                                    spawn(async move {
                                        let _ = b.withdraw_token(raw_id).await;
                                        pledging.set(false);
                                        on_pledge.call(());
                                    });
                                },
                                "Withdraw"
                            }
                        }
                    }
                }
            }

            // Pledge picker (expanded)
            if show_pledge() && !is_pledged {
                div { class: "pledge-picker",
                    div { class: "gc-label", "Pledge to intention:" }
                    for card in &my_cards {
                        {
                            let b = bridge.clone();
                            let raw_token_id = token.raw_id;
                            let intention_id = card.raw_id;
                            let title = card.title.clone();
                            rsx! {
                                button {
                                    class: "pledge-option",
                                    onclick: move |_| {
                                        let b = b.clone();
                                        pledging.set(true);
                                        spawn(async move {
                                            let _ = b.pledge_token(raw_token_id, intention_id).await;
                                            pledging.set(false);
                                            show_pledge.set(false);
                                            on_pledge.call(());
                                        });
                                    },
                                    "{title}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
