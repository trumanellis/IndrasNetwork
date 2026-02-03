//! Home realm screen - main view after genesis completes.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{GenesisState, NoteView, QuestView};

/// Helper to hex-encode a 16-byte ID.
fn hex_id(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Refresh quests and notes from the network into state.
async fn refresh_home_realm_data(
    network: &Arc<IndrasNetwork>,
    state: &mut Signal<GenesisState>,
) {
    if let Ok(home) = network.home_realm().await {
        // Load quests
        if let Ok(doc) = home.quests().await {
            let data = doc.read().await;
            let quests: Vec<QuestView> = data.quests.iter().map(|q| {
                QuestView {
                    id: hex_id(&q.id),
                    title: q.title.clone(),
                    description: q.description.clone(),
                    is_complete: q.completed_at_millis.is_some(),
                }
            }).collect();
            drop(data);
            state.write().quests = quests;
        }

        // Load notes
        if let Ok(doc) = home.notes().await {
            let data = doc.read().await;
            let notes: Vec<NoteView> = data.notes.iter().map(|n| {
                NoteView {
                    id: hex_id(&n.id),
                    title: n.title.clone(),
                    content_preview: n.content.chars().take(100).collect(),
                }
            }).collect();
            drop(data);
            state.write().notes = notes;
        }
    }
}

#[component]
pub fn HomeRealmScreen(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    let s = state.read();
    let display_name = if s.display_name.is_empty() {
        "Anonymous".to_string()
    } else {
        s.display_name.clone()
    };
    let member_id = s.member_id_short.clone().unwrap_or_default();
    let quest_count = s.quests.len();
    let note_count = s.notes.len();
    let quests = s.quests.clone();
    let notes = s.notes.clone();
    let note_form_open = s.note_form_open;
    let nudge_dismissed = s.nudge_dismissed;
    let has_content = !notes.is_empty() || quest_count > 1;
    drop(s);

    // Check if story keystore is initialized
    let story_initialized = {
        let data_dir = super::app::default_data_dir();
        indras_node::StoryKeystore::new(&data_dir).is_initialized()
    };
    let show_nudge = !story_initialized && has_content && !nudge_dismissed;

    rsx! {
        div {
            class: "genesis-screen home-screen",

            // Header
            header {
                class: "home-header",

                h1 {
                    class: "home-title",
                    "My Home Realm"
                }

                div {
                    class: "home-header-info",
                    span {
                        class: "home-display-name",
                        "{display_name}"
                    }
                    span {
                        class: "home-member-id",
                        "{member_id}"
                    }
                }
            }

            // Nudge banner
            if show_nudge {
                div {
                    class: "nudge-banner",
                    p {
                        "You have {note_count} note(s) with no story protection."
                    }
                    div {
                        class: "nudge-actions",
                        button {
                            class: "genesis-btn-primary",
                            onclick: move |_| {
                                state.write().pass_story_active = true;
                            },
                            "Protect your identity"
                        }
                        button {
                            class: "genesis-btn-secondary nudge-dismiss",
                            onclick: move |_| {
                                state.write().nudge_dismissed = true;
                            },
                            "Later"
                        }
                    }
                }
            }

            // Main content
            div {
                class: "home-layout",

                // Main panel
                div {
                    class: "home-main",

                    // Quests panel
                    section {
                        class: "home-panel home-quests",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Quests" }
                            span { class: "panel-count", "{quest_count}" }
                        }

                        if quests.is_empty() {
                            div {
                                class: "panel-empty",
                                "No quests yet."
                            }
                        } else {
                            div {
                                class: "quests-list",
                                for quest in quests.iter() {
                                    {render_quest_item(quest, state, network)}
                                }
                            }
                        }
                    }

                    // Notes panel
                    section {
                        class: "home-panel home-notes",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Notes" }
                            span { class: "panel-count", "{note_count}" }
                            button {
                                class: "genesis-btn-secondary",
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.note_form_open = !s.note_form_open;
                                    if !s.note_form_open {
                                        s.note_draft_title.clear();
                                        s.note_draft_content.clear();
                                    }
                                },
                                if note_form_open { "Cancel" } else { "+ New Note" }
                            }
                        }

                        // Note creation form
                        if note_form_open {
                            div {
                                class: "note-form",
                                input {
                                    class: "genesis-input note-form-input",
                                    r#type: "text",
                                    placeholder: "Note title...",
                                    value: "{state.read().note_draft_title}",
                                    oninput: move |evt| {
                                        state.write().note_draft_title = evt.value();
                                    },
                                }
                                textarea {
                                    class: "genesis-input note-form-textarea",
                                    placeholder: "Write your note...",
                                    rows: "4",
                                    value: "{state.read().note_draft_content}",
                                    oninput: move |evt| {
                                        state.write().note_draft_content = evt.value();
                                    },
                                }
                                button {
                                    class: "genesis-btn-primary",
                                    disabled: state.read().note_draft_title.trim().is_empty(),
                                    onclick: move |_| {
                                        let title = state.read().note_draft_title.clone();
                                        let content = state.read().note_draft_content.clone();
                                        spawn(async move {
                                            let net = network.read();
                                            if let Some(ref net) = *net {
                                                if let Ok(home) = net.home_realm().await {
                                                    if let Ok(_note_id) = home.create_note(
                                                        title,
                                                        content,
                                                        vec![],
                                                    ).await {
                                                        // Clear form and close
                                                        {
                                                            let mut s = state.write();
                                                            s.note_draft_title.clear();
                                                            s.note_draft_content.clear();
                                                            s.note_form_open = false;
                                                        }
                                                        // Refresh data
                                                        refresh_home_realm_data(net, &mut state).await;
                                                    } else {
                                                        tracing::error!("Failed to create note");
                                                    }
                                                }
                                            }
                                        });
                                    },
                                    "Create Note"
                                }
                            }
                        }

                        if notes.is_empty() && !note_form_open {
                            div {
                                class: "panel-empty",
                                p { "No notes yet." }
                                p { class: "panel-empty-hint", "Create your first note to get started." }
                            }
                        } else {
                            div {
                                class: "notes-list",
                                for note in notes.iter() {
                                    div {
                                        key: "{note.id}",
                                        class: "note-card",
                                        h3 { class: "note-title", "{note.title}" }
                                        p { class: "note-preview", "{note.content_preview}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Sidebar
                aside {
                    class: "home-sidebar",

                    section {
                        class: "home-panel sidebar-info",

                        h2 { class: "panel-title", "Identity" }

                        div {
                            class: "sidebar-field",
                            span { class: "sidebar-label", "Member ID" }
                            span { class: "sidebar-value sidebar-value-mono", "{member_id}" }
                        }

                        div {
                            class: "sidebar-field",
                            span { class: "sidebar-label", "Display Name" }
                            span { class: "sidebar-value", "{display_name}" }
                        }

                        div {
                            class: "sidebar-field",
                            span { class: "sidebar-label", "Quests" }
                            span { class: "sidebar-value", "{quest_count}" }
                        }

                        div {
                            class: "sidebar-field",
                            span { class: "sidebar-label", "Notes" }
                            span { class: "sidebar-value", "{note_count}" }
                        }

                        div {
                            class: "sidebar-field",
                            span { class: "sidebar-label", "Protection" }
                            span {
                                class: if story_initialized { "sidebar-value sidebar-protected" } else { "sidebar-value sidebar-unprotected" },
                                if story_initialized { "Story-protected" } else { "Unprotected" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a single quest item with clickable completion.
fn render_quest_item(
    quest: &QuestView,
    state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    let quest_id = quest.id.clone();
    let is_complete = quest.is_complete;
    let title = quest.title.clone();
    let description = quest.description.clone();

    rsx! {
        div {
            key: "{quest_id}",
            class: if is_complete { "quest-item quest-complete" } else { "quest-item" },

            div {
                class: if is_complete { "quest-checkbox" } else { "quest-checkbox quest-checkbox-clickable" },
                onclick: move |_| {
                    if !is_complete {
                        let qid = quest_id.clone();
                        let mut state = state;
                        let network = network;
                        spawn(async move {
                            let net = network.read();
                            if let Some(ref net) = *net {
                                if let Ok(home) = net.home_realm().await {
                                    // Parse hex ID back to [u8; 16]
                                    if let Some(id_bytes) = hex_to_quest_id(&qid) {
                                        if let Ok(()) = home.complete_quest(id_bytes).await {
                                            refresh_home_realm_data(net, &mut state).await;
                                        } else {
                                            tracing::error!("Failed to complete quest");
                                        }
                                    }
                                }
                            }
                        });
                    }
                },
                if is_complete {
                    span { class: "quest-check", "\u{2713}" }
                }
            }

            div {
                class: "quest-content",
                h3 { class: "quest-title", "{title}" }
                div {
                    class: "quest-description",
                    {render_quest_description(&description, state)}
                }
            }
        }
    }
}

/// Parse a hex string back to a QuestId ([u8; 16]).
fn hex_to_quest_id(hex: &str) -> Option<[u8; 16]> {
    if hex.len() != 32 {
        return None;
    }
    let mut id = [0u8; 16];
    for i in 0..16 {
        id[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(id)
}

/// Render quest description with checklist support.
/// Lines starting with "- [ ]" or "- [x]" are rendered as checklist items.
/// The "Write your pass story" item triggers the pass story flow.
fn render_quest_description(
    description: &str,
    state: Signal<GenesisState>,
) -> Element {
    let lines: Vec<String> = description.lines().map(|l| l.to_string()).collect();

    rsx! {
        for line in lines.iter() {
            {render_description_line(line, state)}
        }
    }
}

fn render_description_line(line: &str, mut state: Signal<GenesisState>) -> Element {
    let trimmed = line.trim();

    if trimmed.starts_with("- [ ] ") {
        let text = trimmed.strip_prefix("- [ ] ").unwrap_or(trimmed);
        let is_story_item = text.to_lowercase().contains("pass story")
            || text.to_lowercase().contains("story");
        let text = text.to_string();

        if is_story_item {
            rsx! {
                div {
                    class: "checklist-item checklist-clickable",
                    onclick: move |_| {
                        state.write().pass_story_active = true;
                    },
                    span { class: "checklist-box", "\u{2610}" }
                    span { class: "checklist-text checklist-link", "{text}" }
                }
            }
        } else {
            rsx! {
                div {
                    class: "checklist-item",
                    span { class: "checklist-box", "\u{2610}" }
                    span { class: "checklist-text", "{text}" }
                }
            }
        }
    } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
        let text = if trimmed.starts_with("- [x] ") {
            trimmed.strip_prefix("- [x] ").unwrap_or(trimmed)
        } else {
            trimmed.strip_prefix("- [X] ").unwrap_or(trimmed)
        };
        let text = text.to_string();

        rsx! {
            div {
                class: "checklist-item checklist-done",
                span { class: "checklist-box", "\u{2611}" }
                span { class: "checklist-text", "{text}" }
            }
        }
    } else if trimmed.is_empty() {
        rsx! { br {} }
    } else {
        let text = trimmed.to_string();
        rsx! {
            p { class: "quest-desc-line", "{text}" }
        }
    }
}
