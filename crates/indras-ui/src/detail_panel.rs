//! Detail panel with tabbed views for Properties, Audience, and Heat.

use dioxus::prelude::*;
use crate::heat_display::HeatBar;

/// Property row data for the Properties tab.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyRow {
    pub key: String,
    pub value: String,
    pub accent: bool,
}

/// A reference item shown in the Properties tab.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceItem {
    pub icon: String,
    pub name: String,
    pub ref_type: String,
}

/// Audience member display data.
#[derive(Clone, Debug, PartialEq)]
pub struct AudienceMember {
    pub name: String,
    pub letter: String,
    pub color_class: String,
    pub role: String,
    pub short_id: String,
}

/// Sync status entry.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncEntry {
    pub name: String,
    pub status: String,
    pub status_text: String,
}

/// Heat bar data for the Heat tab.
#[derive(Clone, Debug, PartialEq)]
pub struct HeatEntry {
    pub label: String,
    pub value: f32,
    pub color: String,
}

/// Attention trail event.
#[derive(Clone, Debug, PartialEq)]
pub struct TrailEvent {
    pub time: String,
    pub target: String,
}

/// Detail panel with 3 tabs: Properties, Audience, Heat.
#[component]
pub fn DetailPanel(
    active_tab: usize,
    on_tab_change: EventHandler<usize>,
    on_close: EventHandler<()>,
    properties: Vec<PropertyRow>,
    audience: Vec<AudienceMember>,
    heat_entries: Vec<HeatEntry>,
    trail_events: Vec<TrailEvent>,
    #[props(default = Vec::new())]
    references: Vec<ReferenceItem>,
    #[props(default = String::new())]
    artifact_id_display: String,
    #[props(default = 0)]
    refs_count: usize,
    #[props(default = String::new())]
    steward_name: String,
    #[props(default = String::new())]
    steward_letter: String,
    #[props(default = false)]
    is_own_steward: bool,
    #[props(default = Vec::new())]
    sync_entries: Vec<SyncEntry>,
    #[props(default = 0.0)]
    combined_heat: f32,
) -> Element {
    let tab_names = ["Properties", "Audience", "Heat"];

    rsx! {
        div {
            class: "detail-panel",

            div {
                class: "detail-header",
                span { class: "detail-title", "Artifact Properties" }
                button {
                    class: "detail-close",
                    onclick: move |_| on_close.call(()),
                    "\u{2715}"
                }
            }

            div {
                class: "detail-tabs",
                for (i, tab_name) in tab_names.iter().enumerate() {
                    {
                        let active_class = if i == active_tab { " active" } else { "" };
                        rsx! {
                            button {
                                class: "detail-tab{active_class}",
                                onclick: move |_| on_tab_change.call(i),
                                "{tab_name}"
                            }
                        }
                    }
                }
            }

            // Properties tab
            if active_tab == 0 {
                div {
                    class: "detail-tab-content active",
                    // Identity section
                    div {
                        class: "detail-section",
                        div { class: "detail-section-title", "Identity" }
                        for prop in properties.iter() {
                            div {
                                class: "prop-row",
                                span { class: "prop-key", "{prop.key}" }
                                span {
                                    class: if prop.accent { "prop-val accent" } else { "prop-val" },
                                    "{prop.value}"
                                }
                            }
                        }
                        if !artifact_id_display.is_empty() {
                            div {
                                class: "prop-row",
                                span { class: "prop-key", "ID" }
                                span { class: "prop-val", "{artifact_id_display}" }
                            }
                        }
                        if refs_count > 0 {
                            div {
                                class: "prop-row",
                                span { class: "prop-key", "Refs" }
                                span { class: "prop-val", "{refs_count} artifacts" }
                            }
                        }
                    }
                    // Recent Attention section
                    if !trail_events.is_empty() {
                        div {
                            class: "detail-section",
                            div { class: "detail-section-title", "Recent Attention" }
                            for event in trail_events.iter() {
                                div {
                                    class: "trail-event",
                                    span { class: "trail-time", "{event.time}" }
                                    span { class: "trail-arrow", "\u{25B6}" }
                                    span { class: "trail-target", "{event.target}" }
                                }
                            }
                        }
                    }
                    // References section
                    if !references.is_empty() {
                        div {
                            class: "detail-section",
                            div { class: "detail-section-title", "References" }
                            for (i, ref_item) in references.iter().enumerate() {
                                if i < 3 {
                                    div {
                                        class: "ref-item",
                                        span { class: "ref-icon", "{ref_item.icon}" }
                                        span { class: "ref-name", "{ref_item.name}" }
                                        span { class: "ref-type", "{ref_item.ref_type}" }
                                    }
                                }
                            }
                            if references.len() > 3 {
                                div {
                                    style: "color:var(--text-ghost);font-size:11px;padding:4px 8px 4px 38px",
                                    "+ {references.len() - 3} more..."
                                }
                            }
                        }
                    }
                }
            }

            // Audience tab
            if active_tab == 1 {
                div {
                    class: "detail-tab-content active",
                    // Search bar
                    div {
                        class: "audience-search",
                        input {
                            class: "audience-search-input",
                            placeholder: "Search peers to add...",
                        }
                    }

                    // Stewardship transfer
                    if !steward_name.is_empty() {
                        div {
                            class: "steward-transfer",
                            div { class: "steward-transfer-title", "Stewardship" }
                            div {
                                class: "steward-current",
                                div {
                                    class: "audience-dot",
                                    style: "background:linear-gradient(135deg,var(--accent-teal),var(--accent-violet))",
                                    "{steward_letter}"
                                }
                                div {
                                    class: "steward-current-info",
                                    div { class: "steward-current-label", "Current Steward" }
                                    div {
                                        class: "steward-current-name",
                                        if is_own_steward {
                                            "{steward_name} (you)"
                                        } else {
                                            "{steward_name}"
                                        }
                                    }
                                }
                            }
                            button { class: "transfer-btn", "\u{1F504} Transfer Stewardship..." }
                        }
                    }

                    // Audience members
                    for member in audience.iter() {
                        div {
                            class: "audience-member",
                            div {
                                class: "audience-member-avatar {member.color_class}",
                                style: if member.role == "steward" {
                                    "background:linear-gradient(135deg,var(--accent-teal),var(--accent-violet));color:var(--bg-void)"
                                } else { "" },
                                "{member.letter}"
                            }
                            div {
                                class: "audience-member-info",
                                div { class: "audience-member-name", "{member.name}" }
                                if !member.short_id.is_empty() {
                                    div { class: "audience-member-id", "{member.short_id}" }
                                }
                            }
                            div {
                                class: "audience-member-actions",
                                span { class: "audience-badge {member.role}", "{member.role}" }
                                if member.role != "steward" {
                                    button {
                                        class: "audience-remove-btn",
                                        title: "Remove from audience",
                                        "\u{2715}"
                                    }
                                }
                            }
                        }
                    }

                    // Add peer row
                    div {
                        class: "audience-add",
                        div { class: "audience-add-icon", "+" }
                        div { class: "audience-add-text", "Add peer to audience..." }
                    }

                    // Sync status
                    if !sync_entries.is_empty() {
                        div {
                            class: "sync-section",
                            div { class: "detail-section-title", "Sync Status" }
                            for entry in sync_entries.iter() {
                                div {
                                    class: "sync-item",
                                    div { class: "sync-dot {entry.status}" }
                                    span { class: "sync-name", "{entry.name}" }
                                    span { class: "sync-status", "{entry.status_text}" }
                                }
                            }
                        }
                    }
                }
            }

            // Heat tab
            if active_tab == 2 {
                {
                    let combined_pct = (combined_heat.clamp(0.0, 1.0) * 100.0) as u32;
                    let combined_display = format!("{:.2}", combined_heat);
                    let peer_count = heat_entries.len();
                    rsx! {
                        div {
                            class: "detail-tab-content active",
                            div {
                                class: "detail-section",
                                div { class: "detail-section-title", "Per-Peer Heat" }
                                div {
                                    class: "heat-viz",
                                    for entry in heat_entries.iter() {
                                        HeatBar {
                                            label: entry.label.clone(),
                                            value: entry.value,
                                            color: Some(entry.color.clone()),
                                        }
                                    }
                                    // Combined heat bar
                                    div {
                                        class: "heat-bar-row",
                                        style: "margin-top:6px;padding-top:8px;border-top:1px solid var(--border-subtle)",
                                        span {
                                            class: "heat-bar-label",
                                            style: "font-weight:500;color:var(--text-secondary)",
                                            "Combined"
                                        }
                                        div {
                                            class: "heat-bar-track",
                                            style: "height:6px",
                                            div {
                                                class: "heat-bar-fill",
                                                style: "width: {combined_pct}%; background: linear-gradient(90deg, var(--heat-2), var(--heat-5))",
                                            }
                                        }
                                        span {
                                            class: "heat-bar-value",
                                            style: "color:var(--heat-3);font-weight:500",
                                            "{combined_display}"
                                        }
                                    }
                                }
                            }
                            // Heat Computation section
                            div {
                                class: "detail-section",
                                div { class: "detail-section-title", "Heat Computation" }
                                div {
                                    class: "prop-row",
                                    span { class: "prop-key", "Unique peers" }
                                    span { class: "prop-val", "{peer_count} / {peer_count}" }
                                }
                                div {
                                    class: "prop-row",
                                    span { class: "prop-key", "Total dwell" }
                                    span { class: "prop-val", "14m 22s" }
                                }
                                div {
                                    class: "prop-row",
                                    span { class: "prop-key", "Recency weight" }
                                    span { class: "prop-val", "0.78" }
                                }
                                div {
                                    class: "prop-row",
                                    span { class: "prop-key", "Density score" }
                                    span { class: "prop-val", "{combined_display}" }
                                }
                            }
                            // CSS Output section
                            div {
                                class: "detail-section",
                                div { class: "detail-section-title", "CSS Output" }
                                div {
                                    style: "font-family:var(--font-mono);font-size:12px;color:var(--text-secondary);padding:10px 14px;background:var(--bg-surface);border:1px solid var(--border-dim);border-radius:var(--radius-md);line-height:1.6",
                                    span { style: "color:var(--accent-violet)", "--heat" }
                                    ": "
                                    span { style: "color:var(--heat-3)", "{combined_display}" }
                                    ";"
                                    br {}
                                    span { style: "color:var(--accent-violet)", "--heat-color" }
                                    ": "
                                    span { style: "color:var(--heat-3)", "rgb(200, 168, 47)" }
                                    ";"
                                    br {}
                                    span { style: "color:var(--accent-violet)", "--heat-glow" }
                                    ": "
                                    span { style: "color:var(--heat-3)", "0 0 8px rgba(200,168,47,0.3)" }
                                    ";"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
