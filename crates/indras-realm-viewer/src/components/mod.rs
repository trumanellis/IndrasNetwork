//! UI Components for Realm Viewer
//!
//! Redesigned 3-panel dashboard with frosted glass controls.

use dioxus::prelude::*;

use crate::playback;
use crate::state::{
    member_name, short_id, AppState, ClaimInfo, MemberStats, QuestAttention, QuestInfo,
    QuestStatus, RealmInfo,
};
use crate::theme::{ThemeSwitcher, ThemedRoot};

/// Main application component
#[component]
pub fn App(state: Signal<AppState>) -> Element {
    let is_pov_mode = state.read().selected_pov.is_some();

    if is_pov_mode {
        rsx! {
            POVDashboard { state }
        }
    } else {
        rsx! {
            ThemedRoot {
                ThemeSwitcher {}
                div { class: "app-container",
                    Header { state }
                    main { class: "main-content",
                        LeftPanel { state }
                        CenterPanel { state }
                        RightPanel { state }
                    }
                }
                FloatingControlBar { state }
            }
        }
    }
}

/// Simplified header for Overview mode
#[component]
fn Header(state: Signal<AppState>) -> Element {
    let tick = state.read().tick;
    let total_events = state.read().total_events;
    let realm_count = state.read().realms.realms.len();
    let member_count = state.read().all_members().len();

    rsx! {
        header { class: "header",
            div { class: "header-left",
                h1 { class: "app-title", "Realm Viewer" }
            }
            div { class: "header-stats",
                span { class: "stat", "Tick: ", span { class: "stat-value", "{tick}" } }
                span { class: "stat", "Events: ", span { class: "stat-value", "{total_events}" } }
                span { class: "stat", "Realms: ", span { class: "stat-value", "{realm_count}" } }
                span { class: "stat", "Members: ", span { class: "stat-value", "{member_count}" } }
            }
        }
    }
}

// ============================================================================
// LEFT PANEL - Realms & Members
// ============================================================================

#[component]
fn LeftPanel(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "left-panel",
            RealmsSection { state }
            MembersSection { state }
            NetworkTopology { state }
        }
    }
}

#[component]
fn RealmsSection(state: Signal<AppState>) -> Element {
    let realms: Vec<RealmInfo> = state.read().realms.realms_by_size().into_iter().cloned().collect();
    let count = realms.len();

    rsx! {
        div { class: "panel-section",
            div { class: "panel-title",
                "Realms"
                span { class: "panel-count", "{count}" }
            }
            div { class: "realm-list",
                for realm in realms.iter().take(10) {
                    RealmCard { realm: realm.clone() }
                }
                if realms.is_empty() {
                    div { class: "empty-state", "No realms yet" }
                }
            }
        }
    }
}

#[component]
fn RealmCard(realm: RealmInfo) -> Element {
    let member_names: Vec<String> = realm.members.iter()
        .take(3)
        .map(|m| member_name(m))
        .collect();
    let display_name = if member_names.is_empty() {
        short_id(&realm.realm_id)
    } else {
        member_names.join(" + ")
    };
    let extra = if realm.members.len() > 3 {
        format!(" +{}", realm.members.len() - 3)
    } else {
        String::new()
    };

    rsx! {
        div { class: "realm-card",
            div { class: "realm-name", "{display_name}{extra}" }
            div { class: "realm-meta",
                div { class: "realm-members-count",
                    div { class: "member-dots",
                        for _ in 0..realm.members.len().min(5) {
                            span { class: "member-dot" }
                        }
                    }
                    "{realm.members.len()} members"
                }
                span { "{realm.quest_count} quests" }
            }
        }
    }
}

#[component]
fn MembersSection(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let members = state_read.all_members();
    let count = members.len();

    // Get current focus for each member
    let members_with_focus: Vec<(String, Option<String>)> = members
        .into_iter()
        .map(|m| {
            let focus = state_read.attention.focus_for_member(&m).cloned();
            (m, focus)
        })
        .collect();

    rsx! {
        div { class: "panel-section",
            div { class: "panel-title",
                "Members"
                span { class: "panel-count", "{count}" }
            }
            div { class: "member-list",
                for (member, focus) in members_with_focus.iter() {
                    MemberCard {
                        state,
                        member_id: member.clone(),
                        focus: focus.clone()
                    }
                }
                if count == 0 {
                    div { class: "empty-state", "No members yet" }
                }
            }
        }
    }
}

