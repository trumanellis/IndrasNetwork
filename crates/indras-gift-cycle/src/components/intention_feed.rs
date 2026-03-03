//! Intention feed — unified list of all intentions.

use dioxus::prelude::*;
use indras_sync_engine::IntentionId;

use crate::data::IntentionCardData;

/// Unified intention feed component.
#[component]
pub fn IntentionFeed(
    cards: Vec<IntentionCardData>,
    on_select: EventHandler<IntentionId>,
    on_create: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "intention-feed",
            // Header
            div { class: "feed-header",
                h2 { class: "feed-title", "Intentions" }
                button {
                    class: "gc-btn gc-btn-primary",
                    onclick: move |_| on_create.call(()),
                    "\u{2795} New Intention"
                }
            }

            div { class: "feed-section",
                div { class: "section-header",
                    span { class: "section-title", "All Intentions" }
                    span { class: "section-count", "{cards.len()}" }
                }
                if cards.is_empty() {
                    div { class: "feed-empty",
                        "No intentions yet. Create one to start the cycle."
                    }
                }
                for card in &cards {
                    IntentionCard {
                        card: card.clone(),
                        on_click: move |id| on_select.call(id),
                    }
                }
            }
        }
    }
}

/// A single intention card in the feed.
#[component]
fn IntentionCard(card: IntentionCardData, on_click: EventHandler<IntentionId>) -> Element {
    let heat_class = if card.heat >= 0.8 {
        "heat-5"
    } else if card.heat >= 0.6 {
        "heat-4"
    } else if card.heat >= 0.4 {
        "heat-3"
    } else if card.heat >= 0.2 {
        "heat-2"
    } else if card.heat > 0.0 {
        "heat-1"
    } else {
        ""
    };

    let status_class = match card.status.as_str() {
        "Fulfilled" => "status-fulfilled",
        "Verified" => "status-verified",
        "Proven" => "status-proven",
        _ => "status-open",
    };

    let raw_id = card.raw_id;
    let desc_preview: String = card.description.chars().take(120).collect();

    rsx! {
        div {
            class: "intention-card",
            onclick: move |_| on_click.call(raw_id),

            div { class: "intention-card-header",
                div { class: "creator-avatar {card.creator_color_class}", "{card.creator_letter}" }
                div { class: "intention-card-meta",
                    div { class: "intention-card-creator", "{card.creator_name}" }
                    div { class: "intention-card-time", "{card.posted_ago}" }
                }
                div { class: "status-badge {status_class}", "{card.status}" }
            }

            div { class: "intention-card-title", "{card.title}" }
            if !desc_preview.is_empty() {
                div { class: "intention-card-desc", "{desc_preview}" }
            }

            div { class: "intention-card-footer",
                if !heat_class.is_empty() {
                    div { class: "heat-dot {heat_class}",
                        title: "Heat: {card.heat:.1}",
                    }
                }
                if !card.attention_duration.is_empty() {
                    span { class: "card-meta-item",
                        "\u{1f441} {card.attention_duration}"
                    }
                }
                if card.proof_count > 0 {
                    span { class: "card-meta-item",
                        "\u{1f4cb} {card.proof_count}"
                    }
                }
                if card.token_count > 0 {
                    div { class: "tagged-badge",
                        span { class: "tagged-icon", "\u{1fa99}" }
                        span { class: "tagged-label", "{card.token_count} tagged" }
                    }
                }
            }
        }
    }
}
