//! Omni V2 - Live Multi-Screen Dashboard Components
//!
//! Each column is a live viewport into what one member is currently seeing.
//! As events stream in, members "flip between screens" and we watch it
//! happen in real-time. Like a mission control room with one monitor per person.

use dioxus::prelude::*;

use crate::state::{
    member_name, short_id, format_duration_millis,
    AppState, MemberScreen, ArtifactState, ArtifactStatus, ArtifactInfo, DraftArtifactInfo,
};
use crate::theme::{ThemeSwitcher, ThemedRoot};

use super::{
    member_color_class, member_color_var,
    FloatingControlBar, MarkdownPreviewOverlay, ProofNarrativeOverlay,
    PreviewFile, PreviewViewMode, PreviewContext,
    ProofNarrativeData, ProofNarrativeContext,
};

// ============================================================================
// OMNI V2 APP - Top-level component
// ============================================================================

/// Top-level V2 omni viewer component
#[component]
pub fn OmniV2App(state: Signal<AppState>) -> Element {
    rsx! {
        ThemedRoot {
            ThemeSwitcher {}
            OmniV2Dashboard { state }
            FloatingControlBar { state }
        }
    }
}

// ============================================================================
// OMNI V2 DASHBOARD - CSS Grid layout
// ============================================================================

/// Grid layout showing all member columns
#[component]
fn OmniV2Dashboard(state: Signal<AppState>) -> Element {
    let members = state.read().all_members();
    let column_count = members.len().max(1);

    rsx! {
        div {
            class: "v2-dashboard",
            style: "--omni-columns: {column_count}",
            for member in members.iter() {
                MemberColumn {
                    state,
                    member: member.clone(),
                }
            }
            if members.is_empty() {
                div { class: "v2-empty",
                    div { class: "v2-empty-text", "Waiting for members..." }
                }
            }
        }
    }
}

// ============================================================================
// MEMBER COLUMN - Single column viewport
// ============================================================================

/// Single column showing one member's current screen
#[component]
fn MemberColumn(state: Signal<AppState>, member: String) -> Element {
    let color_class = member_color_class(&member);
    let color_var = member_color_var(&member);

    let ticks_since = state.read().ticks_since_action(&member);
    let idle_class = if ticks_since > 50 { " column-idle" } else { "" };

    // Per-column preview overlay state
    let preview_open = use_signal(|| false);
    let preview_file = use_signal(|| None::<PreviewFile>);
    let preview_mode = use_signal(|| PreviewViewMode::Rendered);

    // Per-column proof narrative overlay state
    let proof_narrative_open = use_signal(|| false);
    let proof_narrative_data = use_signal(|| None::<ProofNarrativeData>);
    let proof_narrative_mode = use_signal(|| PreviewViewMode::Rendered);

    // Provide contexts scoped to this column's subtree
    use_context_provider(|| PreviewContext {
        is_open: preview_open,
        file: preview_file,
        view_mode: preview_mode,
    });

    use_context_provider(|| ProofNarrativeContext {
        is_open: proof_narrative_open,
        data: proof_narrative_data,
    });

    rsx! {
        div {
            class: "member-column {color_class}{idle_class}",
            style: "--member-color: {color_var}",
            ColumnHeader { state, member: member.clone() }
            ColumnBody { state, member: member.clone() }
            ColumnFooter { state, member: member.clone() }
            ColumnNav { state, member: member.clone() }
            // Overlays scoped to this column
            MarkdownPreviewOverlay {
                is_open: preview_open,
                file: preview_file,
                view_mode: preview_mode,
            }
            ProofNarrativeOverlay {
                is_open: proof_narrative_open,
                data: proof_narrative_data,
                view_mode: proof_narrative_mode,
            }
        }
    }
}

// ============================================================================
// COLUMN HEADER - Identity row only
// ============================================================================

/// Fixed header: avatar + name + recency
#[component]
fn ColumnHeader(state: Signal<AppState>, member: String) -> Element {
    let name = member_name(&member);
    let initial = name.chars().next().unwrap_or('?');
    let color_class = member_color_class(&member);

    let ticks_since = state.read().ticks_since_action(&member);
    let recency = if ticks_since == u32::MAX {
        "--".to_string()
    } else if ticks_since == 0 {
        "now".to_string()
    } else {
        format!("{}t ago", ticks_since)
    };

    rsx! {
        div { class: "column-header",
            div { class: "v2-avatar {color_class}", "{initial}" }
            div { class: "identity-name", "{name}" }
            div { class: "identity-recency", "{recency}" }
        }
    }
}

// ============================================================================
// COLUMN NAV - Bottom tab bar
// ============================================================================