#[component]
fn MemberCard(state: Signal<AppState>, member_id: String, focus: Option<String>) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let name = member_name(&member_id);
    let initial = name.chars().next().unwrap_or('?');
    let color_class = member_color_class(&member_id);
    let member_id_clone = member_id.clone();

    // Look up quest title if focusing
    let focus_title = focus.as_ref().and_then(|qid| {
        state_read.quests.quests.get(qid).map(|q| q.title.clone())
    });

    rsx! {
        div {
            class: "member-card clickable",
            onclick: move |_| {
                state_write.write().selected_pov = Some(member_id_clone.clone());
            },
            div { class: "member-avatar {color_class}", "{initial}" }
            div { class: "member-info",
                div { class: "member-name", "{name}" }
                if let Some(ref title) = focus_title {
                    div { class: "member-focus",
                        span { class: "focus-arrow", "→" }
                        span { class: "focus-quest", "{title}" }
                    }
                }
            }
        }
    }
}

// ============================================================================
// CENTER PANEL - Quest List
// ============================================================================

#[component]
fn CenterPanel(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "center-panel",
            QuestListPanel { state }
        }
    }
}

#[component]
fn NetworkTopology(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let members = state_read.all_members();

    // Sized for left panel (narrower)
    let width = 240.0_f64;
    let height = 180.0_f64;
    let center_x = width / 2.0;
    let center_y = height / 2.0;

    // Calculate positions for members in a circle
    let member_count = members.len().max(1) as f64;
    let radius = (height / 2.0 - 25.0).min(width / 2.5);
    let angle_step = std::f64::consts::PI * 2.0 / member_count;
    let angle_offset = -std::f64::consts::PI / 2.0; // Start from top

    // Calculate positions
    let positions: Vec<(String, f64, f64)> = members
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let angle = angle_offset + (i as f64) * angle_step;
            let x = center_x + radius * angle.cos();
            let y = center_y + radius * angle.sin();
            (m.clone(), x, y)
        })
        .collect();

    // Build edges from contacts
    let edges: Vec<(usize, usize, bool)> = {
        let mut result = Vec::new();
        for (i, member) in members.iter().enumerate() {
            let contacts = state_read.contacts.get_contacts(member);
            for contact in contacts {
                if let Some(j) = members.iter().position(|m| m == contact) {
                    if i < j {
                        // Check if bidirectional
                        let reverse_contacts = state_read.contacts.get_contacts(contact);
                        let bidirectional = reverse_contacts.contains(&member);
                        result.push((i, j, bidirectional));
                    }
                }
            }
        }
        result
    };

    rsx! {
        div { class: "network-container",
            div { class: "network-header",
                span { class: "network-title", "Network Topology" }
                span { class: "panel-count", "{members.len()} members, {edges.len()} connections" }
            }
            div { class: "network-view",
                svg {
                    class: "network-svg",
                    view_box: "0 0 {width} {height}",
                    preserve_aspect_ratio: "xMidYMid meet",

                    // Draw edges
                    for (i, j, bidirectional) in edges.iter() {
                        {
                            let (_, x1, y1) = &positions[*i];
                            let (_, x2, y2) = &positions[*j];
                            let edge_class = if *bidirectional {
                                "network-edge bidirectional"
                            } else {
                                "network-edge unidirectional"
                            };

                            rsx! {
                                line {
                                    class: "{edge_class}",
                                    x1: "{x1}",
                                    y1: "{y1}",
                                    x2: "{x2}",
                                    y2: "{y2}",
                                }
                            }
                        }
                    }

                    // Draw nodes
                    for (member, x, y) in positions.iter() {
                        {
                            let name = member_name(member);
                            let fill = member_color_var(member);
                            let member_clone = member.clone();

                            rsx! {
                                g {
                                    class: "peer-node",
                                    onclick: move |_| {
                                        state_write.write().selected_pov = Some(member_clone.clone());
                                    },
                                    circle {
                                        class: "peer-node-circle",
                                        cx: "{x}",
                                        cy: "{y}",
                                        r: "24",
                                        fill: "{fill}",
                                    }
                                    text {
                                        class: "peer-node-label",
                                        x: "{x}",
                                        y: "{y}",
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                }
                if members.is_empty() {
                    div { class: "empty-state", "No members to display" }
                }
            }
        }
    }
}

#[component]
fn QuestListPanel(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    // Get quests sorted by attention (most attention first)
    let quests_with_attention: Vec<(QuestInfo, QuestAttention)> = {
        let attention_rankings = state_read.attention.quests_by_attention();
        let mut result = Vec::new();

        for qa in attention_rankings {
            if let Some(quest) = state_read.quests.quests.get(&qa.quest_id) {
                result.push((quest.clone(), qa.clone()));
            }
        }

        // Add quests without attention data
        for quest in state_read.quests.quests.values() {
            if !result.iter().any(|(q, _)| q.quest_id == quest.quest_id) {
                result.push((
                    quest.clone(),
                    QuestAttention {
                        quest_id: quest.quest_id.clone(),
                        total_attention_ms: 0,
                        by_member: std::collections::HashMap::new(),
                        currently_focusing: Vec::new(),
                    },
                ));
            }
        }

        result
    };

    let count = quests_with_attention.len();
    let max_attention = quests_with_attention
        .iter()
        .map(|(_, qa)| qa.total_attention_ms)
        .max()
        .unwrap_or(1)
        .max(1);

    rsx! {
        div { class: "quest-list-container",
            div { class: "quest-list-header",
                span { class: "quest-list-title", "Quests" }
                span { class: "quest-list-sort", "Sorted by attention • {count} total" }
            }
            div { class: "quest-list",
                for (quest, attention) in quests_with_attention.iter() {
                    QuestCardWithAttention {
                        quest: quest.clone(),
                        attention: attention.clone(),
                        max_attention
                    }
                }
                if count == 0 {
                    div { class: "empty-state", "No quests yet" }
                }
            }
        }
    }
}

#[component]
fn QuestCardWithAttention(quest: QuestInfo, attention: QuestAttention, max_attention: u64) -> Element {
    let secs = attention.total_attention_ms as f64 / 1000.0;
    let bar_width_pct = if max_attention > 0 {
        (attention.total_attention_ms as f64 / max_attention as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let status_class = match quest.status {
        QuestStatus::Open => "open",
        QuestStatus::Claimed => "claimed",
        QuestStatus::Verified => "verified",
        QuestStatus::Completed => "completed",
    };

    let status_text = match quest.status {
        QuestStatus::Open => "Open",
        QuestStatus::Claimed => "Claimed",
        QuestStatus::Verified => "Verified",
        QuestStatus::Completed => "Done",
    };

    rsx! {
        div { class: "quest-card",
            div { class: "quest-card-header",
                span { class: "quest-title", "{quest.title}" }
                span { class: "quest-status-badge {status_class}", "{status_text}" }
            }
            div { class: "quest-attention",
                div { class: "attention-bar",
                    div {
                        class: "attention-bar-fill",
                        style: "width: {bar_width_pct}%"
                    }
                }
                span { class: "attention-value", "{secs:.1}s" }
            }
            if !attention.currently_focusing.is_empty() {
                div { class: "quest-focusers",
                    for member in &attention.currently_focusing {
                        {
                            let name = member_name(member);
                            let initial = name.chars().next().unwrap_or('?');
                            let color_class = member_color_class(member);

                            rsx! {
                                span {
                                    class: "focuser-dot {color_class}",
                                    title: "{name}",
                                    "{initial}"
                                }
                            }
                        }
                    }
                    span { class: "focusing-label", "focusing" }
                }
            }
            if !quest.claims.is_empty() {
                div { class: "quest-claims",
                    for claim in &quest.claims {
                        ClaimBadge { claim: claim.clone() }
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
            if claim.verified { "✓" }
            "{initial}"
        }
    }
}

// ============================================================================
// RIGHT PANEL - Chat, Activity Timeline & Stats
// ============================================================================

#[component]
fn RightPanel(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "right-panel",
            ChatPanel { state }
            ActivityTimeline { state }
            GlobalStats { state }
        }
    }
}

#[component]
fn ChatPanel(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let messages = state_read.chat.recent_messages(20);
    let blessing_count = state_read.chat.total_blessings;
    let message_count = state_read.chat.total_messages;
    let mut draft = use_signal(|| String::new());

    rsx! {
        div { class: "chat-panel",
            div { class: "chat-header",
                span { class: "chat-title", "Realm Chat" }
                span { class: "chat-stats", "{message_count} msgs, {blessing_count} blessings" }
            }
            div { class: "chat-messages",
                for msg in messages.iter() {
                    ChatMessageItem { message: (*msg).clone() }
                }
                if messages.is_empty() {
                    div { class: "empty-state", "No messages yet" }
                }
            }
            div { class: "chat-input-container",
                input {
                    class: "chat-input",
                    r#type: "text",
                    placeholder: "Type a message...",
                    value: "{draft}",
                    oninput: move |e| draft.set(e.value())
                }
                button {
                    class: "chat-send-btn",
                    disabled: draft.read().is_empty(),
                    "Send"
                }
            }
        }
    }
}

#[component]
fn ChatMessageItem(message: crate::state::ChatMessage) -> Element {
    let name = member_name(&message.member);
    let color_class = member_color_class(&message.member);

    match &message.message_type {
        crate::state::ChatMessageType::Text => {
            rsx! {
                div { class: "chat-message text-message",
                    span { class: "chat-tick", "[{message.tick}]" }
                    span { class: "chat-sender {color_class}", "{name}" }
                    span { class: "chat-content", "{message.content}" }
                }
            }
        }
        crate::state::ChatMessageType::ProofSubmitted { quest_title, artifact_name, .. } => {
            rsx! {
                div { class: "chat-message proof-message",
                    span { class: "chat-tick", "[{message.tick}]" }
                    span { class: "chat-icon", "📎" }
                    span { class: "chat-sender {color_class}", "{name}" }
                    div { class: "proof-content",
                        span { class: "proof-label", "Proof: " }
                        span { class: "proof-quest", "{quest_title}" }
                        span { class: "proof-artifact", "({artifact_name})" }
                    }
                }
            }
        }
        crate::state::ChatMessageType::BlessingGiven { claimant, attention_millis, .. } => {
            let claimant_name = member_name(claimant);
            let duration = format_blessing_duration(*attention_millis);
            rsx! {
                div { class: "chat-message blessing-message",
                    span { class: "chat-tick", "[{message.tick}]" }
                    span { class: "chat-icon", "✨" }
                    span { class: "chat-sender {color_class}", "{name}" }
                    span { class: "blessing-text", "blessed {claimant_name} ({duration})" }
                }
            }
        }
    }
}

fn format_blessing_duration(millis: u64) -> String {
    let seconds = millis / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        let remaining_mins = minutes % 60;
        if remaining_mins > 0 {
            format!("{}h {}m", hours, remaining_mins)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}

#[component]
fn ActivityTimeline(state: Signal<AppState>) -> Element {
    let events = &state.read().event_log;

    rsx! {
        div { class: "activity-timeline",
            div { class: "timeline-header",
                span { class: "timeline-title", "Activity Timeline" }
            }
            div { class: "timeline-list",
                for event in events.iter().take(30) {
                    div { class: "timeline-event {event.category.css_class()}",
                        span { class: "event-icon",
                            match event.category.css_class() {
                                "event-realm" => "R",
                                "event-quest" => "Q",
                                "event-attention" => "A",
                                "event-contacts" => "C",
                                "event-chat" => "💬",
                                "event-blessing" => "✨",
                                _ => "•"
                            }
                        }
                        span { class: "event-tick", "[{event.tick}]" }
                        span { class: "event-message", "{event.summary}" }
                    }
                }
                if events.is_empty() {
                    div { class: "empty-state", "No events yet" }
                }
            }
        }
    }
}

#[component]
fn GlobalStats(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let realm_count = state_read.realms.realms.len();
    let quest_count = state_read.quests.quests.len();
    let member_count = state_read.all_members().len();
    let contact_count = state_read.contacts.contact_count();

    rsx! {
        div { class: "global-stats",
            span { class: "stats-title", "Statistics" }
            div { class: "stats-grid",
                div { class: "stat-item",
                    span { class: "stat-value", "{realm_count}" }
                    span { class: "stat-label", "Realms" }
                }
                div { class: "stat-item",
                    span { class: "stat-value", "{quest_count}" }
                    span { class: "stat-label", "Quests" }
                }
                div { class: "stat-item",
                    span { class: "stat-value", "{member_count}" }
                    span { class: "stat-label", "Members" }
                }
                div { class: "stat-item",
                    span { class: "stat-value", "{contact_count}" }
                    span { class: "stat-label", "Contacts" }
                }
            }
        }
    }
}

// ============================================================================
// FLOATING CONTROL BAR
// ============================================================================

#[component]
fn FloatingControlBar(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let is_paused = state.read().playback.paused;
    let speed = state.read().playback.speed;
    let tick = state.read().tick;

    rsx! {
        div { class: "floating-controls",
            // Main control pill
            div { class: "control-pill",
                // Reset button
                button {
                    class: "control-icon-btn",
                    title: "Reset",
                    onclick: move |_| {
                        state_write.write().reset();
                        playback::reset();
                        playback::request_reset();
                    },
                    "↻"
                }

                // Play/Pause button (hero)
                button {
                    class: "play-pause-btn",
                    title: if is_paused { "Play" } else { "Pause" },
                    onclick: move |_| {
                        let new_paused = !is_paused;
                        state_write.write().playback.paused = new_paused;
                        playback::set_paused(new_paused);
                    },
                    if is_paused { "▶" } else { "⏸" }
                }

                // Step button
                button {
                    class: "control-icon-btn",
                    title: "Step",
                    disabled: !is_paused,
                    onclick: move |_| {
                        playback::request_step();
                    },
                    "⏭"
                }

                div { class: "control-divider" }

                // Tick counter
                div { class: "tick-counter",
                    span { class: "tick-label", "T:" }
                    span { class: "tick-current", "{tick}" }
                }
            }

            // Speed pill
            div { class: "speed-pill",
                span { class: "speed-value", "{speed:.1}x" }
                input {
                    class: "speed-slider",
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

            // Status pill
            div { class: "status-pill",
                div { class: "status-dot" }
                span { class: "status-text",
                    if is_paused { "Paused" } else { "Playing" }
                }
            }
        }
    }
}

// ============================================================================
// POV DASHBOARD - First Person View
// ============================================================================

#[component]
pub fn POVDashboard(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let member = state_read.selected_pov.clone().unwrap_or_default();
    let name = member_name(&member);
    let stats = state_read.stats_for_member(&member);
    let color_class = member_color_class(&member);

    rsx! {
        ThemedRoot {
            ThemeSwitcher {}
            div { class: "pov-dashboard {color_class}",
                POVHeader {
                    name: name.clone(),
                    on_back: move |_| {
                        state_write.write().selected_pov = None;
                    },
                }
                main { class: "pov-content",
                    div { class: "pov-left-column",
                        ProfileHero {
                            member: member.clone(),
                            name: name.clone(),
                            stats: stats.clone(),
                        }
                        MyNetworkView { state, member: member.clone() }
                    }
                    div { class: "pov-center-column",
                        MyAttentionPanel { state, member: member.clone() }
                        div { class: "pov-quests-chat-row",
                            MyQuestsList { state }
                            MyChatPanel { state, member: member.clone() }
                        }
                    }
                    div { class: "pov-right-column",
                        MyRealms { state, member: member.clone() }
                        MyActivity { state, member: member.clone() }
                    }
                }
            }
            FloatingControlBar { state }
        }
    }
}

#[component]
fn POVHeader(name: String, on_back: EventHandler<()>) -> Element {
    rsx! {
        header { class: "pov-header",
            div {
                class: "back-button",
                onclick: move |_| on_back.call(()),
                span { class: "back-arrow", "←" }
                span { "Back to Overview" }
            }
            h1 { class: "pov-title", "{name}'s Dashboard" }
            div { class: "pov-header-spacer" }
        }
    }
}

#[component]
fn ProfileHero(member: String, name: String, stats: MemberStats) -> Element {
    let initial = name.chars().next().unwrap_or('?');
    let color_class = member_color_class(&member);

    rsx! {
        div { class: "profile-hero",
            div { class: "profile-avatar-large {color_class}",
                "{initial}"
            }
            h2 { class: "profile-name", "{name}" }
            div { class: "profile-stats",
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.quests_created}" }
                    span { class: "profile-stat-label", "Created" }
                }
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.quests_assigned}" }
                    span { class: "profile-stat-label", "Claimed" }
                }
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.quests_completed}" }
                    span { class: "profile-stat-label", "Done" }
                }
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.realms_count}" }
                    span { class: "profile-stat-label", "Realms" }
                }
                div { class: "profile-stat",
                    span { class: "profile-stat-value", "{stats.contacts_count}" }
                    span { class: "profile-stat-label", "Contacts" }
                }
            }
        }
    }
}

#[component]
fn MyAttentionPanel(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let current_focus = state_read.attention.focus_for_member(&member).cloned();
    let attention_data = state_read.attention.attention_for_member(&member);

    // Get current focus quest title
    let current_focus_title = current_focus.as_ref().and_then(|qid| {
        state_read.quests.quests.get(qid).map(|q| q.title.clone())
    });

    // Build attention items with quest titles
    let attention_items: Vec<(String, String, u64)> = attention_data
        .iter()
        .take(8)
        .map(|qa| {
            let title = state_read.quests.quests.get(&qa.quest_id)
                .map(|q| q.title.clone())
                .unwrap_or_else(|| short_id(&qa.quest_id));
            (qa.quest_id.clone(), title, qa.total_attention_ms)
        })
        .collect();

    let max_attention = attention_items.first().map(|(_, _, ms)| *ms).unwrap_or(1).max(1);

    rsx! {
        div { class: "my-attention-panel",
            div { class: "panel-header",
                span { class: "panel-title", "My Attention" }
            }

            // Current Focus Section
            div { class: "focus-section",
                div { class: "focus-header", "Currently Focused" }
                if let Some(ref title) = current_focus_title {
                    div { class: "focus-quest-title", "{title}" }
                } else {
                    div { class: "focus-quest-none", "Not focusing on any quest" }
                }
            }

            // Attention History
            if !attention_items.is_empty() {
                div { class: "attention-history",
                    div { class: "attention-history-header", "Time Spent" }
                    div { class: "attention-history-list",
                        for (_, title, ms) in attention_items.iter() {
                            {
                                let secs = *ms as f64 / 1000.0;
                                let bar_width = (*ms as f64 / max_attention as f64 * 100.0).min(100.0);

                                rsx! {
                                    div { class: "attention-history-item",
                                        div { class: "attention-quest-title", "{title}" }
                                        div { class: "attention-quest-bar",
                                            div { class: "attention-bar-track",
                                                div {
                                                    class: "attention-bar-fill",
                                                    style: "width: {bar_width}%"
                                                }
                                            }
                                            span { class: "attention-time", "{secs:.1}s" }
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
}

#[component]
fn MyNetworkView(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let mut state_write = state;
    let contacts = state_read.contacts.contacts_for_member(&member);

    let width = 400.0;
    let height = 180.0;
    let center_x = width / 2.0;
    let center_y = height / 2.0;

    let contact_count = contacts.len().max(1) as f64;
    let angle_step = std::f64::consts::PI * 2.0 / contact_count;
    let angle_offset = -std::f64::consts::PI / 2.0;
    let radius = 65.0;

    rsx! {
        div { class: "my-network-view",
            div { class: "network-header",
                span { class: "network-title", "My Network" }
                span { class: "panel-count", "{contacts.len()} contacts" }
            }
            svg {
                class: "network-svg ego-centric",
                view_box: "0 0 {width} {height}",
                preserve_aspect_ratio: "xMidYMid meet",

                // Draw edges from center to contacts
                for (i, _contact) in contacts.iter().enumerate() {
                    {
                        let angle = angle_offset + (i as f64) * angle_step;
                        let other_x = center_x + radius * angle.cos();
                        let other_y = center_y + radius * angle.sin();

                        rsx! {
                            line {
                                class: "network-edge bidirectional",
                                x1: "{center_x}",
                                y1: "{center_y}",
                                x2: "{other_x}",
                                y2: "{other_y}",
                            }
                        }
                    }
                }

                // Draw center (ego) node
                {
                    let name = member_name(&member);
                    let fill = member_color_var(&member);

                    rsx! {
                        g { class: "ego-node",
                            circle {
                                class: "peer-node-circle ego",
                                cx: "{center_x}",
                                cy: "{center_y}",
                                r: "30",
                                fill: "{fill}",
                            }
                            text {
                                class: "peer-node-label ego",
                                x: "{center_x}",
                                y: "{center_y}",
                                "{name}"
                            }
                        }
                    }
                }

                // Draw contact nodes
                for (i, contact) in contacts.iter().enumerate() {
                    {
                        let angle = angle_offset + (i as f64) * angle_step;
                        let other_x = center_x + radius * angle.cos();
                        let other_y = center_y + radius * angle.sin();
                        let name = member_name(contact);
                        let fill = member_color_var(contact);
                        let contact_clone = contact.clone();

                        rsx! {
                            g {
                                class: "other-node clickable",
                                onclick: move |_| {
                                    state_write.write().selected_pov = Some(contact_clone.clone());
                                },
                                circle {
                                    class: "peer-node-circle",
                                    cx: "{other_x}",
                                    cy: "{other_y}",
                                    r: "20",
                                    fill: "{fill}",
                                }
                                text {
                                    class: "peer-node-label",
                                    x: "{other_x}",
                                    y: "{other_y}",
                                    "{name}"
                                }
                            }
                        }
                    }
                }
            }
            if contacts.is_empty() {
                div { class: "empty-state", "No contacts yet" }
            }
        }
    }
}

/// All quests sorted by cumulative attention with status and realm tags
#[component]
fn MyQuestsList(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    // Get all quests with their attention data, sorted by attention
    let quests_with_attention: Vec<(QuestInfo, QuestAttention, Option<String>)> = {
        let attention_rankings = state_read.attention.quests_by_attention();
        let mut result = Vec::new();

        // First add quests that have attention data
        for qa in attention_rankings {
            if let Some(quest) = state_read.quests.quests.get(&qa.quest_id) {
                // Find realm name for this quest
                let realm_name = state_read.realms.realms.values()
                    .find(|r| r.realm_id == quest.realm_id)
                    .map(|r| {
                        let names: Vec<String> = r.members.iter().take(2).map(|m| member_name(m)).collect();
                        if names.is_empty() { short_id(&r.realm_id) } else { names.join("+") }
                    });
                result.push((quest.clone(), qa.clone(), realm_name));
            }
        }

        // Add quests without attention data
        for quest in state_read.quests.quests.values() {
            if !result.iter().any(|(q, _, _)| q.quest_id == quest.quest_id) {
                let realm_name = state_read.realms.realms.values()
                    .find(|r| r.realm_id == quest.realm_id)
                    .map(|r| {
                        let names: Vec<String> = r.members.iter().take(2).map(|m| member_name(m)).collect();
                        if names.is_empty() { short_id(&r.realm_id) } else { names.join("+") }
                    });
                result.push((
                    quest.clone(),
                    QuestAttention {
                        quest_id: quest.quest_id.clone(),
                        total_attention_ms: 0,
                        by_member: std::collections::HashMap::new(),
                        currently_focusing: Vec::new(),
                    },
                    realm_name,
                ));
            }
        }

        result
    };

    let count = quests_with_attention.len();
    let max_attention = quests_with_attention
        .iter()
        .map(|(_, qa, _)| qa.total_attention_ms)
        .max()
        .unwrap_or(1)
        .max(1);

    rsx! {
        div { class: "my-quests-list",
            div { class: "quest-list-header",
                span { class: "quest-list-title", "All Quests" }
                span { class: "quest-list-sort", "Sorted by attention • {count} total" }
            }
            div { class: "quest-list",
                for (quest, attention, realm_name) in quests_with_attention.iter() {
                    QuestCardWithRealm {
                        quest: quest.clone(),
                        attention: attention.clone(),
                        realm_name: realm_name.clone(),
                        max_attention
                    }
                }
                if count == 0 {
                    div { class: "empty-state", "No quests yet" }
                }
            }
        }
    }
}

#[component]
fn QuestCardWithRealm(quest: QuestInfo, attention: QuestAttention, realm_name: Option<String>, max_attention: u64) -> Element {
    let secs = attention.total_attention_ms as f64 / 1000.0;
    let bar_width_pct = if max_attention > 0 {
        (attention.total_attention_ms as f64 / max_attention as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let status_class = match quest.status {
        QuestStatus::Open => "open",
        QuestStatus::Claimed => "claimed",
        QuestStatus::Verified => "verified",
        QuestStatus::Completed => "completed",
    };

    let status_text = match quest.status {
        QuestStatus::Open => "Open",
        QuestStatus::Claimed => "Claimed",
        QuestStatus::Verified => "Verified",
        QuestStatus::Completed => "Done",
    };

    let realm_display = realm_name.unwrap_or_else(|| "Unknown".to_string());

    rsx! {
        div { class: "quest-card",
            div { class: "quest-card-header",
                span { class: "quest-title", "{quest.title}" }
                div { class: "quest-tags",
                    span { class: "quest-status-badge {status_class}", "{status_text}" }
                    span { class: "quest-realm-badge", "{realm_display}" }
                }
            }
            div { class: "quest-attention",
                div { class: "attention-bar",
                    div {
                        class: "attention-bar-fill",
                        style: "width: {bar_width_pct}%"
                    }
                }
                span { class: "attention-value", "{secs:.1}s" }
            }
            if !attention.currently_focusing.is_empty() {
                div { class: "quest-focusers",
                    for member in &attention.currently_focusing {
                        {
                            let name = member_name(member);
                            let initial = name.chars().next().unwrap_or('?');
                            let color_class = member_color_class(member);

                            rsx! {
                                span {
                                    class: "focuser-dot {color_class}",
                                    title: "{name}",
                                    "{initial}"
                                }
                            }
                        }
                    }
                    span { class: "focusing-label", "focusing" }
                }
            }
        }
    }
}

#[component]
fn MyChatPanel(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let messages = state_read.chat.recent_messages(15);
    let blessing_count = state_read.chat.total_blessings;
    let message_count = state_read.chat.total_messages;
    let mut draft = use_signal(|| String::new());

    rsx! {
        div { class: "my-chat-panel",
            div { class: "panel-header",
                span { class: "panel-title", "Realm Chat" }
                span { class: "panel-count", "{message_count} msgs" }
            }
            div { class: "my-chat-messages",
                for msg in messages.iter() {
                    ChatMessageItem { message: (*msg).clone() }
                }
                if messages.is_empty() {
                    div { class: "empty-state", "No messages yet" }
                }
            }
            div { class: "chat-input-container",
                input {
                    class: "chat-input",
                    r#type: "text",
                    placeholder: "Type a message...",
                    value: "{draft}",
                    oninput: move |e| draft.set(e.value())
                }
                button {
                    class: "chat-send-btn",
                    disabled: draft.read().is_empty(),
                    "Send"
                }
            }
        }
    }
}

#[component]
fn MyRealms(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let realms: Vec<_> = state_read.realms.realms_for_member(&member).into_iter().cloned().collect();
    let count = realms.len();

    rsx! {
        div { class: "my-realms-panel",
            div { class: "panel-header",
                span { class: "panel-title", "My Realms" }
                span { class: "panel-count", "({count})" }
            }
            div { class: "realms-list",
                for realm in realms.iter() {
                    {
                        // Show member names instead of realm ID
                        let member_names: Vec<String> = realm.members.iter()
                            .map(|m| member_name(m))
                            .collect();
                        let realm_title = member_names.join(" + ");
                        let quest_count = realm.quest_count;

                        rsx! {
                            div { class: "realm-card-item",
                                div { class: "realm-card-title", "{realm_title}" }
                                div { class: "realm-card-stats",
                                    span { class: "realm-stat",
                                        span { class: "realm-stat-value", "{realm.members.len()}" }
                                        " members"
                                    }
                                    span { class: "realm-stat-divider", "•" }
                                    span { class: "realm-stat",
                                        span { class: "realm-stat-value", "{quest_count}" }
                                        " quests"
                                    }
                                }
                            }
                        }
                    }
                }
                if realms.is_empty() {
                    div { class: "empty-state", "No realms yet" }
                }
            }
        }
    }
}

#[component]
fn MyActivity(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let events: Vec<_> = state_read.events_for_member(&member).into_iter().take(15).collect();

    rsx! {
        div { class: "my-activity",
            div { class: "panel-header",
                div { class: "panel-title", "My Activity" }
            }
            div { class: "activity-list",
                for event in events.iter() {
                    div { class: "timeline-event {event.category.css_class()}",
                        span { class: "event-tick", "[{event.tick}]" }
                        span { class: "event-message", "{event.summary}" }
                    }
                }
                if events.is_empty() {
                    div { class: "empty-state", "No activity yet" }
                }
            }
        }
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn member_color_class(member: &str) -> &'static str {
    let name = member_name(member).to_lowercase();
    match name.as_str() {
        "love" => "member-love",
        "joy" => "member-joy",
        "peace" => "member-peace",
        "grace" => "member-grace",
        "hope" => "member-hope",
        "faith" => "member-faith",
        "light" => "member-light",
        "truth" => "member-truth",
        "wisdom" => "member-wisdom",
        "mercy" => "member-mercy",
        "valor" => "member-valor",
        "honor" => "member-honor",
        "glory" => "member-glory",
        "spirit" => "member-spirit",
        "unity" => "member-unity",
        "bliss" => "member-bliss",
        _ => "member-default",
    }
}

fn member_color_var(member: &str) -> &'static str {
    let name = member_name(member).to_lowercase();
    match name.as_str() {
        "love" => "var(--color-love)",
        "joy" => "var(--color-joy)",
        "peace" => "var(--color-peace)",
        "grace" => "var(--color-grace)",
        "hope" => "var(--color-hope)",
        "faith" => "var(--color-faith)",
        _ => "var(--accent-primary)",
    }
}
