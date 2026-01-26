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
    let is_pov_mode = state.read().is_pov_mode();

    rsx! {
        header { class: if is_pov_mode { "header pov-mode" } else { "header" },
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

                // POV Selector
                POVSelector { state }
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

/// POV (Point of View) selector dropdown
#[component]
fn POVSelector(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let members = state.read().all_members();
    let selected = state.read().selected_pov.clone();
    let is_pov_mode = selected.is_some();

    rsx! {
        div { class: if is_pov_mode { "pov-selector active" } else { "pov-selector" },
            span { class: "pov-icon", "👁" }
            select {
                class: "pov-dropdown",
                value: selected.as_deref().unwrap_or(""),
                onchange: move |evt| {
                    let value = evt.value();
                    state_write.write().selected_pov = if value.is_empty() {
                        None
                    } else {
                        Some(value)
                    };
                },
                option { value: "", "All Members" }
                for member in members {
                    option { value: "{member}", "{member_name(&member)}" }
                }
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
    let pov = state_read.selected_pov.clone();
    let realms: Vec<RealmInfo> = if let Some(ref pov_member) = pov {
        state_read.realms.realms_for_member(pov_member).into_iter().cloned().collect()
    } else {
        state_read.realms.realms_by_size().into_iter().cloned().collect()
    };
    let count = realms.len();
    let is_empty = realms.is_empty();
    let pov_name = pov.as_ref().map(|m| member_name(m));

    rsx! {
        div { class: "realms-view",
            div { class: "view-header",
                h2 {
                    if let Some(ref name) = pov_name {
                        "{name}'s Realms"
                    } else {
                        "Realms Overview"
                    }
                }
                span { class: "count", "{count} realms" }
            }
            div { class: "realm-grid",
                for realm in realms {
                    RealmCard { realm: realm.clone(), pov: pov.clone() }
                }
            }
            if is_empty {
                div { class: "empty-state",
                    if pov_name.is_some() {
                        p { "This member is not in any realms yet." }
                    } else {
                        p { "No realms yet. Waiting for realm_created events..." }
                    }
                }
            }
        }
    }
}

#[component]
fn RealmCard(realm: RealmInfo, pov: Option<String>) -> Element {
    // In POV mode, show the POV member first
    let mut sorted_members: Vec<&String> = realm.members.iter().collect();
    if let Some(ref pov_member) = pov {
        sorted_members.sort_by(|a, b| {
            if *a == pov_member { std::cmp::Ordering::Less }
            else if *b == pov_member { std::cmp::Ordering::Greater }
            else { a.cmp(b) }
        });
    }

    let member_names: Vec<String> = sorted_members
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
    let pov = state_read.selected_pov.clone();
    let pov_name = pov.as_ref().map(|m| member_name(m));

    rsx! {
        div { class: "quests-view",
            div { class: "view-header",
                h2 {
                    if let Some(ref name) = pov_name {
                        "{name}'s Quests"
                    } else {
                        "Quest Board"
                    }
                }
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
    let pov = state_read.selected_pov.clone();
    let quests: Vec<QuestInfo> = if let Some(ref pov_member) = pov {
        state_read.quests.quests_for_member_by_status(pov_member, status).into_iter().cloned().collect()
    } else {
        state_read.quests.quests_by_status(status).into_iter().cloned().collect()
    };
    let count = quests.len();

    rsx! {
        div { class: "kanban-column {status.css_class()}",
            div { class: "column-header",
                h3 { "{status.display_name()}" }
                span { class: "count", "({count})" }
            }
            div { class: "column-content",
                for quest in quests {
                    QuestCard { quest: quest.clone(), pov: pov.clone() }
                }
            }
        }
    }
}

#[component]
fn QuestCard(quest: QuestInfo, pov: Option<String>) -> Element {
    let creator_name = member_name(&quest.creator);

    // Determine the POV member's role in this quest
    let role = pov.as_ref().map(|pov_member| {
        if quest.creator == *pov_member {
            "creator"
        } else if quest.claims.iter().any(|c| c.claimant == *pov_member) {
            "claimant"
        } else {
            ""
        }
    }).unwrap_or("");

    rsx! {
        div { class: "quest-card",
            div { class: "quest-header",
                div { class: "quest-title", "{quest.title}" }
                if !role.is_empty() {
                    span { class: "role-badge {role}", "{role}" }
                }
            }
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
    let pov = state_read.selected_pov.clone();
    let pov_name = pov.as_ref().map(|m| member_name(m));

    // In POV mode, filter rankings to member's quests
    let rankings = if let Some(ref pov_member) = pov {
        state_read.attention.attention_for_member(pov_member)
    } else {
        state_read.attention.quests_by_attention()
    };

    // In POV mode, only show the POV member's focus prominently
    let members: Vec<(String, Option<String>)> = if let Some(ref pov_member) = pov {
        // Show POV member first, then others
        let mut all: Vec<(String, Option<String>)> = state_read
            .attention
            .members_by_focus()
            .into_iter()
            .map(|(m, f)| (m.clone(), f.cloned()))
            .collect();
        all.sort_by(|a, b| {
            if a.0 == *pov_member { std::cmp::Ordering::Less }
            else if b.0 == *pov_member { std::cmp::Ordering::Greater }
            else { a.0.cmp(&b.0) }
        });
        all
    } else {
        state_read
            .attention
            .members_by_focus()
            .into_iter()
            .map(|(m, f)| (m.clone(), f.cloned()))
            .collect()
    };

    let event_count = state_read.attention.event_count();
    let members_empty = members.is_empty();
    let rankings_empty = rankings.is_empty();

    // Get POV member's current focus for highlight
    let pov_focus = pov.as_ref().and_then(|m| {
        state_read.attention.focus_for_member(m).cloned()
    });

    rsx! {
        div { class: "attention-view",
            div { class: "view-header",
                h2 {
                    if let Some(ref name) = pov_name {
                        "{name}'s Attention"
                    } else {
                        "Attention Tracking"
                    }
                }
                span { class: "count", "{event_count} events" }
            }

            // In POV mode, show "Your Focus" prominently at top
            if let Some(ref _pov_member) = pov {
                div { class: "pov-focus-highlight",
                    span { class: "pov-focus-label", "Your Focus:" }
                    span { class: "pov-focus-value",
                        if let Some(ref quest_id) = pov_focus {
                            "\"{short_id(quest_id)}\""
                        } else {
                            "(none)"
                        }
                    }
                }
            }

            div { class: "attention-panels",
                div { class: "focus-panel",
                    h3 {
                        if pov.is_some() { "All Focus" } else { "Current Focus" }
                    }
                    div { class: "focus-list",
                        for (member, focus) in &members {
                            div {
                                class: if pov.as_ref() == Some(member) { "focus-item highlighted" } else { "focus-item" },
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
                    h3 {
                        if pov.is_some() { "Your Quest Rankings" } else { "Quest Ranking by Attention" }
                    }
                    div { class: "ranking-list",
                        for (i, qa) in rankings.iter().enumerate().take(10) {
                            AttentionRankingItem { rank: i + 1, attention: qa.clone(), pov: pov.clone() }
                        }
                        if rankings_empty {
                            div { class: "empty",
                                if pov.is_some() {
                                    "No attention data for your quests yet"
                                } else {
                                    "No attention data yet"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AttentionRankingItem(rank: usize, attention: QuestAttention, pov: Option<String>) -> Element {
    let secs = attention.total_attention_ms as f64 / 1000.0;
    let max_width = 200.0;
    // Simple scaling - assume 60s is full width
    let bar_width = (secs / 60.0 * max_width).min(max_width);

    // Check if POV member is currently focusing on this quest
    let pov_is_focusing = pov.as_ref()
        .map(|m| attention.currently_focusing.contains(m))
        .unwrap_or(false);

    let focusing_str = if attention.currently_focusing.is_empty() {
        String::from("(no one focusing)")
    } else {
        attention
            .currently_focusing
            .iter()
            .map(|m| {
                let name = member_name(m);
                if pov.as_ref() == Some(m) {
                    format!("●{} (you)", name)
                } else {
                    format!("●{}", name)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    rsx! {
        div { class: if pov_is_focusing { "ranking-item highlighted" } else { "ranking-item" },
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
    let mut state_write = state;
    let state_read = state.read();
    let pov = state_read.selected_pov.clone();
    let pov_name = pov.as_ref().map(|m| member_name(m));

    // In POV mode, filter to relevant members (POV member + their contacts)
    let members: Vec<String> = if let Some(ref pov_member) = pov {
        let mut related = state_read.contacts.contacts_for_member(pov_member);
        // Always include the POV member themselves
        if !related.contains(pov_member) {
            related.insert(0, pov_member.clone());
        } else {
            // Move POV member to front
            related.retain(|m| m != pov_member);
            related.insert(0, pov_member.clone());
        }
        related
    } else {
        let mut all: Vec<String> = state_read.contacts.all_members().into_iter().collect();
        all.sort();
        all
    };

    // In POV mode, auto-select the POV member if nothing selected
    let selected = if let Some(ref pov_member) = pov {
        if state_read.contacts.selected_member.is_none() {
            // Auto-select POV member (need to trigger update)
            Some(pov_member.clone())
        } else {
            state_read.contacts.selected_member.clone()
        }
    } else {
        state_read.contacts.selected_member.clone()
    };

    // Auto-select POV member when entering POV mode
    let pov_for_effect = pov.clone();
    use_effect(move || {
        if let Some(ref pov_member) = pov_for_effect {
            state_write.write().contacts.selected_member = Some(pov_member.clone());
        }
    });

    rsx! {
        div { class: "contacts-view",
            div { class: "view-header",
                h2 {
                    if let Some(ref name) = pov_name {
                        "{name}'s Network"
                    } else {
                        "Contacts Network"
                    }
                }
                span { class: "count",
                    if pov.is_some() {
                        "{members.len()} connections"
                    } else {
                        "{state_read.contacts.member_count()} members, {state_read.contacts.contact_count()} contacts"
                    }
                }
            }

            div { class: "contacts-panels",
                div { class: "contacts-list-panel",
                    h3 {
                        if pov.is_some() { "Your Network" } else { "Members" }
                    }
                    div { class: "member-list",
                        for member in &members {
                            MemberListItem {
                                state,
                                member_id: member.clone(),
                                selected: selected.as_ref() == Some(member),
                                is_pov: pov.as_ref() == Some(member)
                            }
                        }
                        if members.is_empty() {
                            div { class: "empty", "No contacts yet" }
                        }
                    }
                }

                div { class: "contacts-detail-panel",
                    if let Some(ref selected_id) = selected {
                        MemberDetails { state, member_id: selected_id.clone(), pov: pov.clone() }
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
fn MemberListItem(state: Signal<AppState>, member_id: String, selected: bool, is_pov: bool) -> Element {
    let mut state_write = state;
    let member_id_clone = member_id.clone();

    let class = if selected && is_pov {
        "member-item selected pov-member"
    } else if selected {
        "member-item selected"
    } else if is_pov {
        "member-item pov-member"
    } else {
        "member-item"
    };

    rsx! {
        div {
            class: class,
            onclick: move |_| {
                state_write.write().contacts.selected_member = Some(member_id_clone.clone());
            },
            span { class: "member-name", "●{member_name(&member_id)}" }
            if is_pov {
                span { class: "pov-badge", "(you)" }
            }
        }
    }
}

#[component]
fn MemberDetails(state: Signal<AppState>, member_id: String, pov: Option<String>) -> Element {
    let state_read = state.read();
    let contacts = state_read.contacts.get_contacts(&member_id);
    let contacted_by = state_read.contacts.get_contacted_by(&member_id);
    let is_pov_member = pov.as_ref() == Some(&member_id);

    // Determine relationship labels based on POV
    let (contacts_label, contacted_by_label) = if is_pov_member {
        ("Your Contacts", "Who Added You")
    } else if pov.is_some() {
        ("Their Contacts", "Who Added Them")
    } else {
        ("Contacts", "Contacted by")
    };

    rsx! {
        div { class: "member-details",
            h3 {
                if is_pov_member {
                    "Your Profile"
                } else {
                    "{member_name(&member_id)}"
                }
            }

            div { class: "detail-section",
                h4 { "{contacts_label}: {contacts.len()}" }
                ul {
                    for contact in contacts {
                        li {
                            class: if pov.as_ref() == Some(contact) { "pov-highlight" } else { "" },
                            "- {member_name(contact)}"
                            if pov.as_ref() == Some(contact) {
                                " (you)"
                            }
                        }
                    }
                }
            }

            div { class: "detail-section",
                h4 { "{contacted_by_label}: {contacted_by.len()}" }
                ul {
                    for by in contacted_by {
                        li {
                            class: if pov.as_ref() == Some(by) { "pov-highlight" } else { "" },
                            "- {member_name(by)}"
                            if pov.as_ref() == Some(by) {
                                " (you)"
                            }
                        }
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

