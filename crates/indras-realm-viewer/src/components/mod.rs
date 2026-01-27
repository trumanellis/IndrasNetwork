//! UI Components for Realm Viewer
//!
//! Redesigned 3-panel dashboard with frosted glass controls.

use dioxus::prelude::*;

use crate::playback;
use crate::state::{
    member_name, short_id, AppState, ArtifactInfo, ArtifactStatus, ClaimInfo, DraftArtifact, MemberStats,
    QuestAttention, QuestInfo, QuestStatus, RealmInfo, UploadStatus,
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
                    RealmCard { state, realm: realm.clone() }
                }
                if realms.is_empty() {
                    div { class: "empty-state", "No realms yet" }
                }
            }
        }
    }
}

#[component]
fn RealmCard(state: Signal<AppState>, realm: RealmInfo) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let display_name = state_read.realms.get_display_name(&realm);
    let realm_id = realm.realm_id.clone();
    let realm_id_edit = realm.realm_id.clone();

    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| String::new());

    rsx! {
        div { class: "realm-card",
            if editing() {
                input {
                    class: "alias-input",
                    maxlength: "77",
                    value: "{draft}",
                    autofocus: true,
                    oninput: move |e| draft.set(e.value()),
                    onblur: move |_| {
                        let new_alias = draft.read().clone();
                        state_write.write().realms.set_alias(&realm_id, &new_alias);
                        editing.set(false);
                    },
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            let new_alias = draft.read().clone();
                            state_write.write().realms.set_alias(&realm_id_edit, &new_alias);
                            editing.set(false);
                        } else if e.key() == Key::Escape {
                            editing.set(false);
                        }
                    },
                }
            } else {
                div {
                    class: "realm-name editable",
                    onclick: move |_| {
                        let current = state.read().realms.get_alias(&realm.realm_id)
                            .unwrap_or("")
                            .to_string();
                        draft.set(current);
                        editing.set(true);
                    },
                    "{display_name}"
                }
            }
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
            div { class: "quests-chat-row",
                div { class: "quests-artifacts-column",
                    QuestListPanel { state }
                    SharedArtifactGalleryPanel { state }
                }
                ChatPanel { state }
            }
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
// SHARED ARTIFACT GALLERY PANEL
// ============================================================================

/// Gallery panel showing all shared artifacts in the overview dashboard
#[component]
fn SharedArtifactGalleryPanel(state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let artifacts = state_read.artifacts.all_artifacts();
    let shared_count = state_read.artifacts.total_shared;
    let recalled_count = state_read.artifacts.total_recalled;

    rsx! {
        div { class: "shared-artifact-gallery-panel",
            div { class: "artifact-gallery-header",
                span { class: "artifact-gallery-title", "Shared Artifacts" }
                div { class: "artifact-gallery-stats",
                    span { class: "artifact-stat shared", "{shared_count} shared" }
                    if recalled_count > 0 {
                        span { class: "artifact-stat recalled", "{recalled_count} recalled" }
                    }
                }
            }
            div { class: "shared-artifact-grid",
                for artifact in artifacts.iter().take(12) {
                    SharedArtifactCard { artifact: (*artifact).clone() }
                }
                if artifacts.is_empty() {
                    div { class: "empty-state", "No shared artifacts yet" }
                }
            }
        }
    }
}

/// Convert a local file path to a data URL for display in webview
fn load_image_as_data_url(path: &str) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    // Build absolute path
    let full_path = if path.starts_with('/') {
        std::path::PathBuf::from(path)
    } else {
        std::env::current_dir().ok()?.join(path)
    };

    // Read file
    let data = std::fs::read(&full_path).ok()?;

    // Determine mime type from extension
    let mime = match full_path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };

    // Encode as data URL
    let encoded = STANDARD.encode(&data);
    Some(format!("data:{};base64,{}", mime, encoded))
}

