//! UI Components for Realm Viewer
//!
//! All Dioxus components for the dashboard views.

use dioxus::prelude::*;

use crate::playback;
use crate::state::{
    member_name, short_id, ActiveTab, AppState, ClaimInfo, QuestAttention, QuestInfo, QuestStatus,
    RealmInfo,
};
use crate::theme::{ThemeSwitcher, ThemedRoot};

/// Main application component
#[component]
pub fn App(state: Signal<AppState>) -> Element {
    rsx! {
        ThemedRoot {
            ThemeSwitcher {}
            div { class: "app-container",
                Header { state }
                main { class: "main-content",
                    TabContent { state }
                }
                EventLogPanel { state }
            }
        }
    }
}

/// Header with tab navigation and playback controls
#[component]
fn Header(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let is_paused = state.read().playback.paused;
    let speed = state.read().playback.speed;

    rsx! {
        header { class: "header",
            div { class: "header-left",
                h1 { class: "app-title", "Realm Viewer" }

                // Playback controls inline with title
                div { class: "playback-controls",
                    button {
                        class: "control-btn reset",
                        onclick: move |_| {
                            state_write.write().reset();
                            playback::reset();
                            playback::request_reset();
                        },
                        "↻"
                    }
                    button {
                        class: "control-btn play-pause",
                        onclick: move |_| {
                            let new_paused = !is_paused;
                            state_write.write().playback.paused = new_paused;
                            playback::set_paused(new_paused);
                        },
                        if is_paused { "▶" } else { "⏸" }
                    }
                    div { class: "speed-control",
                        label { "{speed}x" }
                        input {
                            r#type: "range",
                            min: "0.5",
                            max: "10",
                            step: "0.5",
                            value: "{speed}",
                            onchange: move |evt| {
                                if let Ok(v) = evt.value().parse::<f32>() {
                                    state_write.write().playback.speed = v;
                                    playback::set_speed(v);
                                }
                            },
                        }
                    }
                    button {
                        class: "control-btn step",
                        disabled: !is_paused,
                        onclick: move |_| {
                            playback::request_step();
                        },
                        "⏭"
                    }
                }
            }

            nav { class: "tab-nav",
                for tab in ActiveTab::all() {
                    button {
                        class: if state.read().active_tab == *tab { "tab-button active" } else { "tab-button" },
                        onclick: move |_| {
                            state_write.write().active_tab = *tab;
                        },
                        "{tab.display_name()}"
                    }
                }
            }

            div { class: "stats",
                span { class: "stat", "Tick: {state.read().tick}" }
                span { class: "stat", "Events: {state.read().total_events}" }
            }
        }
    }
}

/// Tab content router
#[component]
fn TabContent(state: Signal<AppState>) -> Element {
    match state.read().active_tab {
        ActiveTab::Realms => rsx! { RealmsView { state } },
        ActiveTab::Quests => rsx! { QuestsView { state } },
        ActiveTab::Attention => rsx! { AttentionView { state } },
        ActiveTab::Contacts => rsx! { ContactsView { state } },
    }
}

// ============================================================================
// REALMS VIEW
// ============================================================================

#[component]
fn RealmsView(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let realms: Vec<RealmInfo> = state_read.realms.realms_by_size().into_iter().cloned().collect();
    let count = realms.len();
    let is_empty = realms.is_empty();

    rsx! {
        div { class: "realms-view",
            div { class: "view-header",
                h2 { "Realms Overview" }
                span { class: "count", "{count} realms" }
            }
            div { class: "realm-grid",
                for realm in realms {
                    RealmCard { realm: realm.clone() }
                }
            }
            if is_empty {
                div { class: "empty-state",
                    p { "No realms yet. Waiting for realm_created events..." }
                }
            }
        }
    }
}

#[component]
fn RealmCard(realm: RealmInfo) -> Element {
    let member_names: Vec<String> = realm
        .members
        .iter()
        .take(4)
        .map(|m| member_name(m))
        .collect();
    let display_name = member_names.join(" + ");
    let extra = if realm.members.len() > 4 {
        format!(" +{}", realm.members.len() - 4)
    } else {
        String::new()
    };

    rsx! {
        div { class: "realm-card",
            div { class: "realm-name", "{display_name}{extra}" }
            div { class: "realm-id", "{short_id(&realm.realm_id)}" }
            div { class: "realm-stats",
                span { class: "member-count",
                    span { class: "dots",
                        for _ in 0..realm.members.len().min(5) {
                            span { class: "dot", "●" }
                        }
                    }
                    " {realm.members.len()} members"
                }
                span { class: "quest-count", "{realm.quest_count} quests" }
            }
        }
    }
}

// ============================================================================
// QUESTS VIEW
// ============================================================================

