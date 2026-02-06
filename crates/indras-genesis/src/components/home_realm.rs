//! Home realm screen - main view after genesis completes.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;
use indras_sync_engine::{HomeRealmQuests, HomeRealmNotes};

use indras_ui::{ArtifactDisplayInfo, ArtifactDisplayStatus, ArtifactGallery};

use crate::state::{ContactView, ContactSentiment, EventDirection, GenesisState, GenesisStep, NoteView, QuestAttentionView, QuestClaimView, QuestStatus, QuestView};

/// Helper to hex-encode a 16-byte ID.
fn hex_id(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Refresh quests and notes from the network into state.
async fn refresh_home_realm_data(
    network: &Arc<IndrasNetwork>,
    state: &mut Signal<GenesisState>,
) {
    let my_id = network.id();
    let _my_id_short: String = my_id.iter().take(8).map(|b| format!("{:02x}", b)).collect();

    if let Ok(home) = network.home_realm().await {
        // Load quests with full claim information
        if let Ok(doc) = home.quests().await {
            let data = doc.read().await;
            let quests: Vec<QuestView> = data.quests.iter().map(|q| {
                let creator_id_short: String = q.creator.iter().take(8).map(|b| format!("{:02x}", b)).collect();
                let is_creator = q.creator == my_id;
                let is_complete = q.completed_at_millis.is_some();

                // Build claim views
                let claims: Vec<QuestClaimView> = q.claims.iter().map(|c| {
                    let claimant_id_short: String = c.claimant.iter().take(8).map(|b| format!("{:02x}", b)).collect();
                    QuestClaimView {
                        claimant_id_short,
                        claimant_name: None, // TODO: resolve from contacts
                        verified: c.verified,
                        has_proof: c.has_proof(),
                        submitted_at: chrono::DateTime::from_timestamp_millis(c.submitted_at_millis)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default(),
                    }
                }).collect();

                let pending_claim_count = q.pending_claims().len();
                let verified_claim_count = q.verified_claims().len();

                // Determine status
                let status = if is_complete {
                    QuestStatus::Completed
                } else if verified_claim_count > 0 {
                    QuestStatus::Verified
                } else if !q.claims.is_empty() {
                    QuestStatus::Claimed
                } else {
                    QuestStatus::Open
                };

                QuestView {
                    id: hex_id(&q.id),
                    title: q.title.clone(),
                    description: q.description.clone(),
                    is_complete,
                    status,
                    creator_id_short,
                    is_creator,
                    claims,
                    pending_claim_count,
                    verified_claim_count,
                    attention: QuestAttentionView::default(),
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

        // Load artifacts
        if let Ok(doc) = home.artifact_index().await {
            let data = doc.read().await;
            let artifacts: Vec<ArtifactDisplayInfo> = data.active_artifacts().map(|a| {
                ArtifactDisplayInfo {
                    id: a.id.iter().map(|b| format!("{:02x}", b)).collect(),
                    name: a.name.clone(),
                    size: a.size,
                    mime_type: a.mime_type.clone(),
                    status: ArtifactDisplayStatus::Active,
                    data_url: None,
                    grant_count: a.grants.len(),
                    owner_label: if a.grants.is_empty() {
                        Some("Private".to_string())
                    } else {
                        Some(format!("Shared with {}", a.grants.len()))
                    },
                }
            }).collect();
            drop(data);
            state.write().artifacts = artifacts;
        }
    }

    // Load contacts (use async read to avoid blocking in async context)
    if let Some(contacts_realm) = network.contacts_realm().await {
        if let Ok(doc) = contacts_realm.contacts().await {
            let data = doc.read().await;
            let contacts: Vec<ContactView> = data.contacts.iter().map(|(mid, entry)| {
                ContactView {
                    member_id: *mid,
                    member_id_short: mid.iter().take(8).map(|b| format!("{:02x}", b)).collect(),
                    display_name: entry.display_name.clone(),
                    status: "confirmed".to_string(),
                    sentiment: ContactSentiment::Neutral, // TODO: load from sentiment document
                }
            }).collect();
            drop(data);
            state.write().contacts = contacts;
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
    let contacts = s.contacts.clone();
    let contact_count = s.contacts.len();
    let artifacts = s.artifacts.clone();
    let artifact_count = s.artifacts.len();
    let tokens = s.tokens.clone();
    let token_count = s.tokens.len();
    let note_form_open = s.note_form_open;
    let nudge_dismissed = s.nudge_dismissed;
    let event_log = s.event_log.clone();
    let has_content = !notes.is_empty() || quest_count > 1;
    drop(s);

    // Periodically refresh contacts and save world view.
    // The inbox listener creates DM realms in the background; this poll
    // ensures the UI reflects new connections within a few seconds.
    // World view is saved every 30 seconds and on connection changes.
    use_effect(move || {
        spawn(async move {
            let mut tick = 0u32;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let net = match network.read().as_ref() {
                    Some(n) => n.clone(),
                    None => continue,
                };
                tick += 1;

                // Count DM realms as a proxy for connections
                let dm_count = net.realms().iter()
                    .filter(|realm_id| {
                        net.get_realm_by_id(realm_id)
                            .map(|r| r.name() == Some("DM"))
                            .unwrap_or(false)
                    })
                    .count();
                let current_count = state.read().contacts.len();

                // Reload contacts and save world view if connection count changed
                if dm_count != current_count {
                    if let Some(contacts_realm) = net.contacts_realm().await {
                        if let Ok(doc) = contacts_realm.contacts().await {
                            let data = doc.read().await;
                            let contacts: Vec<ContactView> = data.contacts.iter().map(|(mid, entry)| {
                                ContactView {
                                    member_id: *mid,
                                    member_id_short: mid.iter().take(8).map(|b| format!("{:02x}", b)).collect(),
                                    display_name: entry.display_name.clone(),
                                    status: "confirmed".to_string(),
                                    sentiment: ContactSentiment::Neutral,
                                }
                            }).collect();
                            drop(data);
                            state.write().contacts = contacts;
                        }
                    }
                    // Save world view on connection change
                    let _ = net.save_world_view().await;
                    tick = 0; // Reset periodic timer
                }

                // Periodic world view save every 30 seconds (6 ticks × 5s)
                if tick >= 6 {
                    let _ = net.save_world_view().await;
                    tick = 0;
                }
            }
        });
    });

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

                    // Artifacts panel
                    section {
                        class: "home-panel home-artifacts",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Artifacts" }
                            span { class: "panel-count", "{artifact_count}" }
                        }

                        if artifacts.is_empty() {
                            div {
                                class: "panel-empty",
                                p { "No artifacts yet." }
                                p { class: "panel-empty-hint", "Share files in a realm to see them here." }
                            }
                        } else {
                            ArtifactGallery { artifacts: artifacts.clone() }
                        }
                    }

                    // Tokens of Gratitude panel
                    section {
                        class: "home-panel home-tokens",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Tokens of Gratitude" }
                            span { class: "panel-count", "{token_count}" }
                        }

                        if tokens.is_empty() {
                            div {
                                class: "panel-empty",
                                p { "No tokens yet." }
                                p { class: "panel-empty-hint", "Receive blessings to earn tokens of gratitude." }
                            }
                        } else {
                            div {
                                class: "tokens-list",
                                for token in tokens.iter() {
                                    div {
                                        key: "{token.id_short}",
                                        class: if token.is_pledged { "token-card token-pledged" } else { "token-card" },

                                        div {
                                            class: "token-icon",
                                            "\u{2728}" // Sparkles
                                        }

                                        div {
                                            class: "token-info",
                                            if let Some(ref quest) = token.source_quest_title {
                                                div { class: "token-source", "From: {quest}" }
                                            }
                                            if let Some(ref blesser) = token.blesser_name {
                                                div { class: "token-blesser", "By: {blesser}" }
                                            }
                                            div { class: "token-created", "{token.created_at}" }
                                        }

                                        if token.is_pledged {
                                            if let Some(ref pledged_to) = token.pledged_quest_title {
                                                span { class: "token-pledged-badge", "Pledged to: {pledged_to}" }
                                            } else {
                                                span { class: "token-pledged-badge", "Pledged" }
                                            }
                                        }
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

                    section {
                        class: "home-panel sidebar-connect",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Connections" }
                            span { class: "panel-count", "{contact_count}" }
                        }

                        if contacts.is_empty() {
                            div {
                                class: "contacts-empty",
                                "No connections yet. Share your invite link to get started."
                            }
                        } else {
                            div {
                                class: "contacts-list",
                                for contact in contacts.iter() {
                                    {
                                        let mid = contact.member_id;
                                        let contact_name = contact.display_name.clone();
                                        let is_pending = contact.status == "pending";
                                        let id_short = contact.member_id_short.clone();
                                        rsx! {
                                            div {
                                                key: "{id_short}",
                                                class: if is_pending { "contact-item contact-pending" } else { "contact-item contact-clickable" },
                                                onclick: move |_| {
                                                    if !is_pending {
                                                        let mut s = state.write();
                                                        s.peer_realm_messages.clear();
                                                        s.peer_realm_draft.clear();
                                                        s.peer_realm_last_seq = 0;
                                                        s.peer_realm_message_count = 0;
                                                        s.peer_realm_action_menu_open = false;
                                                        s.peer_realm_contact_name = contact_name.clone();
                                                        s.step = GenesisStep::PeerRealm(mid);
                                                    }
                                                },
                                                if let Some(ref name) = contact_name {
                                                    span { class: "contact-name", "{name}" }
                                                    span { class: "contact-id contact-id-secondary", "{id_short}" }
                                                } else {
                                                    span { class: "contact-id", "{id_short}" }
                                                }
                                                if is_pending {
                                                    span { class: "contact-status-badge", "(pending)" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        button {
                            class: "genesis-btn-primary",
                            onclick: move |_| {
                                state.write().contact_invite_open = true;
                            },
                            "Make Contact"
                        }
                    }
                }
            }

            // Event Log
            section {
                class: "event-log",

                div {
                    class: "event-log-header",
                    span { class: "event-log-title", "Network Log" }
                }

                div {
                    class: "event-log-list",

                    if event_log.is_empty() {
                        span { class: "event-log-msg", "No events yet." }
                    }

                    for entry in event_log.iter() {
                        div {
                            class: "event-log-entry",
                            span { class: "event-log-time", "{entry.timestamp}" }
                            span {
                                class: match entry.direction {
                                    EventDirection::Sent => "event-log-arrow event-log-arrow-sent",
                                    EventDirection::Received => "event-log-arrow event-log-arrow-received",
                                    EventDirection::System => "event-log-arrow event-log-arrow-system",
                                },
                                match entry.direction {
                                    EventDirection::Sent => "\u{2192}",
                                    EventDirection::Received => "\u{2190}",
                                    EventDirection::System => "\u{00b7}",
                                }
                            }
                            span { class: "event-log-msg", "{entry.message}" }
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

                div {
                    class: "quest-header",
                    h3 { class: "quest-title", "{title}" }
                    span { class: "quest-status-badge {status_class}", "{status_badge}" }
                }

                div {
                    class: "quest-description",
                    {render_quest_description(&description, state)}
                }

                // Claims section
                if !claims.is_empty() {
                    div {
                        class: "quest-claims",
                        div {
                            class: "quest-claims-header",
                            span { class: "quest-claims-title", "Claims ({pending_count} pending, {verified_count} verified)" }
                        }
                        for (idx, claim) in claims.iter().enumerate() {
                            {render_quest_claim(claim, idx, &quest_id, is_creator, state, network)}
                        }
                    }
                }

                // Action buttons
                if !is_complete {
                    div {
                        class: "quest-actions",

                        // Claim button (show claim form)
                        if !showing_claim_form {
                            button {
                                class: "genesis-btn-secondary quest-claim-btn",
                                onclick: move |_| {
                                    state.write().claiming_quest_id = Some(quest_id_for_claim.clone());
                                },
                                "Submit Claim"
                            }
                        }
                    }
                }

                // Claim form (inline)
                if showing_claim_form {
                    div {
                        class: "quest-claim-form",
                        textarea {
                            class: "genesis-input quest-claim-textarea",
                            placeholder: "Describe your proof of completion...",
                            rows: "3",
                            value: "{state.read().claim_proof_text}",
                            oninput: move |evt| {
                                state.write().claim_proof_text = evt.value();
                            },
                        }
                        div {
                            class: "quest-claim-form-actions",
                            button {
                                class: "genesis-btn-primary",
                                onclick: move |_| {
                                    let qid = quest_id.clone();
                                    let mut state = state;
                                    let network = network;
                                    spawn(async move {
                                        let net = network.read();
                                        if let Some(ref net) = *net {
                                            if let Ok(home) = net.home_realm().await {
                                                if let Some(id_bytes) = hex_to_quest_id(&qid) {
                                                    // Submit claim without artifact for now
                                                    if let Ok(_idx) = home.submit_quest_claim(id_bytes, None).await {
                                                        state.write().claiming_quest_id = None;
                                                        state.write().claim_proof_text.clear();
                                                        refresh_home_realm_data(net, &mut state).await;
                                                    } else {
                                                        tracing::error!("Failed to submit claim");
                                                    }
                                                }
                                            }
                                        }
                                    });
                                },
                                "Submit"
                            }
                            button {
                                class: "genesis-btn-secondary",
                                onclick: move |_| {
                                    state.write().claiming_quest_id = None;
                                    state.write().claim_proof_text.clear();
                                },
                                "Cancel"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a single quest claim with verify button.
fn render_quest_claim(
    claim: &QuestClaimView,
    claim_index: usize,
    quest_id: &str,
    is_creator: bool,
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    let claimant = claim.claimant_name.clone().unwrap_or_else(|| claim.claimant_id_short.clone());
    let verified = claim.verified;
    let has_proof = claim.has_proof;
    let submitted_at = claim.submitted_at.clone();
    let quest_id = quest_id.to_string();

    rsx! {
        div {
            class: if verified { "quest-claim quest-claim-verified" } else { "quest-claim" },

            span { class: "quest-claim-claimant", "{claimant}" }
            span { class: "quest-claim-time", "{submitted_at}" }

            if has_proof {
                span { class: "quest-claim-proof-badge", "\u{1f4ce}" }
            }

            if verified {
                span { class: "quest-claim-verified-badge", "\u{2713} Verified" }
            } else if is_creator {
                button {
                    class: "genesis-btn-secondary quest-verify-btn",
                    onclick: move |_| {
                        let qid = quest_id.clone();
                        let idx = claim_index;
                        let mut state = state;
                        let network = network;
                        spawn(async move {
                            let net = network.read();
                            if let Some(ref net) = *net {
                                if let Ok(home) = net.home_realm().await {
                                    if let Some(id_bytes) = hex_to_quest_id(&qid) {
                                        if let Ok(()) = home.verify_quest_claim(id_bytes, idx).await {
                                            refresh_home_realm_data(net, &mut state).await;
                                        } else {
                                            tracing::error!("Failed to verify claim");
                                        }
                                    }
                                }
                            }
                        });
                    },
                    "Verify"
                }
            }
        }
    }
}

/// Render a single contact item with sentiment indicator.
fn render_contact_item(
    contact: &ContactView,
    mut state: Signal<GenesisState>,
) -> Element {
    let mid = contact.member_id;
    let contact_name = contact.display_name.clone();
    let is_pending = contact.status == "pending";
    let id_short = contact.member_id_short.clone();
    let sentiment = contact.sentiment;
    let is_blocked = sentiment == ContactSentiment::Blocked;

    // Sentiment indicator
    let sentiment_icon = match sentiment {
        ContactSentiment::Recommend => "\u{2b50}", // Star
        ContactSentiment::Neutral => "",
        ContactSentiment::Blocked => "\u{1f6ab}", // No entry sign
    };

    let sentiment_class = match sentiment {
        ContactSentiment::Recommend => "contact-sentiment contact-sentiment-recommend",
        ContactSentiment::Neutral => "contact-sentiment contact-sentiment-neutral",
        ContactSentiment::Blocked => "contact-sentiment contact-sentiment-blocked",
    };

    rsx! {
        div {
            key: "{id_short}",
            class: if is_blocked {
                "contact-item contact-blocked"
            } else if is_pending {
                "contact-item contact-pending"
            } else {
                "contact-item contact-clickable"
            },

            // Main clickable area
            div {
                class: "contact-info",
                onclick: move |_| {
                    if !is_pending && !is_blocked {
                        let mut s = state.write();
                        s.peer_realm_messages.clear();
                        s.peer_realm_messages.clear();
                        s.peer_realm_draft.clear();
                        s.peer_realm_last_seq = 0;
                        s.peer_realm_message_count = 0;
                        s.peer_realm_action_menu_open = false;
                        s.peer_realm_contact_name = contact_name.clone();
                        s.peer_realm_quests.clear();
                        s.peer_realm_notes.clear();
                        s.peer_realm_artifacts.clear();
                        s.peer_realm_note_form_open = false;
                        s.peer_realm_note_draft_title.clear();
                        s.peer_realm_note_draft_content.clear();
                        s.peer_realm_claiming_quest_id = None;
                        s.peer_realm_claim_proof_text.clear();
                        s.step = GenesisStep::PeerRealm(mid);
                    }
                },

                // Sentiment indicator
                if !sentiment_icon.is_empty() {
                    span { class: "{sentiment_class}", "{sentiment_icon}" }
                }

                if let Some(ref name) = contact_name {
                    span { class: "contact-name", "{name}" }
                    span { class: "contact-id contact-id-secondary", "{id_short}" }
                } else {
                    span { class: "contact-id", "{id_short}" }
                }

                if is_pending {
                    span { class: "contact-status-badge", "(pending)" }
                }
                if is_blocked {
                    span { class: "contact-status-badge contact-blocked-badge", "(blocked)" }
                }
            }

            // Sentiment action buttons
            div {
                class: "contact-actions",

                if sentiment != ContactSentiment::Recommend {
                    button {
                        class: "contact-action-btn contact-recommend-btn",
                        title: "Recommend",
                        onclick: move |_| {
                            // Update sentiment to Recommend
                            let mut s = state.write();
                            if let Some(c) = s.contacts.iter_mut().find(|c| c.member_id == mid) {
                                c.sentiment = ContactSentiment::Recommend;
                            }
                        },
                        "\u{2b50}"
                    }
                }

                if sentiment == ContactSentiment::Recommend {
                    button {
                        class: "contact-action-btn contact-neutral-btn",
                        title: "Remove recommendation",
                        onclick: move |_| {
                            let mut s = state.write();
                            if let Some(c) = s.contacts.iter_mut().find(|c| c.member_id == mid) {
                                c.sentiment = ContactSentiment::Neutral;
                            }
                        },
                        "\u{2212}" // Minus sign
                    }
                }

                if sentiment != ContactSentiment::Blocked {
                    button {
                        class: "contact-action-btn contact-block-btn",
                        title: "Block",
                        onclick: move |_| {
                            let mut s = state.write();
                            if let Some(c) = s.contacts.iter_mut().find(|c| c.member_id == mid) {
                                c.sentiment = ContactSentiment::Blocked;
                            }
                        },
                        "\u{1f6ab}"
                    }
                } else {
                    button {
                        class: "contact-action-btn contact-unblock-btn",
                        title: "Unblock",
                        onclick: move |_| {
                            let mut s = state.write();
                            if let Some(c) = s.contacts.iter_mut().find(|c| c.member_id == mid) {
                                c.sentiment = ContactSentiment::Neutral;
                            }
                        },
                        "\u{2713}" // Checkmark
                    }
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