/// Card displaying a shared artifact with its status
#[component]
fn SharedArtifactCard(artifact: ArtifactInfo) -> Element {
    let status_class = artifact.status.css_class();
    let status_text = artifact.status.display_name();
    let sharer_name = member_name(&artifact.sharer);
    let color_class = member_color_class(&artifact.sharer);
    let icon = artifact.icon();
    let size = artifact.formatted_size();
    let has_image = artifact.has_displayable_image();

    // Load image as data URL for webview display
    let image_url = if has_image {
        artifact.asset_path.as_ref().and_then(|path| load_image_as_data_url(path))
    } else {
        None
    };

    rsx! {
        div { class: "shared-artifact-card {status_class}",
            if let Some(ref url) = image_url {
                div { class: "artifact-card-thumbnail",
                    img {
                        src: "{url}",
                        alt: "{artifact.name}",
                    }
                }
            } else {
                div { class: "artifact-card-icon", "{icon}" }
            }
            div { class: "artifact-card-info",
                div { class: "artifact-card-name", "{artifact.name}" }
                div { class: "artifact-card-meta",
                    span { class: "artifact-card-size", "{size}" }
                    span { class: "artifact-card-sharer {color_class}", "by {sharer_name}" }
                }
            }
            div { class: "artifact-card-status {status_class}", "{status_text}" }
            if artifact.status == ArtifactStatus::Recalled {
                div { class: "artifact-card-recall-overlay" }
            }
        }
    }
}

/// Gallery panel showing artifacts for a specific member in POV dashboard
#[component]
fn MyArtifactGalleryPanel(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();

    // Get artifacts shared by this member
    let my_artifacts = state_read.artifacts.artifacts_by_member(&member);
    let my_shared_count = my_artifacts.iter().filter(|a| a.status == ArtifactStatus::Shared).count();
    let my_recalled_count = my_artifacts.iter().filter(|a| a.status == ArtifactStatus::Recalled).count();

    // Get all realm artifacts (for artifacts shared with me)
    let member_realms: Vec<_> = state_read.realms.realms_for_member(&member)
        .into_iter()
        .map(|r| r.realm_id.clone())
        .collect();

    let realm_artifacts: Vec<_> = state_read.artifacts.all_artifacts()
        .into_iter()
        .filter(|a| member_realms.contains(&a.realm_id) && a.sharer != member)
        .take(6)
        .collect();

    rsx! {
        div { class: "my-artifact-gallery-panel",
            // My shared artifacts section
            div { class: "artifact-section",
                div { class: "artifact-section-header",
                    span { class: "artifact-section-title", "My Shared Files" }
                    div { class: "artifact-section-stats",
                        span { class: "artifact-stat shared", "{my_shared_count} active" }
                        if my_recalled_count > 0 {
                            span { class: "artifact-stat recalled", "{my_recalled_count} recalled" }
                        }
                    }
                }
                div { class: "my-artifact-grid",
                    for artifact in my_artifacts.iter().take(6) {
                        MyArtifactCard { artifact: (*artifact).clone() }
                    }
                    if my_artifacts.is_empty() {
                        div { class: "empty-state-small", "No files shared" }
                    }
                }
            }

            // Files shared with me section
            if !realm_artifacts.is_empty() {
                div { class: "artifact-section",
                    div { class: "artifact-section-header",
                        span { class: "artifact-section-title", "Shared With Me" }
                        span { class: "artifact-section-count", "{realm_artifacts.len()} files" }
                    }
                    div { class: "my-artifact-grid",
                        for artifact in realm_artifacts.iter() {
                            SharedArtifactCard { artifact: (*artifact).clone() }
                        }
                    }
                }
            }
        }
    }
}

