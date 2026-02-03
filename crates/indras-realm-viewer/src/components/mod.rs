//! UI Components for Realm Viewer
//!
//! Redesigned 3-panel dashboard with frosted glass controls.

use dioxus::prelude::*;

use crate::playback;
use crate::state::{
    member_name, short_id, format_duration_millis, AppState, ArtifactInfo, ArtifactStatus, ClaimInfo,
    DraftArtifact, QuestAttention, QuestInfo, QuestStatus, RealmInfo, TokenOfGratitude, UploadStatus,
};
use crate::theme::{ThemeSwitcher, ThemedRoot};

pub mod omni;
pub mod omni_v2;
pub mod scenario_picker;

/// File being previewed in overlay
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreviewFile {
    pub name: String,
    /// Content with artifact references resolved to data URLs (for rendered mode).
    pub content: String,
    /// Content with artifact references resolved to friendly filenames (for raw mode).
    pub raw_content: String,
    pub mime_type: String,
    /// Data URL for image preview (set when previewing an image file).
    pub data_url: Option<String>,
}

/// View mode for markdown preview
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PreviewViewMode {
    #[default]
    Rendered,
    Raw,
}

/// Context for markdown preview overlay
#[derive(Clone, Copy)]
pub struct PreviewContext {
    pub is_open: Signal<bool>,
    pub file: Signal<Option<PreviewFile>>,
    pub view_mode: Signal<PreviewViewMode>,
}

/// Data for proof narrative overlay
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProofNarrativeData {
    pub quest_title: String,
    pub member: String,
    pub narrative: String,
    pub artifacts: Vec<crate::state::ProofArtifactStateItem>,
}

/// Context for proof narrative overlay
#[derive(Clone, Copy)]
pub struct ProofNarrativeContext {
    pub is_open: Signal<bool>,
    pub data: Signal<Option<ProofNarrativeData>>,
}

