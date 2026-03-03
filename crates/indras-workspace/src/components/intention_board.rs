//! Intention Board dashboard — the main view centered on the Intention cycle.

use dioxus::prelude::*;
use crate::state::workspace::DashboardTab;
use crate::services::realm_data::TokenCardData;
use indras_sync_engine::IntentionKind;

/// Summary card for an intention in the board list.
#[derive(Clone, Debug, PartialEq)]
pub struct IntentionCardData {
    pub id: String,
    pub kind: IntentionKind,
    pub title: String,
    pub description: String,
    pub proof_count: usize,
    pub token_count: usize,
    pub attention_duration: String,
    pub heat: f32,
    pub is_complete: bool,
}

/// Tab bar for the dashboard.
#[component]
fn DashboardTabs(
    active: DashboardTab,
    on_change: EventHandler<DashboardTab>,
) -> Element {
    let tabs = [
        DashboardTab::MyIntentions,
        DashboardTab::Community,
        DashboardTab::Tokens,
        DashboardTab::Chat,
    ];

    rsx! {
        div {
            class: "dashboard-tabs",
            for tab in tabs {
                {
                    let is_active = tab == active;
                    let cls = if is_active { "dashboard-tab active" } else { "dashboard-tab" };
                    rsx! {
                        button {
                            class: "{cls}",
                            onclick: move |_| on_change.call(tab),
                            "{tab.label()}"
                        }
                    }
                }
            }
        }
    }
}

/// Single intention card in the board list.
#[component]
fn IntentionCard(
    data: IntentionCardData,
    on_click: EventHandler<String>,
) -> Element {
    let heat_pct = (data.heat * 100.0).min(100.0) as u32;
    let kind_icon = data.kind.icon();
    let kind_css = data.kind.css_class();
    let card_cls = if data.is_complete {
        format!("intention-card completed {}", kind_css)
    } else {
        format!("intention-card {}", kind_css)
    };
    let id = data.id.clone();

    rsx! {
        div {
            class: "{card_cls}",
            onclick: move |_| on_click.call(id.clone()),
            div {
                class: "intention-card-header",
                div { class: "intention-card-icon-orb", "{kind_icon}" }
                span { class: "intention-card-title", "{data.title}" }
                if data.heat > 0.0 {
                    div {
                        class: "intention-card-heat",
                        div {
                            class: "intention-card-heat-track",
                            div {
                                class: "intention-card-heat-fill",
                                style: "width:{heat_pct}%",
                            }
                        }
                    }
                }
            }
            div { class: "intention-card-desc", "{data.description}" }
            div {
                class: "intention-card-meta",
                if data.proof_count > 0 {
                    span { class: "intention-card-stat", "{data.proof_count} proofs" }
                }
                if data.token_count > 0 {
                    span { class: "intention-card-stat", "{data.token_count} tokens" }
                }
                if !data.attention_duration.is_empty() {
                    span { class: "intention-card-stat", "{data.attention_duration}" }
                }
            }
        }
    }
}