/// Fixed bottom navigation bar with screen tabs
#[component]
fn ColumnNav(state: Signal<AppState>, member: String) -> Element {
    let mut state_write = state;
    let current_screen = state.read().screen_for_member(&member).screen.clone();

    rsx! {
        div { class: "column-nav",
            for screen in MemberScreen::tabs().iter() {
                {
                    let is_active = *screen == current_screen;
                    let active_class = if is_active { " tab-active" } else { "" };
                    let label = screen.label();
                    let icon_svg = screen.icon_svg();
                    let screen_clone = screen.clone();
                    let member_clone = member.clone();
                    rsx! {
                        div {
                            class: "screen-tab{active_class}",
                            onclick: move |_| {
                                state_write.write()
                                    .member_screens
                                    .entry(member_clone.clone())
                                    .or_default()
                                    .screen = screen_clone.clone();
                            },
                            svg {
                                class: "tab-icon",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "16",
                                height: "16",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                dangerous_inner_html: "{icon_svg}",
                            }
                            span { class: "tab-label", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// COLUMN BODY - Scrollable screen content
// ============================================================================

/// Scrollable body that renders the current screen's content
#[component]
fn ColumnBody(state: Signal<AppState>, member: String) -> Element {
    let screen = state.read().screen_for_member(&member).screen.clone();

    rsx! {
        div { class: "column-body",
            match screen {
                MemberScreen::Home => rsx! { V2HomeScreen { state, member: member.clone() } },
                MemberScreen::QuestBoard => rsx! { V2QuestScreen { state, member: member.clone() } },
                MemberScreen::Chat => rsx! { V2ChatScreen { state, member: member.clone() } },
                MemberScreen::ProofEditor => rsx! { V2ProofScreen { state, member: member.clone() } },
                MemberScreen::Artifacts => rsx! { V2ArtifactScreen { state, member: member.clone() } },
                MemberScreen::Realms => rsx! { V2RealmsScreen { state, member: member.clone() } },
                MemberScreen::Activity => rsx! { V2HomeScreen { state, member: member.clone() } },
            }
        }
    }
}

// ============================================================================
// COLUMN FOOTER - Persistent activity tail
// ============================================================================

/// Fixed bottom bar showing recent activity for this member
#[component]
fn ColumnFooter(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let events: Vec<_> = state_read
        .events_for_member(&member)
        .into_iter()
        .take(4)
        .collect();

    if events.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "column-footer",
            for event in events.iter() {
                div { class: "footer-activity-line",
                    span { class: "footer-activity-tick", "[{event.tick}]" }
                    span { class: "footer-activity-text", "{event.summary}" }
                }
            }
        }
    }
}

// ============================================================================
// V2 HOME SCREEN - Profile + focus + contacts + tokens
// ============================================================================

/// Home screen: large avatar, focus indicator, stats, contacts, tokens
#[component]
fn V2HomeScreen(state: Signal<AppState>, member: String) -> Element {
    let name = member_name(&member);
    let initial = name.chars().next().unwrap_or('?');
    let color_class = member_color_class(&member);

    let state_read = state.read();
    let current_focus = state_read.attention.focus_for_member(&member).cloned();
    let focus_title = current_focus.as_ref().and_then(|qid| {
        state_read.quests.quests.get(qid).map(|q| q.title.clone())
    });

    let stats = state_read.stats_for_member(&member);
    let contacts = state_read.contacts.contacts_for_member(&member);
    let tokens = state_read.tokens.tokens_for_member(&member);
    let total_token_value = state_read.tokens.total_value_for_member(&member);

    rsx! {
        div { class: "v2-home",
            // Profile section
            div { class: "v2-home-profile",
                div { class: "v2-home-avatar {color_class}", "{initial}" }
                div { class: "v2-home-name", "{name}" }
            }
            // Focus indicator
            if let Some(ref title) = focus_title {
                div { class: "v2-home-focus",
                    span { class: "v2-home-focus-dot" }
                    span { class: "v2-home-focus-text", "{title}" }
                }
            } else {
                div { class: "v2-home-no-focus", "No current focus" }
            }
            // Stats ribbon
            div { class: "v2-home-stats",
                span { class: "mini-stat", "{stats.realms_count}R" }
                span { class: "mini-stat", "{stats.quests_created}Q" }
                span { class: "mini-stat", "{stats.events_count}E" }
                if stats.tokens_count > 0 {
                    span { class: "mini-stat mini-stat-accent", "{stats.tokens_count}T" }
                }
            }
            // Contacts section
            if !contacts.is_empty() {
                div { class: "v2-home-section",
                    div { class: "mini-section-label", "CONTACTS" }
                    div { class: "v2-home-contacts",
                        for contact_id in contacts.iter().take(6) {
                            {
                                let contact_name = member_name(contact_id);
                                let contact_initial = contact_name.chars().next().unwrap_or('?');
                                let contact_color = member_color_class(contact_id);
                                let is_mutual = state_read.contacts.is_mutual(&member, contact_id);
                                let mutual_class = if is_mutual { " v2-contact-mutual" } else { "" };
                                rsx! {
                                    div { class: "v2-home-contact{mutual_class}",
                                        div { class: "v2-avatar {contact_color}", "{contact_initial}" }
                                        span { class: "v2-home-contact-name", "{contact_name}" }
                                        if is_mutual {
                                            span { class: "v2-home-contact-badge", "mutual" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Tokens of Gratitude section
            if !tokens.is_empty() {
                div { class: "v2-home-section",
                    div { class: "mini-section-label", "TOKENS OF GRATITUDE" }
                    div { class: "v2-home-token-total",
                        "Total: {format_duration_millis(total_token_value)}"
                    }
                    for token in tokens.iter().take(4) {
                        div { class: "v2-home-token-row",
                            span { class: "v2-home-token-quest", "{token.quest_title}" }
                            span { class: "v2-home-token-value", "{token.formatted_value()}" }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// V2 QUEST SCREEN - Quest list + create form + claim buttons
// ============================================================================

/// Quest screen: quest list with attention bars, create form, claim buttons
#[component]
fn V2QuestScreen(state: Signal<AppState>, member: String) -> Element {
    let mut show_create_form = use_signal(|| false);
    let mut quest_title_draft = use_signal(String::new);

    let state_read = state.read();
    let current_focus = state_read.attention.focus_for_member(&member).cloned();

    // Build attention lookup: quest_id -> member's ms
    let attention_items = state_read.attention.quests_by_attention();
    let attention_map: std::collections::HashMap<&str, u64> = attention_items
        .iter()
        .filter_map(|qa| {
            let ms = qa.by_member.get(&member).copied().unwrap_or(0);
            if ms > 0 { Some((qa.quest_id.as_str(), ms)) } else { None }
        })
        .collect();

    // Show all quests in this member's realms
    let member_realms = state_read.realms.realms_for_member(&member);
    let realm_ids: Vec<&str> = member_realms.iter().map(|r| r.realm_id.as_str()).collect();

    let quests: Vec<(String, u64, bool, String, String)> = state_read
        .quests
        .quests
        .values()
        .filter(|q| realm_ids.contains(&q.realm_id.as_str()))
        .map(|q| {
            let my_ms = attention_map.get(q.quest_id.as_str()).copied().unwrap_or(0);
            let is_focusing = current_focus.as_ref() == Some(&q.quest_id);
            let status = format!("{:?}", q.status);
            (q.title.clone(), my_ms, is_focusing, status, q.quest_id.clone())
        })
        .collect();

    let max_ms = quests.iter().map(|(_, ms, _, _, _)| *ms).max().unwrap_or(1).max(1);

    rsx! {
        div { class: "v2-quest",
            // Header with create toggle
            div { class: "v2-quest-header",
                span { class: "mini-section-label", "QUESTS" }
                button {
                    class: "v2-btn",
                    onclick: move |_| show_create_form.toggle(),
                    if *show_create_form.read() { "Cancel" } else { "+ New Quest" }
                }
            }
            // Create form
            if *show_create_form.read() {
                div { class: "v2-quest-create-form",
                    input {
                        class: "v2-input",
                        r#type: "text",
                        placeholder: "Quest title...",
                        value: "{quest_title_draft}",
                        oninput: move |e| quest_title_draft.set(e.value()),
                    }
                    button {
                        class: "v2-btn v2-btn-primary",
                        disabled: quest_title_draft.read().is_empty(),
                        onclick: move |_| {
                            quest_title_draft.set(String::new());
                            show_create_form.set(false);
                        },
                        "Create"
                    }
                }
            }
            // Quest list
            if quests.is_empty() {
                div { class: "mini-empty", "No quests yet" }
            }
            for (title, ms, is_focusing, status, _quest_id) in quests.iter() {
                {
                    let bar_pct = (*ms as f64 / max_ms as f64 * 100.0).min(100.0);
                    let secs = *ms as f64 / 1000.0;
                    let focus_class = if *is_focusing { " quest-focusing" } else { "" };
                    let is_open = status == "Open";

                    rsx! {
                        div { class: "mini-quest-item{focus_class}",
                            div { class: "mini-quest-title-row",
                                span { class: "mini-quest-title", "{title}" }
                                span { class: "mini-quest-status", "{status}" }
                            }
                            div { class: "mini-quest-bar-row",
                                div { class: "mini-quest-bar",
                                    div {
                                        class: "mini-quest-bar-fill",
                                        style: "width: {bar_pct}%",
                                    }
                                }
                                span { class: "mini-quest-time", "{secs:.1}s" }
                                if *is_focusing {
                                    span { class: "mini-quest-focus-badge", "FOCUS" }
                                }
                            }
                            // Action row with Claim button
                            if is_open {
                                div { class: "v2-quest-actions",
                                    button { class: "v2-btn", "Claim" }
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
// V2 CHAT SCREEN - Messages + input + action menu
// ============================================================================

/// Enriched chat content for V2 rendering
enum V2ChatContent {
    Text(String),
    ProofFolder {
        quest_id: String,
        quest_title: String,
        claimant: String,
        artifact_count: usize,
        folder_id: String,
    },
    Blessing {
        claimant_name: String,
        duration_text: String,
    },
}

/// A single chat message ready for V2 rendering
struct V2ChatMsg {
    tick: u32,
    sender: String,
    #[allow(dead_code)]
    sender_id: String,
    color_class: String,
    content: V2ChatContent,
}

/// Chat screen: scrollable messages, input row, action menu
#[component]
fn V2ChatScreen(state: Signal<AppState>, member: String) -> Element {
    let mut draft = use_signal(String::new);
    let mut show_action_menu = use_signal(|| false);
    let mut showing_quest_selector = use_signal(|| false);

    let state_read = state.read();

    // Get realm name for header
    let member_realms = state_read.realms.realms_for_member(&member);
    let realm_name = member_realms.first()
        .map(|r| state_read.realms.get_display_name(r))
        .unwrap_or_else(|| "Chat".to_string());
    let realm_ids: Vec<String> = member_realms.iter().map(|r| r.realm_id.clone()).collect();

    // Collect messages with rich content types
    let mut messages: Vec<V2ChatMsg> = Vec::new();
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for realm_id in &realm_ids {
        if let Some(realm_msgs) = state_read.chat.messages_by_realm.get(realm_id) {
            for msg in realm_msgs.iter().rev().take(15) {
                if !msg.is_deleted {
                    seen_ids.insert(&msg.id);
                    let sender = member_name(&msg.member);
                    let content = match &msg.message_type {
                        crate::state::ChatMessageType::Text => V2ChatContent::Text(msg.content.clone()),
                        crate::state::ChatMessageType::ProofSubmitted { quest_title, .. } => {
                            V2ChatContent::Text(format!("[Proof: {}]", quest_title))
                        }
                        crate::state::ChatMessageType::ProofFolderSubmitted { quest_id, quest_title, artifact_count, folder_id, .. } => {
                            V2ChatContent::ProofFolder {
                                quest_id: quest_id.clone(),
                                quest_title: quest_title.clone(),
                                claimant: msg.member.clone(),
                                artifact_count: *artifact_count,
                                folder_id: folder_id.clone(),
                            }
                        }
                        crate::state::ChatMessageType::BlessingGiven { claimant, attention_millis, .. } => {
                            V2ChatContent::Blessing {
                                claimant_name: member_name(claimant),
                                duration_text: format_duration_millis(*attention_millis),
                            }
                        }
                        crate::state::ChatMessageType::Image { alt_text, filename, .. } => {
                            let desc = alt_text.as_ref().or(filename.as_ref())
                                .map(|s| s.as_str()).unwrap_or("image");
                            V2ChatContent::Text(format!("[Image: {}]", desc))
                        }
                        crate::state::ChatMessageType::Gallery { title, items, .. } => {
                            let desc = title.as_deref().unwrap_or("gallery");
                            V2ChatContent::Text(format!("[Gallery: {} ({} items)]", desc, items.len()))
                        }
                    };
                    let color_class = member_color_class(&msg.member).to_string();
                    messages.push(V2ChatMsg {
                        tick: msg.tick,
                        sender,
                        sender_id: msg.member.clone(),
                        color_class,
                        content,
                    });
                }
            }
        }
    }

    // Also include global messages involving this member (skip realm duplicates)
    for msg in state_read.chat.global_messages.iter().rev().take(15) {
        if !msg.is_deleted && msg.member == member && !seen_ids.contains(msg.id.as_str()) {
            let sender = member_name(&msg.member);
            let content = match &msg.message_type {
                crate::state::ChatMessageType::Text => V2ChatContent::Text(msg.content.clone()),
                _ => V2ChatContent::Text(msg.content.chars().take(50).collect()),
            };
            let color_class = member_color_class(&msg.member).to_string();
            messages.push(V2ChatMsg {
                tick: msg.tick,
                sender,
                sender_id: msg.member.clone(),
                color_class,
                content,
            });
        }
    }

    messages.sort_by(|a, b| b.tick.cmp(&a.tick));
    messages.truncate(15);

    let msg_count = messages.len();

    // Build gratitude data: for each proof folder message, find members with
    // unreleased attention on that quest who haven't blessed yet
    let gratitude_data: std::collections::HashMap<String, Vec<(String, String, u64)>> = messages.iter()
        .filter_map(|msg| {
            if let V2ChatContent::ProofFolder { quest_id, claimant, folder_id, .. } = &msg.content {
                // Get attention on this quest per member
                let quest_attention = state_read.attention.attention.get(quest_id.as_str());
                // Get existing blessings for this proof
                let blessing_key = (quest_id.clone(), claimant.clone());
                let blessing_info = state_read.chat.proof_blessings.get(&blessing_key);
                let blessed_members: std::collections::HashSet<&str> = blessing_info
                    .map(|info| info.blessings.iter().map(|b| b.blesser.as_str()).collect())
                    .unwrap_or_default();

                let mut pending: Vec<(String, String, u64)> = Vec::new();
                if let Some(att_map) = quest_attention {
                    for (member_id, millis) in att_map {
                        // Skip the claimant themselves and already-blessed members
                        if member_id != claimant && !blessed_members.contains(member_id.as_str()) && *millis > 0 {
                            pending.push((
                                member_id.clone(),
                                member_name(member_id),
                                *millis,
                            ));
                        }
                    }
                }
                if !pending.is_empty() {
                    Some((folder_id.clone(), pending))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    // Get quests for quest selector
    let quests_for_selector: Vec<(String, String)> = if *showing_quest_selector.read() {
        state_read.quests.quests.iter()
            .filter(|(_, q)| q.claims.iter().any(|c| c.claimant == member) || q.creator == member)
            .map(|(id, q)| (id.clone(), q.title.clone()))
            .take(6)
            .collect()
    } else {
        Vec::new()
    };

    rsx! {
        div { class: "v2-chat",
            // Header
            div { class: "v2-chat-header",
                span { class: "v2-chat-realm-name", "{realm_name}" }
                span { class: "v2-chat-msg-count", "{msg_count} msgs" }
            }
            // Messages (scrollable)
            div { class: "v2-chat-messages",
                if messages.is_empty() {
                    div { class: "mini-empty", "No messages yet" }
                }
                for msg in messages.iter().rev() {
                    {
                        match &msg.content {
                            V2ChatContent::Text(text) => {
                                let sender = &msg.sender;
                                let color_class = &msg.color_class;
                                rsx! {
                                    div { class: "mini-chat-msg",
                                        span { class: "mini-chat-sender {color_class}", "{sender}" }
                                        span { class: "mini-chat-text", "{text}" }
                                    }
                                }
                            }
                            V2ChatContent::ProofFolder { quest_title, artifact_count, folder_id, .. } => {
                                let sender = &msg.sender;
                                let color_class = &msg.color_class;
                                let label = format!("[Proof Folder: {} ({} files)]", quest_title, artifact_count);
                                let pending = gratitude_data.get(folder_id.as_str());
                                rsx! {
                                    div { class: "mini-chat-msg",
                                        span { class: "mini-chat-sender {color_class}", "{sender}" }
                                        span { class: "mini-chat-text", "{label}" }
                                    }
                                    if let Some(pending_members) = pending {
                                        div { class: "v2-gratitude-section",
                                            for (_member_id, name, millis) in pending_members.iter() {
                                                {
                                                    let duration = format_duration_millis(*millis);
                                                    rsx! {
                                                        div { class: "v2-gratitude-row",
                                                            span { class: "v2-gratitude-member", "{name}" }
                                                            span { class: "v2-gratitude-duration", "{duration}" }
                                                            button { class: "v2-btn-gratitude", "Release Gratitude" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            V2ChatContent::Blessing { claimant_name, duration_text } => {
                                let sender = &msg.sender;
                                let color_class = &msg.color_class;
                                let text = format!("[Blessed {} ({})]", claimant_name, duration_text);
                                rsx! {
                                    div { class: "mini-chat-msg",
                                        span { class: "mini-chat-sender {color_class}", "{sender}" }
                                        span { class: "mini-chat-text", "{text}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Quest selector popup
            if *showing_quest_selector.read() {
                div { class: "v2-quest-selector",
                    div { class: "mini-section-label", "SELECT QUEST FOR PROOF" }
                    for (quest_id, quest_title) in quests_for_selector.iter() {
                        button {
                            class: "v2-btn v2-quest-selector-item",
                            onclick: {
                                let _quest_id = quest_id.clone();
                                move |_| {
                                    showing_quest_selector.set(false);
                                    show_action_menu.set(false);
                                }
                            },
                            "{quest_title}"
                        }
                    }
                    if quests_for_selector.is_empty() {
                        div { class: "mini-empty", "No quests available" }
                    }
                }
            }
            // Action menu popup
            if *show_action_menu.read() && !*showing_quest_selector.read() {
                div { class: "v2-chat-action-menu",
                    button {
                        class: "v2-btn",
                        onclick: move |_| {
                            // Visual only - would trigger artifact upload
                        },
                        "Artifact"
                    }
                    button {
                        class: "v2-btn",
                        onclick: move |_| {
                            // Visual only - would trigger document creation
                        },
                        "Document"
                    }
                    button {
                        class: "v2-btn",
                        onclick: move |_| {
                            showing_quest_selector.set(true);
                        },
                        "Proof of Service"
                    }
                }
            }
            // Input row (pinned bottom)
            div { class: "v2-chat-input-row",
                button {
                    class: "v2-btn v2-chat-plus-btn",
                    onclick: move |_| {
                        show_action_menu.toggle();
                        showing_quest_selector.set(false);
                    },
                    "+"
                }
                input {
                    class: "v2-input v2-chat-input",
                    r#type: "text",
                    placeholder: "Message...",
                    value: "{draft}",
                    oninput: move |e| draft.set(e.value()),
                }
                button {
                    class: "v2-btn v2-btn-primary",
                    disabled: draft.read().is_empty(),
                    onclick: move |_| {
                        draft.set(String::new());
                    },
                    "Send"
                }
            }
        }
    }
}

// ============================================================================
// V2 PROOF SCREEN - Markdown narrative + artifact grid + submit
// ============================================================================

/// Proof editor screen: shows active draft with rendered Markdown or submitted proofs
#[component]
fn V2ProofScreen(state: Signal<AppState>, member: String) -> Element {
    let mut view_mode = use_signal(|| super::PreviewViewMode::Rendered);

    let state_read = state.read();

    // Check for active draft from event-driven state
    let active_draft = state_read.member_proof_drafts.draft_for_member(&member).cloned();

    // Get quest title for the draft
    let quest_title = active_draft.as_ref().and_then(|d| {
        state_read.quests.quests.get(&d.quest_id).map(|q| q.title.clone())
    }).unwrap_or_else(|| {
        active_draft.as_ref().map(|d| short_id(&d.quest_id)).unwrap_or_default()
    });

    // Get submitted tokens
    let tokens = state_read.tokens.tokens_for_member(&member);

    // Pre-render narrative HTML if we have markdown content
    let rendered_html = active_draft.as_ref().and_then(|draft| {
        if !draft.narrative.is_empty() && *view_mode.read() == super::PreviewViewMode::Rendered {
            Some(render_draft_narrative_with_images(&draft.narrative, &draft.artifacts))
        } else {
            None
        }
    });

    let mode = *view_mode.read();

    rsx! {
        div { class: "v2-proof",
            if let Some(ref draft) = active_draft {
                // Active proof draft
                div { class: "v2-proof-header",
                    span { class: "v2-proof-title", "PROOF OF SERVICE" }
                    span { class: "v2-proof-quest-title", "{quest_title}" }
                    button { class: "v2-btn v2-proof-close", "X" }
                }
                // Narrative section with Markdown rendering
                div { class: "v2-proof-narrative",
                    div { class: "v2-proof-narrative-header",
                        label { class: "v2-proof-label", "Your Story" }
                        if !draft.narrative.is_empty() {
                            button {
                                class: "v2-btn v2-proof-view-toggle",
                                onclick: move |_| {
                                    view_mode.set(if mode == super::PreviewViewMode::Rendered {
                                        super::PreviewViewMode::Raw
                                    } else {
                                        super::PreviewViewMode::Rendered
                                    });
                                },
                                if mode == super::PreviewViewMode::Rendered { "Raw" } else { "Rendered" }
                            }
                        }
                    }
                    // Markdown content area
                    div { class: "v2-proof-narrative-content",
                        if let Some(ref html) = rendered_html {
                            // Rendered markdown with embedded images
                            div {
                                class: "v2-proof-narrative-rendered",
                                dangerous_inner_html: "{html}",
                            }
                        } else if !draft.narrative.is_empty() {
                            // Raw markdown view
                            pre { class: "v2-proof-narrative-raw", "{draft.narrative}" }
                        } else if draft.narrative_length > 0 {
                            // Fallback: no narrative text available, show char count
                            div { class: "v2-proof-narrative-placeholder",
                                "~{draft.narrative_length} chars written..."
                            }
                        } else {
                            div { class: "v2-proof-narrative-placeholder",
                                "Waiting for narrative..."
                            }
                        }
                    }
                }
                // Artifact grid
                div { class: "v2-proof-artifacts",
                    div { class: "v2-proof-artifacts-header",
                        span { class: "v2-proof-label", "Evidence" }
                        span { class: "v2-proof-file-count", "{draft.artifacts.len()} files" }
                    }
                    if !draft.artifacts.is_empty() {
                        div { class: "v2-proof-artifact-grid",
                            for artifact in draft.artifacts.iter() {
                                {
                                    let has_preview = artifact.data_url.is_some() && artifact.is_image();
                                    rsx! {
                                        div { class: "v2-proof-artifact-thumb",
                                            if has_preview {
                                                if let Some(ref url) = artifact.data_url {
                                                    img {
                                                        class: "v2-proof-artifact-img",
                                                        src: "{url}",
                                                        alt: artifact.caption.as_deref().unwrap_or(&artifact.name),
                                                    }
                                                }
                                            } else if artifact.is_video() {
                                                span { class: "v2-proof-artifact-icon v2-proof-artifact-video", ">" }
                                            } else {
                                                span { class: "v2-proof-artifact-icon", ">" }
                                            }
                                            span { class: "v2-proof-artifact-name", "{artifact.name}" }
                                            span { class: "v2-proof-artifact-size",
                                                {format_artifact_size(artifact.size)}
                                            }
                                            button { class: "v2-btn v2-proof-artifact-remove", "x" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    button { class: "v2-btn", "+ Add File" }
                }
                // Action buttons
                div { class: "v2-proof-actions",
                    button { class: "v2-btn", "Cancel" }
                    button { class: "v2-btn v2-btn-primary", "Submit Proof" }
                }
            } else {
                // No active draft - show submitted proofs
                div { class: "mini-section-label", "PROOF EDITOR" }
                div { class: "v2-proof-no-draft", "No active proof draft" }
                if !tokens.is_empty() {
                    div { class: "v2-proof-submitted",
                        div { class: "mini-subsection-label", "SUBMITTED" }
                        for token in tokens.iter().take(5) {
                            div { class: "v2-proof-submitted-row",
                                span { class: "v2-proof-submitted-quest", "{token.quest_title}" }
                                span { class: "mini-proof-badge", "{token.artifact_count} files" }
                                span { class: "v2-proof-submitted-value", "{token.formatted_value()}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render markdown narrative with draft artifact image references replaced by data URLs.
///
/// Works with `DraftArtifactInfo` (from per-member draft state) instead of `ProofArtifactStateItem`.
/// Transforms `![caption](artifact:ID)` syntax to `![caption](data:...)` for display.
fn render_draft_narrative_with_images(narrative: &str, artifacts: &[DraftArtifactInfo]) -> String {
    use pulldown_cmark::{html, Event, Options, Parser, Tag};
    use std::collections::HashMap;

    // Build lookup map: artifact_id -> data_url
    let artifact_map: HashMap<&str, Option<&str>> = artifacts
        .iter()
        .map(|a| (a.artifact_id.as_str(), a.data_url.as_deref()))
        .collect();

    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(narrative, options);

    // Transform image URLs with artifact: prefix
    let transformed = parser.map(|event| {
        match event {
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
                if dest_url.starts_with("artifact:") {
                    let hash = &dest_url[9..]; // Skip "artifact:" prefix
                    if let Some(Some(data_url)) = artifact_map.get(hash) {
                        return Event::Start(Tag::Image {
                            link_type,
                            dest_url: (*data_url).to_string().into(),
                            title,
                            id,
                        });
                    }
                }
                Event::Start(Tag::Image { link_type, dest_url, title, id })
            }
            other => other,
        }
    });

    let mut html_output = String::new();
    html::push_html(&mut html_output, transformed);
    html_output
}

/// Format artifact size for display in proof screen
fn format_artifact_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    }
}

// ============================================================================
// V2 ARTIFACT SCREEN - Shared files with recall/share buttons
// ============================================================================

/// Artifact screen: shared files with interactive recall/share buttons
#[component]
fn V2ArtifactScreen(state: Signal<AppState>, member: String) -> Element {
    let mut preview_ctx: PreviewContext = use_context();
    let state_read = state.read();

    // Get artifacts shared by this member
    let member_artifacts = state_read.artifacts.artifacts_by_member(&member);
    let member_realms = state_read.realms.realms_for_member(&member);
    let realm_ids: Vec<&str> = member_realms.iter().map(|r| r.realm_id.as_str()).collect();

    // Collect realm artifacts by others (full ArtifactInfo refs)
    let mut shared_artifacts: Vec<&ArtifactInfo> = Vec::new();
    for realm_id in &realm_ids {
        for a in state_read.artifacts.artifacts_for_realm(realm_id) {
            if a.sharer != member {
                shared_artifacts.push(a);
            }
        }
    }
    shared_artifacts.truncate(10);

    let total = member_artifacts.len() + shared_artifacts.len();
    let files_label = if total == 1 { "1 file".to_string() } else { format!("{} files", total) };

    rsx! {
        div { class: "v2-files",
            // Header
            div { class: "v2-files-header",
                span { class: "mini-section-label", "FILES" }
                span { class: "v2-files-count", "{files_label}" }
            }
            // My files section
            if !member_artifacts.is_empty() {
                div { class: "v2-files-section",
                    div { class: "v2-files-section-label", "MY FILES" }
                    for a in member_artifacts.iter().take(10) {
                        {
                            let has_thumb = a.data_url.is_some() && a.has_displayable_image();
                            let size_text = a.formatted_size();
                            let icon = a.icon();
                            let is_shared = a.status == ArtifactStatus::Shared;
                            let file_name = a.name.clone();
                            let artifact_hash = a.artifact_hash.clone();
                            let mime = a.mime_type.clone().unwrap_or_default();
                            let asset_path = a.asset_path.clone();
                            rsx! {
                                div {
                                    class: "v2-file-card v2-file-clickable",
                                    onclick: {
                                        let file_name = file_name.clone();
                                        let artifact_hash = artifact_hash.clone();
                                        let mime = mime.clone();
                                        let asset_path = asset_path.clone();
                                        let has_thumb = has_thumb;
                                        move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                            if mime.starts_with("image/") && has_thumb {
                                                // Image preview
                                                let st = state.read();
                                                let data_url = st.artifacts.artifacts.get(&artifact_hash)
                                                    .and_then(|a| a.data_url.clone());
                                                drop(st);
                                                if let Some(url) = data_url {
                                                    preview_ctx.file.set(Some(PreviewFile {
                                                        name: file_name.clone(),
                                                        mime_type: mime.clone(),
                                                        data_url: Some(url),
                                                        ..Default::default()
                                                    }));
                                                    preview_ctx.view_mode.set(PreviewViewMode::Rendered);
                                                    preview_ctx.is_open.set(true);
                                                }
                                            } else {
                                                // Text/markdown preview
                                                let st = state.read();
                                                let original = st.documents.get_content(&artifact_hash)
                                                    .map(|dc| dc.content.clone())
                                                    .or_else(|| asset_path.as_ref().and_then(|p| super::load_text_file_content(p)))
                                                    .unwrap_or_default();
                                                let content = resolve_artifact_references(&original, &st.artifacts);
                                                let raw_content = resolve_artifact_references_friendly(&original, &st.artifacts);
                                                drop(st);
                                                if !content.is_empty() {
                                                    preview_ctx.file.set(Some(PreviewFile {
                                                        name: file_name.clone(),
                                                        content,
                                                        raw_content,
                                                        mime_type: mime.clone(),
                                                        ..Default::default()
                                                    }));
                                                    preview_ctx.view_mode.set(PreviewViewMode::Rendered);
                                                    preview_ctx.is_open.set(true);
                                                }
                                            }
                                        }
                                    },
                                    // Thumbnail or icon
                                    if has_thumb {
                                        div { class: "v2-file-thumb",
                                            img {
                                                src: "{a.data_url.as_ref().unwrap()}",
                                                alt: "{a.name}",
                                            }
                                        }
                                    } else {
                                        div { class: "v2-file-thumb v2-file-thumb-icon",
                                            span { "{icon}" }
                                        }
                                    }
                                    // Details
                                    div { class: "v2-file-info",
                                        div { class: "v2-file-name", "{a.name}" }
                                        div { class: "v2-file-meta", "{size_text}" }
                                    }
                                    // Action
                                    if is_shared {
                                        button { class: "v2-file-action", "Recall" }
                                    } else {
                                        span { class: "v2-file-recalled-badge", "Recalled" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Shared with me section
            if !shared_artifacts.is_empty() {
                div { class: "v2-files-section",
                    div { class: "v2-files-section-label", "SHARED WITH ME" }
                    for a in shared_artifacts.iter().take(10) {
                        {
                            let has_thumb = a.data_url.is_some() && a.has_displayable_image();
                            let size_text = a.formatted_size();
                            let icon = a.icon();
                            let sharer_name = member_name(&a.sharer);
                            let file_name = a.name.clone();
                            let artifact_hash = a.artifact_hash.clone();
                            let mime = a.mime_type.clone().unwrap_or_default();
                            let asset_path = a.asset_path.clone();
                            rsx! {
                                div {
                                    class: "v2-file-card v2-file-clickable",
                                    onclick: {
                                        let file_name = file_name.clone();
                                        let artifact_hash = artifact_hash.clone();
                                        let mime = mime.clone();
                                        let asset_path = asset_path.clone();
                                        let has_thumb = has_thumb;
                                        move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                            if mime.starts_with("image/") && has_thumb {
                                                // Image preview
                                                let st = state.read();
                                                let data_url = st.artifacts.artifacts.get(&artifact_hash)
                                                    .and_then(|a| a.data_url.clone());
                                                drop(st);
                                                if let Some(url) = data_url {
                                                    preview_ctx.file.set(Some(PreviewFile {
                                                        name: file_name.clone(),
                                                        mime_type: mime.clone(),
                                                        data_url: Some(url),
                                                        ..Default::default()
                                                    }));
                                                    preview_ctx.view_mode.set(PreviewViewMode::Rendered);
                                                    preview_ctx.is_open.set(true);
                                                }
                                            } else {
                                                // Text/markdown preview
                                                let st = state.read();
                                                let original = st.documents.get_content(&artifact_hash)
                                                    .map(|dc| dc.content.clone())
                                                    .or_else(|| asset_path.as_ref().and_then(|p| super::load_text_file_content(p)))
                                                    .unwrap_or_default();
                                                let content = resolve_artifact_references(&original, &st.artifacts);
                                                let raw_content = resolve_artifact_references_friendly(&original, &st.artifacts);
                                                drop(st);
                                                if !content.is_empty() {
                                                    preview_ctx.file.set(Some(PreviewFile {
                                                        name: file_name.clone(),
                                                        content,
                                                        raw_content,
                                                        mime_type: mime.clone(),
                                                        ..Default::default()
                                                    }));
                                                    preview_ctx.view_mode.set(PreviewViewMode::Rendered);
                                                    preview_ctx.is_open.set(true);
                                                }
                                            }
                                        }
                                    },
                                    // Thumbnail or icon
                                    if has_thumb {
                                        div { class: "v2-file-thumb",
                                            img {
                                                src: "{a.data_url.as_ref().unwrap()}",
                                                alt: "{a.name}",
                                            }
                                        }
                                    } else {
                                        div { class: "v2-file-thumb v2-file-thumb-icon",
                                            span { "{icon}" }
                                        }
                                    }
                                    // Details
                                    div { class: "v2-file-info",
                                        div { class: "v2-file-name", "{a.name}" }
                                        div { class: "v2-file-meta",
                                            "{size_text} · from {sharer_name}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if member_artifacts.is_empty() && shared_artifacts.is_empty() {
                div { class: "v2-files-empty", "No files yet" }
            }
        }
    }
}

// ============================================================================
// V2 REALMS SCREEN - Realm cards with editable alias + leave button
// ============================================================================

/// Realms screen: realm cards with click-to-edit alias and leave button
#[component]
fn V2RealmsScreen(state: Signal<AppState>, member: String) -> Element {
    let mut editing_realm = use_signal(|| None::<String>);
    let mut alias_draft = use_signal(String::new);

    let state_read = state.read();
    let realms = state_read.realms.realms_for_member(&member);

    rsx! {
        div { class: "v2-realms",
            div { class: "mini-section-label", "REALMS" }
            if realms.is_empty() {
                div { class: "mini-empty", "No realms yet" }
            }
            for realm in realms.iter() {
                {
                    let realm_id = realm.realm_id.clone();
                    let display_name = state_read.realms.get_display_name(realm);
                    let member_count = realm.members.len();
                    let quest_count = realm.quest_count;
                    let is_editing = editing_realm.read().as_ref() == Some(&realm_id);

                    rsx! {
                        div { class: "v2-realm-card",
                            if is_editing {
                                // Edit mode: input + save/cancel
                                div { class: "v2-realm-edit-row",
                                    input {
                                        class: "v2-input v2-realm-alias-input",
                                        r#type: "text",
                                        value: "{alias_draft}",
                                        oninput: move |e| alias_draft.set(e.value()),
                                        onkeydown: move |e| {
                                            if e.key() == Key::Enter {
                                                editing_realm.set(None);
                                            } else if e.key() == Key::Escape {
                                                editing_realm.set(None);
                                            }
                                        },
                                    }
                                }
                            } else {
                                // Display mode: clickable name
                                div {
                                    class: "v2-realm-name-row",
                                    onclick: {
                                        let rid = realm_id.clone();
                                        let dn = display_name.clone();
                                        move |_| {
                                            alias_draft.set(dn.clone());
                                            editing_realm.set(Some(rid.clone()));
                                        }
                                    },
                                    span { class: "v2-realm-name", "{display_name}" }
                                    span { class: "v2-realm-edit-hint", "click to rename" }
                                }
                            }
                            div { class: "v2-realm-meta",
                                span { class: "mini-realm-stat", "{member_count} members" }
                                span { class: "mini-realm-stat", "{quest_count} quests" }
                            }
                            div { class: "v2-realm-actions",
                                button { class: "v2-btn", "Leave" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// V2 ACTIVITY SCREEN - Recent events (kept for completeness)
// ============================================================================

/// Activity screen: recent events involving this member
#[component]
fn V2ActivityScreen(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let events: Vec<_> = state_read
        .events_for_member(&member)
        .into_iter()
        .take(8)
        .collect();

    rsx! {
        div { class: "mini-activity",
            div { class: "mini-section-label", "ACTIVITY" }
            if events.is_empty() {
                div { class: "mini-empty", "No activity yet" }
            }
            for event in events.iter() {
                div { class: "mini-activity-line",
                    span { class: "mini-activity-tick", "[{event.tick}]" }
                    span { class: "mini-activity-text", "{event.summary}" }
                }
            }
        }
    }
}

/// Replace `artifact:HASH` references in markdown content with data URLs from artifact state.
/// This allows rendered markdown to display embedded images stored as artifacts.
fn resolve_artifact_references(content: &str, artifacts: &ArtifactState) -> String {
    let mut result = content.to_string();
    for (hash, info) in &artifacts.artifacts {
        if let Some(ref data_url) = info.data_url {
            let artifact_ref = format!("artifact:{}", hash);
            if result.contains(&artifact_ref) {
                result = result.replace(&artifact_ref, data_url);
            }
        }
    }
    result
}

/// Replace `artifact:HASH` references with friendly filenames for raw markdown display.
fn resolve_artifact_references_friendly(content: &str, artifacts: &ArtifactState) -> String {
    let mut result = content.to_string();
    for (hash, info) in &artifacts.artifacts {
        let artifact_ref = format!("artifact:{}", hash);
        if result.contains(&artifact_ref) {
            result = result.replace(&artifact_ref, &info.name);
        }
    }
    result
}