/// Main application component
#[component]
pub fn App(state: Signal<AppState>) -> Element {
    // Preview overlay state
    let preview_open = use_signal(|| false);
    let preview_file = use_signal(|| None::<PreviewFile>);
    let preview_mode = use_signal(|| PreviewViewMode::Rendered);

    // Proof narrative overlay state
    let proof_narrative_open = use_signal(|| false);
    let proof_narrative_data = use_signal(|| None::<ProofNarrativeData>);
    let proof_narrative_mode = use_signal(|| PreviewViewMode::Rendered);

    // Provide preview context to children
    use_context_provider(|| PreviewContext {
        is_open: preview_open,
        file: preview_file,
        view_mode: preview_mode,
    });

    // Provide proof narrative context to children
    use_context_provider(|| ProofNarrativeContext {
        is_open: proof_narrative_open,
        data: proof_narrative_data,
    });

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

/// Breadcrumb navigation for artifact folders
#[component]
fn ArtifactBreadcrumb(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let nav = &state.read().artifacts.navigation;
    let path = nav.path.clone();
    let is_root = nav.current_folder.is_none();

    rsx! {
        div { class: "artifact-breadcrumb",
            span {
                class: if is_root { "breadcrumb-segment active" } else { "breadcrumb-segment clickable" },
                onclick: move |_| { state_write.write().artifacts.navigate_to(0); },
                "Realm Artifacts"
            }
            for (i, crumb) in path.iter().enumerate() {
                span { class: "breadcrumb-separator", " > " }
                span {
                    class: if i == path.len() - 1 { "breadcrumb-segment active" } else { "breadcrumb-segment clickable" },
                    onclick: move |_| { state_write.write().artifacts.navigate_to(i + 1); },
                    "{crumb.title}"
                }
            }
        }
    }
}

/// Card for a shared folder (gallery)
#[component]
fn SharedFolderCard(
    folder_id: String,
    title: String,
    item_count: usize,
    sharer: String,
    on_open: EventHandler<(String, String)>,
) -> Element {
    let sharer_name = member_name(&sharer);
    let color_class = member_color_class(&sharer);
    let fid = folder_id.clone();
    let ftitle = title.clone();

    rsx! {
        div {
            class: "shared-artifact-card folder-card",
            onclick: move |_| on_open.call((fid.clone(), ftitle.clone())),
            div { class: "artifact-card-icon folder-icon", "📁" }
            div { class: "artifact-card-info",
                div { class: "artifact-card-name", "{title}" }
                div { class: "artifact-card-meta",
                    span { class: "artifact-card-size", "{item_count} items" }
                    span { class: "artifact-card-sharer {color_class}", "by {sharer_name}" }
                }
            }
        }
    }
}

/// Card for items inside a folder
#[component]
fn GalleryItemCard(item: crate::state::GalleryStateItem) -> Element {
    let mut preview_ctx = use_context::<PreviewContext>();
    let icon = item.icon();
    let is_image = item.is_image();
    let is_text = item.is_text();
    let image_url = if is_image {
        item.asset_path
            .as_ref()
            .and_then(|p| load_image_as_data_url(p))
            .or_else(|| {
                item.thumbnail_data
                    .as_ref()
                    .map(|d| format!("data:{};base64,{}", item.mime_type, d))
            })
    } else {
        None
    };

    let size = if item.size < 1024 {
        format!("{} B", item.size)
    } else if item.size < 1024 * 1024 {
        format!("{:.1} KB", item.size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", item.size as f64 / (1024.0 * 1024.0))
    };

    // Click handler for text files
    let card_class = if is_text {
        "shared-artifact-card gallery-item-card clickable-text-item"
    } else {
        "shared-artifact-card gallery-item-card"
    };

    rsx! {
        div {
            class: card_class,
            onclick: {
                let item = item.clone();
                move |_| {
                    if item.is_text() {
                        if let Some(content) = item.asset_path.as_ref()
                            .and_then(|p| load_text_file_content(p))
                            .or_else(|| item.text_preview.clone())
                        {
                            preview_ctx.file.set(Some(PreviewFile {
                                name: item.name.clone(),
                                raw_content: content.clone(),
                                content,
                                mime_type: item.mime_type.clone(),
                                ..Default::default()
                            }));
                            preview_ctx.view_mode.set(PreviewViewMode::Rendered);
                            preview_ctx.is_open.set(true);
                        }
                    }
                }
            },
            if let Some(ref url) = image_url {
                div { class: "artifact-card-thumbnail",
                    img { src: "{url}", alt: "{item.name}" }
                }
            } else {
                div { class: "artifact-card-icon", "{icon}" }
            }
            div { class: "artifact-card-info",
                div { class: "artifact-card-name", "{item.name}" }
                div { class: "artifact-card-meta",
                    span { class: "artifact-card-size", "{size}" }
                }
            }
        }
    }
}

/// Gallery panel showing all shared artifacts in the overview dashboard
#[component]
fn SharedArtifactGalleryPanel(state: Signal<AppState>) -> Element {
    let mut state_write = state;
    let state_read = state.read();
    let is_root = state_read.artifacts.is_at_root();
    let current_folder = state_read.artifacts.navigation.current_folder.clone();

    // Get folders from chat galleries (at root)
    let folders: Vec<_> = if is_root {
        state_read
            .chat
            .global_messages
            .iter()
            .filter_map(|msg| {
                if let crate::state::ChatMessageType::Gallery {
                    folder_id,
                    title,
                    items,
                } = &msg.message_type
                {
                    Some((
                        folder_id.clone(),
                        title.clone().unwrap_or_else(|| "Gallery".into()),
                        items.len(),
                        msg.member.clone(),
                    ))
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![]
    };

    // Get folder contents if inside a folder
    let folder_items: Vec<_> = if let Some(ref fid) = current_folder {
        state_read
            .chat
            .global_messages
            .iter()
            .find_map(|msg| {
                if let crate::state::ChatMessageType::Gallery { folder_id, items, .. } =
                    &msg.message_type
                {
                    if folder_id == fid {
                        Some(items.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    let artifacts = if is_root {
        state_read.artifacts.all_artifacts()
    } else {
        vec![]
    };
    let shared_count = state_read.artifacts.total_shared;
    let recalled_count = state_read.artifacts.total_recalled;
    let folder_count = folders.len();

    rsx! {
        div { class: "shared-artifact-gallery-panel",
            div { class: "artifact-gallery-header",
                ArtifactBreadcrumb { state }
                div { class: "artifact-gallery-stats",
                    if is_root {
                        span { class: "artifact-stat shared", "{shared_count} files" }
                        if folder_count > 0 {
                            span { class: "artifact-stat folders", "{folder_count} folders" }
                        }
                        if recalled_count > 0 {
                            span { class: "artifact-stat recalled", "{recalled_count} recalled" }
                        }
                    }
                }
            }
            div { class: "shared-artifact-grid",
                if is_root {
                    for (fid, title, count, sharer) in folders.iter() {
                        SharedFolderCard {
                            folder_id: fid.clone(),
                            title: title.clone(),
                            item_count: *count,
                            sharer: sharer.clone(),
                            on_open: move |(id, t): (String, String)| {
                                state_write.write().artifacts.open_folder(id, t);
                            }
                        }
                    }
                    for artifact in artifacts.iter().take(12 - folders.len().min(6)) {
                        SharedArtifactCard { artifact: (*artifact).clone() }
                    }
                } else {
                    for item in folder_items.iter() {
                        GalleryItemCard { item: item.clone() }
                    }
                }
                if is_root && artifacts.is_empty() && folders.is_empty() {
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

/// Load a text file's full content from an asset path
pub(crate) fn load_text_file_content(path: &str) -> Option<String> {
    let full_path = if path.starts_with('/') {
        std::path::PathBuf::from(path)
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    std::fs::read_to_string(&full_path).ok()
}

/// Render markdown to HTML
pub(crate) fn render_markdown_to_html(markdown: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// Check if file is markdown
fn is_markdown_file(name: &str, mime_type: &str) -> bool {
    name.ends_with(".md")
        || name.ends_with(".markdown")
        || mime_type == "text/markdown"
        || mime_type == "application/markdown"
}

/// File preview overlay — handles markdown (rendered/raw) and images.
#[component]
pub fn MarkdownPreviewOverlay(
    is_open: Signal<bool>,
    file: Signal<Option<PreviewFile>>,
    view_mode: Signal<PreviewViewMode>,
) -> Element {
    if !is_open() {
        return rsx! {};
    }
    let Some(file_data) = file() else {
        return rsx! {};
    };

    let is_image = file_data.mime_type.starts_with("image/");
    let is_md = !is_image && is_markdown_file(&file_data.name, &file_data.mime_type);
    let mode = view_mode();

    let rendered_html = if is_md && mode == PreviewViewMode::Rendered {
        Some(render_markdown_to_html(&file_data.content))
    } else {
        None
    };

    rsx! {
        div {
            class: "markdown-preview-overlay",
            onclick: move |_| is_open.set(false),
            div {
                class: "markdown-preview-dialog",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "markdown-preview-header",
                    span { class: "markdown-preview-filename", "{file_data.name}" }
                    div { class: "markdown-preview-controls",
                        if is_md {
                            button {
                                class: "markdown-preview-toggle",
                                onclick: move |_| {
                                    view_mode.set(if mode == PreviewViewMode::Rendered {
                                        PreviewViewMode::Raw
                                    } else {
                                        PreviewViewMode::Rendered
                                    });
                                },
                                if mode == PreviewViewMode::Rendered { "View Raw" } else { "View Rendered" }
                            }
                        }
                        button {
                            class: "markdown-preview-close",
                            onclick: move |_| is_open.set(false),
                            "×"
                        }
                    }
                }

                // Content
                div { class: "markdown-preview-content",
                    if is_image {
                        if let Some(ref url) = file_data.data_url {
                            div { class: "image-preview",
                                img {
                                    class: "image-preview-img",
                                    src: "{url}",
                                    alt: "{file_data.name}",
                                }
                            }
                        }
                    } else if let Some(ref html) = rendered_html {
                        div { class: "markdown-rendered", dangerous_inner_html: "{html}" }
                    } else {
                        pre { class: "markdown-raw", "{file_data.raw_content}" }
                    }
                }
            }
        }
    }
}

/// Render markdown narrative with artifact image references replaced by data URLs.
///
/// Transforms `![caption](artifact:HASH)` syntax to `![caption](data:...)` for display.
pub(crate) fn render_narrative_with_images(
    narrative: &str,
    artifacts: &[crate::state::ProofArtifactStateItem],
) -> String {
    use pulldown_cmark::{html, Event, Options, Parser, Tag};
    use std::collections::HashMap;

    // Build lookup map: artifact_hash -> data_url
    let artifact_map: HashMap<&str, Option<&str>> = artifacts
        .iter()
        .map(|a| (a.artifact_hash.as_str(), a.data_url.as_deref()))
        .collect();

    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(narrative, options);

    // Transform image URLs with artifact: prefix
    let transformed = parser.map(|event| {
        match event {
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
                // Check if URL starts with "artifact:"
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

/// Proof narrative overlay showing full markdown with embedded images.
#[component]
pub fn ProofNarrativeOverlay(
    is_open: Signal<bool>,
    data: Signal<Option<ProofNarrativeData>>,
    view_mode: Signal<PreviewViewMode>,
) -> Element {
    if !is_open() {
        return rsx! {};
    }
    let Some(narrative_data) = data() else {
        return rsx! {};
    };

    let member_display = member_name(&narrative_data.member);
    let color_class = member_color_class(&narrative_data.member);
    let mode = view_mode();

    // Render narrative with embedded artifact images (only in rendered mode)
    let rendered_html = if mode == PreviewViewMode::Rendered {
        Some(render_narrative_with_images(&narrative_data.narrative, &narrative_data.artifacts))
    } else {
        None
    };

    // Get image artifacts for gallery footer
    let image_artifacts: Vec<_> = narrative_data.artifacts.iter()
        .filter(|a| a.is_image())
        .collect();

    rsx! {
        div {
            class: "proof-narrative-overlay",
            onclick: move |_| is_open.set(false),
            div {
                class: "proof-narrative-dialog",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "proof-narrative-header",
                    div { class: "proof-narrative-header-left",
                        div { class: "proof-narrative-title-row",
                            span { class: "proof-narrative-icon", "📋" }
                            span { class: "proof-narrative-quest-title", "{narrative_data.quest_title}" }
                        }
                        div { class: "proof-narrative-meta",
                            span { class: "proof-narrative-by", "by " }
                            span { class: "proof-narrative-author {color_class}", "{member_display}" }
                        }
                    }
                    div { class: "proof-narrative-controls",
                        button {
                            class: "proof-narrative-toggle",
                            onclick: move |_| {
                                view_mode.set(if mode == PreviewViewMode::Rendered {
                                    PreviewViewMode::Raw
                                } else {
                                    PreviewViewMode::Rendered
                                });
                            },
                            if mode == PreviewViewMode::Rendered { "View Raw" } else { "View Rendered" }
                        }
                        button {
                            class: "proof-narrative-close",
                            onclick: move |_| is_open.set(false),
                            "×"
                        }
                    }
                }

                // Main content: rendered or raw markdown
                div { class: "proof-narrative-content",
                    if let Some(ref html) = rendered_html {
                        div { class: "proof-narrative-rendered", dangerous_inner_html: "{html}" }
                    } else {
                        pre { class: "proof-narrative-raw", "{narrative_data.narrative}" }
                    }
                }

                // Footer: artifact gallery
                if !image_artifacts.is_empty() {
                    div { class: "proof-narrative-gallery",
                        div { class: "proof-narrative-gallery-label", "Attached Evidence ({image_artifacts.len()})" }
                        div { class: "proof-narrative-gallery-grid",
                            for artifact in image_artifacts {
                                div {
                                    class: "proof-narrative-gallery-item",
                                    title: "{artifact.name}",
                                    if let Some(ref url) = artifact.data_url {
                                        img {
                                            class: "proof-narrative-gallery-thumb",
                                            src: "{url}",
                                            alt: artifact.caption.as_deref().unwrap_or(&artifact.name),
                                        }
                                    } else {
                                        div { class: "proof-narrative-gallery-placeholder", "🖼️" }
                                    }
                                    if let Some(ref caption) = artifact.caption {
                                        span { class: "proof-narrative-gallery-caption", "{caption}" }
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
        crate::state::ChatMessageType::ProofFolderSubmitted {
            artifact_count,
            narrative_preview,
            quest_title,
            narrative,
            artifacts,
            ..
        } => {
            let mut proof_ctx = use_context::<ProofNarrativeContext>();

            // Find first image artifact for thumbnail
            let first_image = artifacts.iter()
                .find(|a| a.is_image())
                .and_then(|a| a.data_url.clone());

            let preview = if narrative_preview.is_empty() {
                format!("{} files", artifact_count)
            } else {
                // Truncate preview if too long
                if narrative_preview.len() > 80 {
                    format!("{}...", &narrative_preview[..80])
                } else {
                    narrative_preview.clone()
                }
            };

            let title_display = if quest_title.is_empty() {
                "Proof Submission".to_string()
            } else {
                quest_title.clone()
            };

            // Check if we have narrative content to show
            let has_narrative = !narrative.is_empty();

            // Data for overlay
            let overlay_data = ProofNarrativeData {
                quest_title: title_display.clone(),
                member: message.member.clone(),
                narrative: narrative.clone(),
                artifacts: artifacts.clone(),
            };

            rsx! {
                div {
                    class: if has_narrative { "chat-message proof-folder-message clickable" } else { "chat-message proof-folder-message" },
                    onclick: {
                        let overlay_data = overlay_data.clone();
                        move |_| {
                            if has_narrative {
                                proof_ctx.data.set(Some(overlay_data.clone()));
                                proof_ctx.is_open.set(true);
                            }
                        }
                    },
                    // Thumbnail if available
                    if let Some(ref thumb_url) = first_image {
                        div { class: "proof-folder-thumb",
                            img {
                                src: "{thumb_url}",
                                alt: "Proof thumbnail",
                            }
                        }
                    }
                    div { class: "proof-folder-content",
                        div { class: "proof-folder-header",
                            span { class: "chat-tick", "[{message.tick}]" }
                            span { class: "chat-icon", "📋" }
                            span { class: "chat-sender {color_class}", "{name}" }
                        }
                        div { class: "proof-folder-title", "{title_display}" }
                        div { class: "proof-folder-preview", "{preview}" }
                        div { class: "proof-folder-meta",
                            span { class: "proof-folder-count", "{artifact_count} attachments" }
                            if has_narrative {
                                span { class: "proof-folder-cta", "Click to view" }
                            }
                        }
                    }
                }
            }
        }
        crate::state::ChatMessageType::Image {
            mime_type,
            inline_data,
            filename,
            alt_text,
            asset_path,
            ..
        } => {
            // Determine image source: prefer asset_path for local testing,
            // then inline_data, then placeholder
            let image_url = asset_path.as_ref()
                .and_then(|path| load_image_as_data_url(path))
                .or_else(|| inline_data.as_ref().map(|data| {
                    format!("data:{};base64,{}", mime_type, data)
                }));

            let display_name = filename.as_deref()
                .or(alt_text.as_deref())
                .unwrap_or("Image");

            rsx! {
                div { class: "chat-message image-message",
                    div { class: "chat-message-header",
                        span { class: "chat-tick", "[{message.tick}]" }
                        span { class: "chat-sender {color_class}", "{name}" }
                    }
                    div { class: "chat-image-container",
                        if let Some(ref url) = image_url {
                            img {
                                class: "chat-inline-image",
                                src: "{url}",
                                alt: "{display_name}",
                                loading: "lazy",
                            }
                        } else {
                            div { class: "chat-image-placeholder",
                                span { class: "placeholder-icon", "🖼️" }
                                span { class: "placeholder-text", "{display_name}" }
                            }
                        }
                    }
                    if let Some(ref caption) = alt_text {
                        div { class: "chat-image-caption", "{caption}" }
                    }
                }
            }
        }
        crate::state::ChatMessageType::Gallery {
            title,
            items,
            ..
        } => {
            let gallery_title = title.as_deref().unwrap_or("Gallery");
            let item_count = items.len();

            rsx! {
                div { class: "chat-message gallery-message",
                    div { class: "chat-message-header",
                        span { class: "chat-tick", "[{message.tick}]" }
                        span { class: "chat-icon", "🖼️" }
                        span { class: "chat-sender {color_class}", "{name}" }
                        span { class: "gallery-title", "{gallery_title}" }
                        span { class: "gallery-count", "({item_count} items)" }
                    }
                    div { class: "chat-gallery-grid",
                        for item in items.iter().take(6) {
                            {
                                let mut preview_ctx = use_context::<PreviewContext>();
                                let item_url = item.asset_path.as_ref()
                                    .and_then(|path| load_image_as_data_url(path))
                                    .or_else(|| item.thumbnail_data.as_ref().map(|data| {
                                        format!("data:{};base64,{}", item.mime_type, data)
                                    }));

                                let is_text = item.is_text();
                                let icon = item.icon();
                                let name = item.name.clone();
                                let text_preview = item.text_preview.clone();

                                let item_class = if is_text {
                                    "gallery-item clickable-text-item"
                                } else {
                                    "gallery-item"
                                };

                                rsx! {
                                    div {
                                        class: item_class,
                                        title: "{name}",
                                        onclick: {
                                            let item = item.clone();
                                            move |_| {
                                                if item.is_text() {
                                                    if let Some(content) = item.asset_path.as_ref()
                                                        .and_then(|p| load_text_file_content(p))
                                                        .or_else(|| item.text_preview.clone())
                                                    {
                                                        preview_ctx.file.set(Some(PreviewFile {
                                                            name: item.name.clone(),
                                                            raw_content: content.clone(),
                                                            content,
                                                            mime_type: item.mime_type.clone(),
                                                            ..Default::default()
                                                        }));
                                                        preview_ctx.view_mode.set(PreviewViewMode::Rendered);
                                                        preview_ctx.is_open.set(true);
                                                    }
                                                }
                                            }
                                        },
                                        if is_text {
                                            // Text/Markdown preview
                                            div { class: "gallery-text-preview",
                                                div { class: "gallery-text-header",
                                                    span { class: "gallery-text-icon", "{icon}" }
                                                    span { class: "gallery-text-name", "{name}" }
                                                }
                                                if let Some(ref preview) = text_preview {
                                                    div { class: "gallery-text-content", "{preview}" }
                                                }
                                            }
                                        } else if let Some(ref url) = item_url {
                                            img {
                                                class: "gallery-thumbnail",
                                                src: "{url}",
                                                alt: "{name}",
                                                loading: "lazy",
                                            }
                                        } else {
                                            div { class: "gallery-placeholder",
                                                span { class: "gallery-placeholder-icon", "{icon}" }
                                                span { class: "gallery-placeholder-name", "{name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if item_count > 6 {
                            div { class: "gallery-more",
                                "+{item_count - 6} more"
                            }
                        }
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
pub fn FloatingControlBar(state: Signal<AppState>) -> Element {
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

            // Timestep slider pill
            {
                let current_pos = playback::get_current_pos();
                let buffer_len = playback::get_buffer_len();
                if buffer_len > 0 {
                    rsx! {
                        div { class: "timestep-pill",
                            span { class: "timestep-label", "{current_pos}" }
                            input {
                                class: "timestep-slider",
                                r#type: "range",
                                min: "0",
                                max: "{buffer_len}",
                                step: "1",
                                value: "{current_pos}",
                                onchange: move |evt| {
                                    if let Ok(v) = evt.value().parse::<usize>() {
                                        playback::request_seek(v);
                                        playback::set_paused(true);
                                        state_write.write().playback.paused = true;
                                    }
                                },
                            }
                            span { class: "timestep-total", "{buffer_len}" }
                        }
                    }
                } else {
                    rsx! {}
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
    let color_class = member_color_class(&member);

    // Preview overlay state (for markdown files in galleries)
    let preview_open = use_signal(|| false);
    let preview_file = use_signal(|| None::<PreviewFile>);
    let preview_mode = use_signal(|| PreviewViewMode::Rendered);

    // Proof narrative overlay state
    let proof_narrative_open = use_signal(|| false);
    let proof_narrative_data = use_signal(|| None::<ProofNarrativeData>);
    let proof_narrative_mode = use_signal(|| PreviewViewMode::Rendered);

    // Provide preview context to children
    use_context_provider(|| PreviewContext {
        is_open: preview_open,
        file: preview_file,
        view_mode: preview_mode,
    });

    // Provide proof narrative context to children
    use_context_provider(|| ProofNarrativeContext {
        is_open: proof_narrative_open,
        data: proof_narrative_data,
    });

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
                            state,
                            member: member.clone(),
                            name: name.clone(),
                        }
                        TokensOfGratitudePanel { state, member: member.clone() }
                        MyNetworkView { state, member: member.clone() }
                    }
                    div { class: "pov-center-column",
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
fn ProfileHero(state: Signal<AppState>, member: String, name: String) -> Element {
    let initial = name.chars().next().unwrap_or('?');
    let color_class = member_color_class(&member);

    let state_read = state.read();
    let current_focus = state_read.attention.focus_for_member(&member).cloned();

    // Get current focus quest title
    let current_focus_title = current_focus.as_ref().and_then(|qid| {
        state_read.quests.quests.get(qid).map(|q| q.title.clone())
    });

    rsx! {
        div { class: "profile-hero",
            div { class: "profile-avatar-large {color_class}",
                "{initial}"
            }
            h2 { class: "profile-name", "{name}" }

            // Current Focus Section
            div { class: "profile-focus-section",
                div { class: "profile-focus-label", "Currently Focused" }
                if let Some(ref title) = current_focus_title {
                    div { class: "profile-focus-quest", "{title}" }
                } else {
                    div { class: "profile-focus-none", "Not focusing on any quest" }
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
pub fn MyChatPanel(state: Signal<AppState>, member: String) -> Element {
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
                    // In viewer mode, submission happens in the simulation backend.
                    // This UI just closes the editor. Actual submission state updates
                    // come through StreamEvent::ProofFolderSubmitted.
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
pub fn MyActivity(state: Signal<AppState>, member: String) -> Element {
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
// TOKENS OF GRATITUDE
// ============================================================================

/// Panel displaying a member's discrete Tokens of Gratitude
#[component]
pub fn TokensOfGratitudePanel(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();
    let tokens = state_read.tokens.tokens_for_member(&member);
    let token_count = tokens.len();
    let total_value = state_read.tokens.total_value_for_member(&member);
    let available_value = state_read.tokens.available_value_for_member(&member);
    let pledged_count = state_read.tokens.pledged_count_for_member(&member);
    let total_formatted = format_duration_millis(total_value);
    let available_formatted = format_duration_millis(available_value);

    // Get max value for relative bar sizing
    let max_value = tokens.first().map(|t| t.value_millis).unwrap_or(1).max(1);

    rsx! {
        div { class: "tokens-panel",
            div { class: "tokens-header",
                div { class: "tokens-title",
                    span { class: "tokens-icon", "🏆" }
                    span { "Tokens of Gratitude" }
                    span { class: "panel-count", "{token_count}" }
                }
            }

            // Summary with total/available value
            if token_count > 0 {
                div { class: "tokens-summary",
                    span { class: "tokens-total-label", "Total:" }
                    span { class: "tokens-total-value", "{total_formatted}" }
                    if pledged_count > 0 {
                        span { class: "tokens-total-label", " | Available:" }
                        span { class: "tokens-total-value", "{available_formatted}" }
                        span { class: "tokens-total-label", " ({pledged_count} pledged)" }
                    }
                }
            }

            // Token list
            div { class: "tokens-list",
                for token in tokens.iter() {
                    TokenCard {
                        token: (*token).clone(),
                        max_value
                    }
                }
                if token_count == 0 {
                    div { class: "empty-state tokens-empty",
                        div { class: "empty-icon", "🏆" }
                        div { class: "empty-text", "No tokens yet" }
                        div { class: "empty-hint", "Submit proofs of service to earn tokens" }
                    }
                }
            }
        }
    }
}

/// Individual discrete Token of Gratitude card
#[component]
fn TokenCard(token: TokenOfGratitude, max_value: u64) -> Element {
    let value_formatted = token.formatted_value();
    let bar_width = if max_value > 0 {
        (token.value_millis as f64 / max_value as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let blesser_name = member_name(&token.blesser);
    let source_quest = short_id(&token.source_quest_id);

    // Pledge status
    let pledge_status = if let Some(ref quest_id) = token.pledged_to {
        format!("Pledged to {}", short_id(quest_id))
    } else {
        "Available".to_string()
    };

    let pledge_class = if token.is_pledged() {
        "token-status-pledged"
    } else {
        "token-status-available"
    };

    rsx! {
        div { class: "token-card",
            div { class: "token-header",
                span { class: "token-icon", "🏆" }
                span { class: "token-title", "From {blesser_name}" }
                span { class: "token-badge {pledge_class}", "{pledge_status}" }
            }

            // Value bar
            div { class: "token-value-section",
                div { class: "token-bar-container",
                    div {
                        class: "token-bar-fill",
                        style: "width: {bar_width}%"
                    }
                }
                span { class: "token-value-text", "{value_formatted}" }
            }

            // Meta info
            div { class: "token-meta",
                span { class: "token-meta-item",
                    span { class: "meta-icon", "✨" }
                    "Blessed by {blesser_name}"
                }
                span { class: "token-meta-item",
                    span { class: "meta-icon", "🎯" }
                    "Source: {source_quest}"
                }
            }
        }
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

pub fn member_color_class(member: &str) -> &'static str {
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

pub fn member_color_var(member: &str) -> &'static str {
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
