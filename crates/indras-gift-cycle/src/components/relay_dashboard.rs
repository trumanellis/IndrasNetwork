//! Relay node dashboard — shows relay health, tier storage, connected peers.

use dioxus::prelude::*;

use crate::data::{RelayOverview, RelayPeerData};

/// Relay node dashboard component.
#[component]
pub fn RelayDashboard(
    data: Option<RelayOverview>,
    on_back: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "relay-dashboard",
            // Header
            div { class: "feed-header",
                h2 { class: "feed-title", "Relay Node" }
                button {
                    class: "gc-btn gc-btn-outline",
                    onclick: move |_| on_back.call(()),
                    "\u{2190} Back"
                }
            }

            if let Some(ref d) = data {
                // SECTION 1: Stats strip — 4 metric cards
                div { class: "relay-stats-grid",
                    div { class: "relay-stat-card",
                        div { class: "relay-stat-value", "{d.peer_count}" }
                        div { class: "relay-stat-label", "Peers" }
                        div { class: "relay-stat-sub", "{d.contact_count} contacts" }
                    }
                    div { class: "relay-stat-card",
                        div { class: "relay-stat-value", "{d.interface_count}" }
                        div { class: "relay-stat-label", "Interfaces" }
                    }
                    div { class: "relay-stat-card",
                        div { class: "relay-stat-value", "{d.total_events}" }
                        div { class: "relay-stat-label", "Events" }
                    }
                    div { class: "relay-stat-card",
                        div { class: "relay-stat-value", "{d.total_storage_label}" }
                        div { class: "relay-stat-label", "Storage" }
                        div { class: "relay-stat-sub", "{d.global_quota_label}" }
                    }
                }

                // SECTION 2: Three staging areas
                div { class: "relay-section-header", "Staging Areas" }
                div { class: "relay-tiers-grid",
                    for tier in &d.tiers {
                        div {
                            class: "relay-tier-card",
                            style: "border-top: 2px solid var({tier.color_var})",
                            div { class: "relay-tier-name", "{tier.name}" }
                            div { class: "relay-tier-usage", "{tier.used_label}" }
                            div { class: "relay-tier-bar",
                                div {
                                    class: "relay-tier-fill",
                                    style: "width: {tier.usage_pct * 100.0:.0}%; background: var({tier.color_var})",
                                }
                            }
                            div { class: "relay-tier-meta",
                                "TTL: {tier.ttl_days}d \u{00b7} Max: {tier.max_interfaces} ifaces"
                            }
                            div { class: "relay-tier-desc", "{tier.description}" }
                        }
                    }
                }

                // SECTION 3: Contacts
                div { class: "relay-section-header",
                    span { "Contacts" }
                    span { class: "section-count", "{d.contact_count}" }
                }
                if d.contacts.is_empty() {
                    div { class: "feed-empty", "No contacts connected yet" }
                }
                for peer in &d.contacts {
                    {render_peer_row(peer)}
                }

                // SECTION 4: Public peers
                if !d.public_peers.is_empty() {
                    div { class: "relay-section-header",
                        span { "Public Peers" }
                        span { class: "section-count", "{d.public_peers.len()}" }
                    }
                    for peer in &d.public_peers {
                        {render_peer_row(peer)}
                    }
                }

                // SECTION 5: Interfaces
                if !d.interfaces.is_empty() {
                    div { class: "relay-section-header",
                        span { "Interfaces" }
                        span { class: "section-count", "{d.interface_count}" }
                    }
                    for iface in &d.interfaces {
                        div { class: "relay-iface-row",
                            span { class: "relay-iface-id", "{iface.id_short}" }
                            span { "{iface.event_count} events" }
                            span { "{iface.storage_label}" }
                        }
                    }
                }

                // SECTION 6: Quotas
                div { class: "relay-quota-card",
                    div { "Global: {d.global_quota_label}" }
                    div { "Per-peer: {d.per_peer_limit_label}" }
                    div { "Cleanup: {d.cleanup_interval_label}" }
                }
            } else {
                div { class: "feed-empty", "Relay service starting\u{2026}" }
            }
        }
    }
}

fn render_peer_row(peer: &RelayPeerData) -> Element {
    let tier_class = match peer.tier_label.as_str() {
        "Connections" => "status-badge status-proven",
        "Self" => "status-badge status-verified",
        _ => "status-badge status-open",
    };
    rsx! {
        div { class: "relay-peer-row",
            div { class: "peer-avatar-sm {peer.color_class}", "{peer.letter}" }
            div { class: "relay-peer-name", "{peer.display_name}" }
            span { class: "{tier_class}", "{peer.tier_label}" }
            div { class: "relay-peer-meta",
                span { "{peer.interface_count} ifaces" }
                span { "{peer.event_count} events" }
                span { "{peer.storage_label}" }
                span { "{peer.last_seen_ago}" }
            }
        }
    }
}