/// Card for artifacts in the POV view with recall action
#[component]
fn MyArtifactCard(artifact: ArtifactInfo) -> Element {
    let status_class = artifact.status.css_class();
    let icon = artifact.icon();
    let size = artifact.formatted_size();
    let can_recall = artifact.status == ArtifactStatus::Shared;

    rsx! {
        div { class: "my-artifact-card {status_class}",
            div { class: "artifact-card-icon", "{icon}" }
            div { class: "artifact-card-info",
                div { class: "artifact-card-name", "{artifact.name}" }
                div { class: "artifact-card-size", "{size}" }
            }
            if can_recall {
                button {
                    class: "artifact-recall-btn",
                    title: "Recall this artifact",
                    "↩"
                }
            } else {
                div { class: "artifact-recalled-badge", "Recalled" }
            }
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
    let mut show_action_menu = use_signal(|| false);

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
                div { class: "chat-input-wrapper",
                    button {
                        class: "chat-action-btn",
                        onclick: move |_| show_action_menu.set(!show_action_menu()),
                        "+"
                    }
                    if show_action_menu() {
                        div { class: "chat-action-menu",
                            button {
                                class: "action-menu-item",
                                onclick: move |_| show_action_menu.set(false),
                                span { class: "action-menu-icon", "📎" }
                                span { "Artifact" }
                            }
                            button {
                                class: "action-menu-item",
                                onclick: move |_| show_action_menu.set(false),
                                span { class: "action-menu-icon", "📄" }
                                span { "Document" }
                            }
                            button {
                                class: "action-menu-item",
                                onclick: move |_| show_action_menu.set(false),
                                span { class: "action-menu-icon", "✓" }
                                span { "Proof of Service" }
                            }
                        }
                    }
                }
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

/// Chat message item component with edit/delete support
#[component]
fn ChatMessageItem(message: crate::state::ChatMessage) -> Element {
    let name = member_name(&message.member);
    let color_class = member_color_class(&message.member);

    // Handle deleted messages
    if message.is_deleted {
        return rsx! {
            div { class: "chat-message deleted-message",
                span { class: "chat-tick", "[{message.tick}]" }
                span { class: "deleted-text", "[message deleted]" }
            }
        };
    }

    match &message.message_type {
        crate::state::ChatMessageType::Text => {
            let is_edited = message.is_edited();

            rsx! {
                div { class: "chat-message text-message",
                    div { class: "chat-message-row",
                        span { class: "chat-tick", "[{message.tick}]" }
                        span { class: "chat-sender {color_class}", "{name}" }
                        span { class: "chat-content", "{message.content}" }
                        if is_edited {
                            span { class: "edited-badge", "(edited)" }
                        }
                    }
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
        crate::state::ChatMessageType::ProofFolderSubmitted { artifact_count, narrative_preview, .. } => {
            let preview = if narrative_preview.is_empty() {
                format!("{} files", artifact_count)
            } else {
                narrative_preview.clone()
            };
            rsx! {
                div { class: "chat-message proof-folder-message",
                    span { class: "chat-tick", "[{message.tick}]" }
                    span { class: "chat-icon", "📂" }
                    span { class: "chat-sender {color_class}", "{name}" }
                    div { class: "proof-content",
                        span { class: "proof-label", "Proof folder: " }
                        span { class: "proof-preview", "{preview}" }
                    }
                }
            }
        }
    }
}

/// Editable chat message item with edit/delete buttons and version history
#[component]
fn EditableChatMessageItem(
    state: Signal<AppState>,
    message: crate::state::ChatMessage,
    current_member: String,
) -> Element {
    let _state = state;
    let name = member_name(&message.member);
    let color_class = member_color_class(&message.member);
    let message_id = message.id.clone();

    let mut editing = use_signal(|| false);
    let mut edit_draft = use_signal(|| message.content.clone());
    let mut show_history = use_signal(|| false);

    let can_edit = message.can_edit(&current_member);
    let is_edited = message.is_edited();
    let version_count = message.version_count();

    // Handle deleted messages
    if message.is_deleted {
        return rsx! {
            div { class: "chat-message deleted-message",
                span { class: "chat-tick", "[{message.tick}]" }
                span { class: "deleted-text", "[message deleted]" }
                if is_edited {
                    button {
                        class: "show-history-btn",
                        onclick: move |_| show_history.set(!show_history()),
                        if show_history() { "Hide history" } else { "Show history" }
                    }
                }
                if show_history() && is_edited {
                    MessageVersionHistory { versions: message.versions.clone() }
                }
            }
        };
    }

    match &message.message_type {
        crate::state::ChatMessageType::Text => {
            if editing() {
                // Edit mode
                rsx! {
                    ChatMessageEditor {
                        message_id: message_id.clone(),
                        draft: edit_draft,
                        on_save: move |_new_content: String| {
                            // In a real app, this would emit an event to update the backend
                            // For now, we just close the editor
                            editing.set(false);
                        },
                        on_cancel: move |_| {
                            edit_draft.set(message.content.clone());
                            editing.set(false);
                        },
                    }
                }
            } else {
                // Normal display
                rsx! {
                    div { class: "chat-message text-message editable-message",
                        div { class: "chat-message-row",
                            span { class: "chat-tick", "[{message.tick}]" }
                            span { class: "chat-sender {color_class}", "{name}" }
                            span { class: "chat-content", "{message.content}" }
                        }
                        div { class: "message-actions",
                            if is_edited {
                                button {
                                    class: "edited-badge-btn",
                                    onclick: move |_| show_history.set(!show_history()),
                                    "edited"
                                    span { class: "version-count", " ({version_count})" }
                                }
                            }
                            if can_edit {
                                button {
                                    class: "edit-btn",
                                    onclick: move |_| {
                                        edit_draft.set(message.content.clone());
                                        editing.set(true);
                                    },
                                    "Edit"
                                }
                                button {
                                    class: "delete-btn",
                                    onclick: move |_| {
                                        // In a real app, this would emit a delete event
                                        // The UI would update when the event is processed
                                    },
                                    "Delete"
                                }
                            }
                        }
                        if show_history() && is_edited {
                            MessageVersionHistory { versions: message.versions.clone() }
                        }
                    }
                }
            }
        }
        // Non-text messages don't support editing
        _ => {
            rsx! {
                ChatMessageItem { message }
            }
        }
    }
}

/// Inline message editor component
#[component]
fn ChatMessageEditor(
    message_id: String,
    draft: Signal<String>,
    on_save: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "message-editor",
            textarea {
                class: "message-edit-input",
                value: "{draft}",
                autofocus: true,
                oninput: move |e| draft.set(e.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Escape {
                        on_cancel.call(());
                    }
                },
            }
            div { class: "message-edit-actions",
                button {
                    class: "edit-cancel-btn",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                button {
                    class: "edit-save-btn",
                    onclick: move |_| on_save.call(draft.read().clone()),
                    "Save"
                }
            }
        }
    }
}

/// Expandable version history component
#[component]
fn MessageVersionHistory(versions: Vec<crate::state::MessageVersion>) -> Element {
    rsx! {
        div { class: "version-history",
            div { class: "version-history-header", "Edit History" }
            for (i, version) in versions.iter().enumerate().rev() {
                div { class: "version-item",
                    span { class: "version-number", "v{i + 1}" }
                    span { class: "version-content", "{version.content}" }
                    span { class: "version-time", "[tick {version.edited_at}]" }
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
                            div { class: "pov-quests-artifacts-column",
                                MyQuestsList { state }
                                MyArtifactGalleryPanel { state, member: member.clone() }
                            }
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
                        state,
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
fn QuestCardWithRealm(
    state: Signal<AppState>,
    quest: QuestInfo,
    attention: QuestAttention,
    realm_name: Option<String>,
    max_attention: u64
) -> Element {
    let mut state_write = state;
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

    // Show submit proof button for non-completed quests
    let show_submit_btn = quest.status != QuestStatus::Completed;
    let quest_id = quest.quest_id.clone();
    let quest_title = quest.title.clone();

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
            // Submit Proof button
            if show_submit_btn {
                button {
                    class: "submit-proof-btn",
                    onclick: move |e| {
                        e.stop_propagation();
                        state_write.write().proof_folder.open_for_quest(
                            quest_id.clone(),
                            quest_title.clone()
                        );
                    },
                    span { class: "submit-proof-icon", "✓" }
                    span { "Submit Proof" }
                }
            }
        }
    }
}

/// Chat panel that can switch to proof folder editor
#[component]
fn MyChatPanel(state: Signal<AppState>, member: String) -> Element {
    let is_editor_open = state.read().proof_folder.is_open();

    if is_editor_open {
        rsx! {
            ProofFolderEditor { state, member }
        }
    } else {
        rsx! {
            MyChatPanelInner { state, member }
        }
    }
}

/// Inner chat panel component (shown when proof editor is closed)
#[component]
fn MyChatPanelInner(state: Signal<AppState>, member: String) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let messages = state_read.chat.recent_messages(15);
    let message_count = state_read.chat.total_messages;
    let mut draft = use_signal(|| String::new());
    let mut show_action_menu = use_signal(|| false);

    // Get the first realm for this member (for alias editing)
    let member_realms: Vec<_> = state_read.realms.realms_for_member(&member).into_iter().cloned().collect();
    let current_realm = member_realms.first().cloned();
    let realm_id = current_realm.as_ref().map(|r| r.realm_id.clone());
    let realm_id_edit = realm_id.clone();
    let realm_id_blur = realm_id.clone();

    // Get display name for the chat header
    let display_name = current_realm.as_ref()
        .map(|r| state_read.realms.get_display_name(r))
        .unwrap_or_else(|| "Realm Chat".to_string());

    let mut editing_alias = use_signal(|| false);
    let mut alias_draft = use_signal(|| String::new());

    // Get quests for the quest selector
    let showing_quest_selector = state_read.proof_folder.showing_quest_selector;
    let available_quests: Vec<_> = state_read.quests.quests.values()
        .filter(|q| q.status != QuestStatus::Completed)
        .cloned()
        .collect();

    rsx! {
        div { class: "my-chat-panel",
            div { class: "chat-header",
                if editing_alias() {
                    input {
                        class: "alias-input",
                        maxlength: "77",
                        value: "{alias_draft}",
                        autofocus: true,
                        oninput: move |e| alias_draft.set(e.value()),
                        onblur: move |_| {
                            if let Some(ref rid) = realm_id_blur {
                                let new_alias = alias_draft.read().clone();
                                state_write.write().realms.set_alias(rid, &new_alias);
                            }
                            editing_alias.set(false);
                        },
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                if let Some(ref rid) = realm_id_edit {
                                    let new_alias = alias_draft.read().clone();
                                    state_write.write().realms.set_alias(rid, &new_alias);
                                }
                                editing_alias.set(false);
                            } else if e.key() == Key::Escape {
                                editing_alias.set(false);
                            }
                        },
                    }
                } else {
                    span {
                        class: "chat-title editable",
                        onclick: move |_| {
                            if let Some(ref rid) = realm_id {
                                let current = state.read().realms.get_alias(rid)
                                    .unwrap_or("")
                                    .to_string();
                                alias_draft.set(current);
                                editing_alias.set(true);
                            }
                        },
                        "{display_name}"
                    }
                    if current_realm.is_some() {
                        span { class: "edit-hint", "(click to rename)" }
                    }
                }
                span { class: "chat-stats", "{message_count} msgs" }
            }
            div { class: "my-chat-messages",
                for msg in messages.iter() {
                    EditableChatMessageItem {
                        state,
                        message: (*msg).clone(),
                        current_member: member.clone(),
                    }
                }
                if messages.is_empty() {
                    div { class: "empty-state", "No messages yet" }
                }
            }
            div { class: "chat-input-container",
                div { class: "chat-input-wrapper",
                    button {
                        class: "chat-action-btn",
                        onclick: move |_| show_action_menu.set(!show_action_menu()),
                        "+"
                    }
                    if show_action_menu() {
                        div { class: "chat-action-menu",
                            button {
                                class: "action-menu-item",
                                onclick: move |_| show_action_menu.set(false),
                                span { class: "action-menu-icon", "📎" }
                                span { "Artifact" }
                            }
                            button {
                                class: "action-menu-item",
                                onclick: move |_| show_action_menu.set(false),
                                span { class: "action-menu-icon", "📄" }
                                span { "Document" }
                            }
                            button {
                                class: "action-menu-item",
                                onclick: move |_| {
                                    show_action_menu.set(false);
                                    state_write.write().proof_folder.show_quest_selector();
                                },
                                span { class: "action-menu-icon", "✓" }
                                span { "Proof of Service" }
                            }
                        }
                    }
                    // Quest selector dropdown
                    if showing_quest_selector {
                        div { class: "quest-selector-dropdown",
                            div { class: "quest-selector-header",
                                span { "Select Quest" }
                                button {
                                    class: "quest-selector-close",
                                    onclick: move |_| {
                                        state_write.write().proof_folder.hide_quest_selector();
                                    },
                                    "×"
                                }
                            }
                            div { class: "quest-selector-list",
                                for quest in available_quests.iter() {
                                    {
                                        let quest_id = quest.quest_id.clone();
                                        let quest_title = quest.title.clone();
                                        rsx! {
                                            button {
                                                class: "quest-selector-item",
                                                onclick: move |_| {
                                                    state_write.write().proof_folder.open_for_quest(
                                                        quest_id.clone(),
                                                        quest_title.clone()
                                                    );
                                                },
                                                span { class: "quest-selector-title", "{quest.title}" }
                                            }
                                        }
                                    }
                                }
                                if available_quests.is_empty() {
                                    div { class: "empty-state", "No open quests" }
                                }
                            }
                        }
                    }
                }
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

// ============================================================================
// PROOF FOLDER EDITOR
// ============================================================================

/// Main proof folder editor component
#[component]
fn ProofFolderEditor(state: Signal<AppState>, member: String) -> Element {
    let mut state_write = state;
    let state_read = state.read();

    let draft = state_read.proof_folder.current_draft.clone();
    let quest_title = draft.as_ref().map(|d| d.quest_title.clone()).unwrap_or_default();
    let narrative = draft.as_ref().map(|d| d.narrative.clone()).unwrap_or_default();
    let artifacts = draft.as_ref().map(|d| d.artifacts.clone()).unwrap_or_default();

    let mut narrative_signal = use_signal(|| narrative.clone());
    let mut show_discard_confirm = use_signal(|| false);

    // Check if there are unsaved changes
    let has_changes = state_read.proof_folder.has_unsaved_changes()
        || !narrative_signal.read().is_empty()
        || !artifacts.is_empty();

    rsx! {
        div { class: "proof-folder-editor",
            // Header
            ProofEditorHeader {
                quest_title: quest_title.clone(),
                on_close: move |_| {
                    if has_changes {
                        show_discard_confirm.set(true);
                    } else {
                        state_write.write().proof_folder.close();
                    }
                },
            }

            // Narrative editor
            NarrativeEditor {
                value: narrative_signal(),
                on_change: move |new_value: String| {
                    narrative_signal.set(new_value.clone());
                    if let Some(draft) = state_write.write().proof_folder.draft_mut() {
                        draft.set_narrative(new_value);
                    }
                },
            }

            // Artifact gallery
            ArtifactGallery {
                state,
                artifacts: artifacts.clone(),
            }

            // Action buttons
            ProofEditorActions {
                state,
                can_submit: !narrative_signal.read().is_empty() || !artifacts.is_empty(),
            }

            // Discard confirmation dialog
            if show_discard_confirm() {
                div { class: "discard-confirm-overlay",
                    onclick: move |_| show_discard_confirm.set(false),
                    div {
                        class: "discard-confirm-dialog",
                        onclick: move |e| e.stop_propagation(),
                        h3 { "Discard Changes?" }
                        p { "You have unsaved changes. Are you sure you want to discard them?" }
                        div { class: "discard-confirm-actions",
                            button {
                                class: "discard-btn-cancel",
                                onclick: move |_| show_discard_confirm.set(false),
                                "Keep Editing"
                            }
                            button {
                                class: "discard-btn-confirm",
                                onclick: move |_| {
                                    show_discard_confirm.set(false);
                                    state_write.write().proof_folder.close();
                                },
                                "Discard"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Header for the proof editor
#[component]
fn ProofEditorHeader(quest_title: String, on_close: EventHandler<()>) -> Element {
    rsx! {
        div { class: "proof-editor-header",
            div { class: "proof-editor-title-section",
                span { class: "proof-editor-label", "PROOF OF SERVICE" }
                h3 { class: "proof-editor-quest-title", "{quest_title}" }
            }
            button {
                class: "proof-editor-close-btn",
                onclick: move |_| on_close.call(()),
                "×"
            }
        }
    }
}

/// Rich text narrative editor with WYSIWYG toolbar
#[component]
fn NarrativeEditor(value: String, on_change: EventHandler<String>) -> Element {
    rsx! {
        div { class: "narrative-editor",
            div { class: "narrative-label", "Your Story" }
            // Toolbar
            div { class: "editor-toolbar",
                button {
                    class: "toolbar-btn",
                    title: "Bold",
                    onclick: move |_| {
                        // In a real app, this would use execCommand or similar
                    },
                    "B"
                }
                button {
                    class: "toolbar-btn toolbar-btn-italic",
                    title: "Italic",
                    onclick: move |_| {},
                    "I"
                }
                button {
                    class: "toolbar-btn",
                    title: "Heading",
                    onclick: move |_| {},
                    "H"
                }
                div { class: "toolbar-divider" }
                button {
                    class: "toolbar-btn",
                    title: "Bullet List",
                    onclick: move |_| {},
                    "•"
                }
                button {
                    class: "toolbar-btn",
                    title: "Numbered List",
                    onclick: move |_| {},
                    "1."
                }
            }
            // Editor area
            textarea {
                class: "narrative-textarea",
                placeholder: "Describe your service... What did you do? How did it help? What did you learn?",
                value: "{value}",
                oninput: move |e| on_change.call(e.value())
            }
        }
    }
}

/// Gallery for managing proof artifacts
#[component]
fn ArtifactGallery(state: Signal<AppState>, artifacts: Vec<DraftArtifact>) -> Element {
    let mut state_write = state;

    rsx! {
        div { class: "artifact-gallery",
            div { class: "artifact-gallery-header",
                span { class: "artifact-gallery-label", "Evidence" }
                span { class: "artifact-count", "{artifacts.len()} files" }
            }
            div { class: "artifact-grid",
                for artifact in artifacts.iter() {
                    ArtifactThumbnail {
                        artifact: artifact.clone(),
                        on_remove: move |id: String| {
                            if let Some(draft) = state_write.write().proof_folder.draft_mut() {
                                draft.remove_artifact(&id);
                            }
                        },
                    }
                }
                // Add artifact button
                button {
                    class: "add-artifact-btn",
                    onclick: move |_| {
                        // In a real app, this would open a file picker
                        // For now, add a mock artifact
                        let mock_id = format!("artifact-{}", std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis());
                        let mock = DraftArtifact::new(
                            mock_id,
                            "example.pdf".to_string(),
                            1024 * 50,
                            Some("application/pdf".to_string())
                        );
                        if let Some(draft) = state_write.write().proof_folder.draft_mut() {
                            draft.add_artifact(mock);
                        }
                    },
                    div { class: "add-artifact-icon", "+" }
                    span { class: "add-artifact-text", "Add File" }
                }
            }
        }
    }
}

/// Single artifact thumbnail in the gallery
#[component]
fn ArtifactThumbnail(artifact: DraftArtifact, on_remove: EventHandler<String>) -> Element {
    let artifact_id = artifact.id.clone();
    let is_uploading = matches!(artifact.upload_status, UploadStatus::Uploading { .. });
    let upload_progress = match artifact.upload_status {
        UploadStatus::Uploading { progress } => progress,
        _ => 0.0,
    };

    rsx! {
        div { class: "artifact-thumbnail",
            // Thumbnail content
            div { class: "artifact-thumbnail-content",
                if artifact.is_image() {
                    if let Some(ref url) = artifact.thumbnail_url {
                        img {
                            class: "artifact-image",
                            src: "{url}",
                            alt: "{artifact.name}"
                        }
                    } else {
                        div { class: "artifact-placeholder", "🖼" }
                    }
                } else {
                    div { class: "artifact-file-icon", "{artifact.file_icon()}" }
                }

                // Upload progress overlay
                if is_uploading {
                    div { class: "artifact-upload-overlay",
                        div {
                            class: "artifact-upload-progress",
                            style: "width: {upload_progress}%"
                        }
                    }
                }
            }

            // Remove button
            button {
                class: "artifact-remove-btn",
                onclick: move |_| on_remove.call(artifact_id.clone()),
                "×"
            }

            // File info
            div { class: "artifact-info",
                span { class: "artifact-name", "{artifact.name}" }
                span { class: "artifact-size", "{artifact.formatted_size()}" }
            }
        }
    }
}

/// Action buttons for the proof editor
#[component]
fn ProofEditorActions(state: Signal<AppState>, can_submit: bool) -> Element {
    let mut state_write = state;

    rsx! {
        div { class: "proof-editor-actions",
            button {
                class: "proof-cancel-btn",
                onclick: move |_| {
                    state_write.write().proof_folder.close();
                },
                "Cancel"
            }
            button {
                class: "proof-submit-btn",
                disabled: !can_submit,
                onclick: move |_| {
                    // TODO: Call realm.submit_proof_folder() when backend is connected
                    // For now, just close the editor
                    state_write.write().proof_folder.close();
                },
                "Submit Proof"
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
                    MyRealmCard { state, realm: realm.clone() }
                }
                if realms.is_empty() {
                    div { class: "empty-state", "No realms yet" }
                }
            }
        }
    }
}

#[component]
fn MyRealmCard(state: Signal<AppState>, realm: RealmInfo) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let display_name = state_read.realms.get_display_name(&realm);
    let realm_id = realm.realm_id.clone();
    let realm_id_edit = realm.realm_id.clone();
    let quest_count = realm.quest_count;

    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| String::new());

    rsx! {
        div { class: "realm-card-item",
            if editing() {
                input {
                    class: "alias-input",
                    maxlength: "77",
                    value: "{draft}",
                    autofocus: true,
                    oninput: move |e| draft.set(e.value()),
                    onblur: move |_| {
                        let new_alias = draft.read().clone();
                        state_write.write().realms.set_alias(&realm_id, &new_alias);
                        editing.set(false);
                    },
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            let new_alias = draft.read().clone();
                            state_write.write().realms.set_alias(&realm_id_edit, &new_alias);
                            editing.set(false);
                        } else if e.key() == Key::Escape {
                            editing.set(false);
                        }
                    },
                }
            } else {
                div {
                    class: "realm-card-title editable",
                    onclick: move |_| {
                        let current = state.read().realms.get_alias(&realm.realm_id)
                            .unwrap_or("")
                            .to_string();
                        draft.set(current);
                        editing.set(true);
                    },
                    "{display_name}"
                }
            }
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