#[component]
fn QuestsView(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let realms: Vec<String> = state_read.quests.realms_with_quests().into_iter().cloned().collect();
    let selected = state_read.quests.selected_realm.clone();

    rsx! {
        div { class: "quests-view",
            div { class: "view-header",
                h2 { "Quest Board" }
                select {
                    class: "realm-filter",
                    value: selected.as_deref().unwrap_or(""),
                    onchange: move |evt| {
                        let value = evt.value();
                        state_write.write().quests.selected_realm = if value.is_empty() {
                            None
                        } else {
                            Some(value)
                        };
                    },
                    option { value: "", "All Realms" }
                    for realm_id in &realms {
                        option { value: "{realm_id}", "{short_id(realm_id)}" }
                    }
                }
            }
            div { class: "kanban-board",
                QuestColumn { state, status: QuestStatus::Open }
                QuestColumn { state, status: QuestStatus::Claimed }
                QuestColumn { state, status: QuestStatus::Verified }
                QuestColumn { state, status: QuestStatus::Completed }
            }
        }
    }
}

#[component]
fn QuestColumn(state: Signal<AppState>, status: QuestStatus) -> Element {
    let state_read = state.read();
    let quests: Vec<QuestInfo> = state_read.quests.quests_by_status(status).into_iter().cloned().collect();
    let count = quests.len();

    rsx! {
        div { class: "kanban-column {status.css_class()}",
            div { class: "column-header",
                h3 { "{status.display_name()}" }
                span { class: "count", "({count})" }
            }
            div { class: "column-content",
                for quest in quests {
                    QuestCard { quest: quest.clone() }
                }
            }
        }
    }
}