/// My Intentions tab — shows the user's own intentions.
#[component]
pub fn MyIntentionsTab(
    intentions: Vec<IntentionCardData>,
    on_click: EventHandler<String>,
    on_create: EventHandler<()>,
) -> Element {
    let active: Vec<IntentionCardData> = intentions.iter().filter(|i| !i.is_complete).cloned().collect();
    let fulfilled: Vec<IntentionCardData> = intentions.iter().filter(|i| i.is_complete).cloned().collect();
    let has_active = !active.is_empty();
    let has_fulfilled = !fulfilled.is_empty();
    let fulfilled_count = fulfilled.len();
    let mut show_fulfilled = use_signal(|| false);

    rsx! {
        div {
            class: "intentions-tab",

            // Create button
            button {
                class: "create-intention-btn",
                onclick: move |_| on_create.call(()),
                "+ Create New Intention"
            }

            // Active intentions
            if has_active {
                div {
                    class: "intentions-section",
                    div { class: "intentions-section-title", "Active Intentions" }
                    div {
                        class: "intentions-list",
                        for item in active {
                            IntentionCard {
                                data: item.clone(),
                                on_click: on_click,
                            }
                        }
                    }
                }
            }

            if !has_active {
                div {
                    class: "intentions-empty",
                    div { class: "intentions-empty-icon", "\u{2728}" }
                    div { class: "intentions-empty-text", "No active intentions yet" }
                    div { class: "intentions-empty-hint", "Create your first intention to get started" }
                }
            }

            // Fulfilled (collapsed by default)
            if has_fulfilled {
                div {
                    class: "intentions-section",
                    div {
                        class: "intentions-section-title clickable",
                        onclick: move |_| {
                            let cur = *show_fulfilled.read();
                            show_fulfilled.set(!cur);
                        },
                        if *show_fulfilled.read() { "\u{25BC}" } else { "\u{25B6}" }
                        " Fulfilled ({fulfilled_count})"
                    }
                    if *show_fulfilled.read() {
                        div {
                            class: "intentions-list",
                            for item in fulfilled {
                                IntentionCard {
                                    data: item.clone(),
                                    on_click: on_click,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Community tab — shows intentions from connected peers.
#[component]
pub fn CommunityTab(
    intentions: Vec<IntentionCardData>,
    on_click: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: "community-tab",
            if intentions.is_empty() {
                div {
                    class: "intentions-empty",
                    div { class: "intentions-empty-icon", "\u{1F310}" }
                    div { class: "intentions-empty-text", "No community intentions yet" }
                    div { class: "intentions-empty-hint", "Connect with peers to see their intentions" }
                }
            } else {
                div {
                    class: "intentions-list",
                    for item in intentions.iter() {
                        IntentionCard {
                            data: item.clone(),
                            on_click: on_click,
                        }
                    }
                }
            }
        }
    }
}

/// Single token card in the wallet view.
#[component]
fn TokenCard(data: TokenCardData) -> Element {
    let state_label = if data.pledged_to.is_some() { "Pledged" } else { "Held" };
    let state_cls = if data.pledged_to.is_some() { "token-pledged" } else { "token-available" };
    let pledged_display = data.pledged_to.clone().unwrap_or_else(|| "\u{2014} none \u{2014}".to_string());
    let short_id = if data.id.len() > 12 {
        format!("tok:{}..{}", &data.id[..4], &data.id[data.id.len()-4..])
    } else {
        format!("tok:{}", data.id)
    };

    rsx! {
        div {
            class: "token-card {state_cls}",
            // Header: icon + id + state pill
            div {
                class: "token-card-header",
                div {
                    class: "token-card-id-group",
                    span { class: "token-card-icon", "\u{1FA99}" }
                    span { class: "token-card-id", "{short_id}" }
                }
                span { class: "token-state-pill", "{state_label}" }
            }
            // Data rows
            div {
                class: "token-data-rows",
                div {
                    class: "token-data-row",
                    span { class: "token-data-key", "Backed by" }
                    span { class: "token-data-val gradient-text", "{data.attention_duration}" }
                }
                div {
                    class: "token-data-row",
                    span { class: "token-data-key", "Blesser" }
                    span {
                        class: "token-data-val",
                        span {
                            class: "token-avatar {data.blesser_color_class}",
                            "{data.blesser_letter}"
                        }
                        "{data.blesser_name}"
                    }
                }
                div {
                    class: "token-data-row",
                    span { class: "token-data-key", "Intention" }
                    span { class: "token-data-val", "{data.source_intention_title}" }
                }
                div {
                    class: "token-data-row",
                    span { class: "token-data-key", "Current holder" }
                    span {
                        class: "token-data-val",
                        span {
                            class: "token-avatar {data.current_holder_color_class}",
                            "{data.current_holder_letter}"
                        }
                        "{data.current_holder_name}"
                    }
                }
                div {
                    class: "token-data-row",
                    span { class: "token-data-key", "Pledged to" }
                    span { class: "token-data-val", "{pledged_display}" }
                }
                if !data.created_ago.is_empty() {
                    div {
                        class: "token-data-row",
                        span { class: "token-data-key", "Created" }
                        span { class: "token-data-val", "{data.created_ago}" }
                    }
                }
            }
            // Steward chain dots
            if data.steward_chain.len() > 1 {
                div {
                    class: "token-card-chain",
                    span { class: "token-card-chain-label", "Chain:" }
                    for (i, dot) in data.steward_chain.iter().enumerate() {
                        span {
                            class: "steward-chain-dot {dot.color_class}",
                            "{dot.letter}"
                        }
                        if i < data.steward_chain.len() - 1 {
                            span { class: "steward-chain-arrow", "\u{2192}" }
                        }
                    }
                }
            }
        }
    }
}

/// Tokens tab — wallet view showing earned and pledged tokens.
#[component]
pub fn TokensTab(tokens: Vec<TokenCardData>) -> Element {
    let available: Vec<_> = tokens.iter().filter(|t| t.pledged_to.is_none()).cloned().collect();
    let pledged: Vec<_> = tokens.iter().filter(|t| t.pledged_to.is_some()).cloned().collect();

    rsx! {
        div {
            class: "tokens-tab",
            if tokens.is_empty() {
                div {
                    class: "intentions-empty",
                    div { class: "intentions-empty-icon", "\u{1FA99}" }
                    div { class: "intentions-empty-text", "Token Wallet" }
                    div { class: "intentions-empty-hint", "Bless service claims to earn tokens of gratitude" }
                }
            } else {
                if !available.is_empty() {
                    div {
                        class: "tokens-section",
                        div { class: "tokens-section-title", "Available ({available.len()})" }
                        div {
                            class: "tokens-list",
                            for t in available {
                                TokenCard { data: t }
                            }
                        }
                    }
                }
                if !pledged.is_empty() {
                    div {
                        class: "tokens-section",
                        div { class: "tokens-section-title", "Pledged ({pledged.len()})" }
                        div {
                            class: "tokens-list",
                            for t in pledged {
                                TokenCard { data: t }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Main IntentionBoard component — renders tabs + active content.
#[component]
pub fn IntentionBoard(
    active_tab: DashboardTab,
    on_tab_change: EventHandler<DashboardTab>,
    my_intentions: Vec<IntentionCardData>,
    community_intentions: Vec<IntentionCardData>,
    tokens: Vec<TokenCardData>,
    on_intention_click: EventHandler<String>,
    on_create_intention: EventHandler<()>,
    chat_element: Element,
) -> Element {
    rsx! {
        div {
            class: "intention-board",
            DashboardTabs {
                active: active_tab,
                on_change: on_tab_change,
            }
            div {
                class: "dashboard-content",
                match active_tab {
                    DashboardTab::MyIntentions => rsx! {
                        MyIntentionsTab {
                            intentions: my_intentions,
                            on_click: on_intention_click,
                            on_create: on_create_intention,
                        }
                    },
                    DashboardTab::Community => rsx! {
                        CommunityTab {
                            intentions: community_intentions,
                            on_click: on_intention_click,
                        }
                    },
                    DashboardTab::Tokens => rsx! {
                        TokensTab { tokens: tokens }
                    },
                    DashboardTab::Chat => rsx! {
                        {chat_element}
                    },
                }
            }
        }
    }
}