#[component]
fn QuestCard(quest: QuestInfo) -> Element {
    let creator_name = member_name(&quest.creator);

    rsx! {
        div { class: "quest-card",
            div { class: "quest-title", "{quest.title}" }
            div { class: "quest-creator", "by: {creator_name}" }
            if !quest.claims.is_empty() {
                div { class: "quest-claims",
                    span { "Claims: {quest.claims.len()}" }
                    div { class: "claim-badges",
                        for claim in &quest.claims {
                            ClaimBadge { claim: claim.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ClaimBadge(claim: ClaimInfo) -> Element {
    let name = member_name(&claim.claimant);
    let initial = name.chars().next().unwrap_or('?');

    rsx! {
        span {
            class: if claim.verified { "claim-badge verified" } else { "claim-badge" },
            title: "{name}",
            if claim.verified {
                "✓"
            }
            "{initial}"
        }
    }
}

// ============================================================================
// ATTENTION VIEW
// ============================================================================

#[component]
fn AttentionView(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let rankings = state_read.attention.quests_by_attention();
    let members: Vec<(String, Option<String>)> = state_read
        .attention
        .members_by_focus()
        .into_iter()
        .map(|(m, f)| (m.clone(), f.cloned()))
        .collect();
    let event_count = state_read.attention.event_count();
    let members_empty = members.is_empty();
    let rankings_empty = rankings.is_empty();

    rsx! {
        div { class: "attention-view",
            div { class: "view-header",
                h2 { "Attention Tracking" }
                span { class: "count", "{event_count} events" }
            }

            div { class: "attention-panels",
                div { class: "focus-panel",
                    h3 { "Current Focus" }
                    div { class: "focus-list",
                        for (member, focus) in &members {
                            div { class: "focus-item",
                                span { class: "member-name", "●{member_name(member)}" }
                                span { class: "focus-arrow", "→" }
                                span { class: "focus-target",
                                    if let Some(quest_id) = focus {
                                        "\"{short_id(quest_id)}\""
                                    } else {
                                        "(none)"
                                    }
                                }
                            }
                        }
                        if members_empty {
                            div { class: "empty", "No focus events yet" }
                        }
                    }
                }

                div { class: "ranking-panel",
                    h3 { "Quest Ranking by Attention" }
                    div { class: "ranking-list",
                        for (i, qa) in rankings.iter().enumerate().take(10) {
                            AttentionRankingItem { rank: i + 1, attention: qa.clone() }
                        }
                        if rankings_empty {
                            div { class: "empty", "No attention data yet" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AttentionRankingItem(rank: usize, attention: QuestAttention) -> Element {
    let secs = attention.total_attention_ms as f64 / 1000.0;
    let max_width = 200.0;
    // Simple scaling - assume 60s is full width
    let bar_width = (secs / 60.0 * max_width).min(max_width);

    let focusing_str = if attention.currently_focusing.is_empty() {
        String::from("(no one focusing)")
    } else {
        attention
            .currently_focusing
            .iter()
            .map(|m| format!("●{}", member_name(m)))
            .collect::<Vec<_>>()
            .join(" ")
    };

    rsx! {
        div { class: "ranking-item",
            span { class: "rank", "{rank}." }
            div { class: "ranking-details",
                div { class: "quest-name", "{short_id(&attention.quest_id)}" }
                div { class: "attention-bar",
                    div {
                        class: "bar-fill",
                        style: "width: {bar_width}px",
                    }
                    span { class: "attention-value", "{secs:.1}s" }
                }
                div { class: "focusing", "{focusing_str}" }
            }
        }
    }
}

// ============================================================================
// CONTACTS VIEW
// ============================================================================

#[component]
fn ContactsView(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let members: Vec<String> = state_read.contacts.all_members().into_iter().collect();
    let selected = state_read.contacts.selected_member.clone();

    rsx! {
        div { class: "contacts-view",
            div { class: "view-header",
                h2 { "Contacts Network" }
                span { class: "count",
                    "{state_read.contacts.member_count()} members, {state_read.contacts.contact_count()} contacts"
                }
            }

            div { class: "contacts-panels",
                div { class: "contacts-list-panel",
                    h3 { "Members" }
                    div { class: "member-list",
                        for member in &members {
                            MemberListItem {
                                state,
                                member_id: member.clone(),
                                selected: selected.as_ref() == Some(member)
                            }
                        }
                        if members.is_empty() {
                            div { class: "empty", "No contacts yet" }
                        }
                    }
                }

                div { class: "contacts-detail-panel",
                    if let Some(ref selected_id) = selected {
                        MemberDetails { state, member_id: selected_id.clone() }
                    } else {
                        div { class: "empty", "Select a member to view details" }
                    }
                }

                div { class: "contacts-matrix-panel",
                    h3 { "Contacts Matrix" }
                    ContactsMatrix { state }
                }
            }
        }
    }
}

#[component]
fn MemberListItem(state: Signal<AppState>, member_id: String, selected: bool) -> Element {
    let mut state_write = state;
    let member_id_clone = member_id.clone();

    rsx! {
        div {
            class: if selected { "member-item selected" } else { "member-item" },
            onclick: move |_| {
                state_write.write().contacts.selected_member = Some(member_id_clone.clone());
            },
            span { class: "member-name", "●{member_name(&member_id)}" }
        }
    }
}

#[component]
fn MemberDetails(state: Signal<AppState>, member_id: String) -> Element {
    let state_read = state.read();
    let contacts = state_read.contacts.get_contacts(&member_id);
    let contacted_by = state_read.contacts.get_contacted_by(&member_id);

    rsx! {
        div { class: "member-details",
            h3 { "Selected: {member_name(&member_id)}" }

            div { class: "detail-section",
                h4 { "Contacts: {contacts.len()}" }
                ul {
                    for contact in contacts {
                        li { "- {member_name(contact)}" }
                    }
                }
            }

            div { class: "detail-section",
                h4 { "Contacted by: {contacted_by.len()}" }
                ul {
                    for by in contacted_by {
                        li { "- {member_name(by)}" }
                    }
                }
            }
        }
    }
}

#[component]
fn ContactsMatrix(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let mut members: Vec<String> = state_read.contacts.all_members().into_iter().collect();
    members.sort();

    if members.len() > 10 {
        // Limit matrix size for readability
        return rsx! {
            div { class: "matrix-overflow",
                "Matrix too large ({members.len()} members). Select a member for details."
            }
        };
    }

    rsx! {
        div { class: "contacts-matrix",
            table {
                thead {
                    tr {
                        th { "" }
                        for m in &members {
                            th { "{member_name(m).chars().next().unwrap_or('?')}" }
                        }
                    }
                }
                tbody {
                    for row in &members {
                        tr {
                            th { "{member_name(row).chars().next().unwrap_or('?')}" }
                            for col in &members {
                                td {
                                    if row == col {
                                        "-"
                                    } else if state_read.contacts.contacts.get(row).map(|c| c.contains(col)).unwrap_or(false) {
                                        "✓"
                                    } else {
                                        ""
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// EVENT LOG PANEL
// ============================================================================

#[component]
fn EventLogPanel(state: Signal<AppState>) -> Element {
    let events = &state.read().event_log;

    rsx! {
        div { class: "event-log-panel",
            div { class: "panel-header",
                h3 { "Event Log" }
            }
            div { class: "event-list",
                for event in events.iter().take(20) {
                    div { class: "event-item {event.category.css_class()}",
                        span { class: "event-tick", "[{event.tick}]" }
                        span { class: "event-type", "{event.type_name}" }
                        span { class: "event-summary", "{event.summary}" }
                    }
                }
            }
        }
    }
}

