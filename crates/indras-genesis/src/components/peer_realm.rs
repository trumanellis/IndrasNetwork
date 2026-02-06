//! Peer realm screen - full shared realm view with a contact.
//!
//! Shows quests, notes, artifacts, and chat shared between you and a contact.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{Content, IndrasNetwork, Message};
use indras_sync_engine::{RealmQuests, RealmNotes, SyncContent};
use indras_ui::{member_color_class, ArtifactDisplayInfo, ArtifactDisplayStatus, ArtifactGallery};

use crate::state::{
    GenesisState, GenesisStep, NoteView, PeerMessageType, PeerMessageView,
    QuestAttentionView, QuestClaimView, QuestStatus, QuestView,
};

/// Helper to hex-encode a 16-byte ID.
fn hex_id(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{:02x}", b)).collect()
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

/// Convert a network Message to a PeerMessageView for rendering.
fn convert_message(msg: &Message, my_id: &[u8; 32], seq: u64) -> PeerMessageView {
    let sender_id = msg.sender.id();
    let sender_id_short: String = sender_id
        .iter()
        .take(8)
        .map(|b| format!("{:02x}", b))
        .collect();
    let is_me = sender_id == *my_id;

    // Edit tracking not yet supported at the network layer
    let is_edited = false;
    let edited_at: Option<String> = None;

    let message_type = match &msg.content {
        Content::Text(s) => PeerMessageType::Text {
            content: s.clone(),
        },
        Content::Image {
            mime_type,
            data,
            filename,
            alt_text,
            ..
        } => PeerMessageType::Image {
            data_url: Some(format!("data:{};base64,{}", mime_type, data)),
            filename: filename.clone(),
            alt_text: alt_text.clone(),
        },
        Content::System(s) => PeerMessageType::System {
            content: s.clone(),
        },
        Content::Artifact(r) => PeerMessageType::Artifact {
            name: r.name.clone(),
            size: r.size,
            mime_type: r.mime_type.clone(),
        },
        Content::Extension { .. } => {
            match SyncContent::from_content(&msg.content) {
                Some(SyncContent::ProofSubmitted {
                    quest_id,
                    claimant,
                    ..
                }) => PeerMessageType::ProofSubmitted {
                    quest_id_short: quest_id
                        .iter()
                        .take(4)
                        .map(|b| format!("{:02x}", b))
                        .collect(),
                    claimant_name: claimant
                        .iter()
                        .take(4)
                        .map(|b| format!("{:02x}", b))
                        .collect(),
                },
                Some(SyncContent::BlessingGiven {
                    claimant,
                    ..
                }) => PeerMessageType::BlessingGiven {
                    claimant_name: claimant
                        .iter()
                        .take(4)
                        .map(|b| format!("{:02x}", b))
                        .collect(),
                    duration: String::new(),
                },
                Some(SyncContent::ProofFolderSubmitted {
                    narrative_preview,
                    artifact_count,
                    ..
                }) => PeerMessageType::ProofFolderSubmitted {
                    narrative_preview: narrative_preview.clone(),
                    artifact_count,
                },
                _ => PeerMessageType::System {
                    content: "[unknown extension message]".to_string(),
                },
            }
        },
        Content::Gallery { title, items, .. } => PeerMessageType::Gallery {
            title: title.clone(),
            item_count: items.len(),
        },
        Content::Reaction { emoji, .. } => PeerMessageType::Reaction {
            emoji: emoji.clone(),
        },
        _ => PeerMessageType::System {
            content: "[unsupported message type]".to_string(),
        },
    };

    PeerMessageView {
        sender_name: msg.sender.name(),
        sender_id_short,
        is_me,
        timestamp: msg.timestamp.format("%H:%M").to_string(),
        message_type,
        seq,
        is_edited,
        edited_at,
    }
}

/// Join or get a peer realm, safely handling the blocking_read() inside realm().
async fn get_peer_realm(
    net: &Arc<IndrasNetwork>,
    peers: Vec<[u8; 32]>,
) -> Result<indras_network::Realm, indras_network::IndraError> {
    let net = Arc::clone(net);
    let handle = tokio::runtime::Handle::current();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let result = handle.block_on(net.realm(peers));
        let _ = tx.send(result);
    });
    rx.await.unwrap_or_else(|_| {
        Err(indras_network::IndraError::InvalidOperation(
            "realm join thread failed".to_string(),
        ))
    })
}

/// Load all shared realm data: quests, notes, artifacts, and messages.
async fn load_shared_realm_data(
    net: &Arc<IndrasNetwork>,
    peer_id: [u8; 32],
    state: &mut Signal<GenesisState>,
) {
    let my_id = net.id();
    let peers = vec![my_id, peer_id];

    match get_peer_realm(net, peers).await {
        Ok(realm) => {
            // Load messages
            match realm.messages_since(0).await {
                Ok(messages) => {
                    let count = messages.len();
                    let views: Vec<PeerMessageView> = messages
                        .iter()
                        .enumerate()
                        .map(|(i, m)| convert_message(m, &my_id, i as u64))
                        .collect();
                    let mut s = state.write();
                    s.peer_realm_messages = views;
                    s.peer_realm_message_count = count;
                    s.peer_realm_last_seq = count as u64;
                }
                Err(e) => {
                    tracing::error!("load_shared_realm_data: messages_since failed: {}", e);
                }
            }

            // Load quests
            if let Ok(doc) = realm.quests().await {
                let data = doc.read().await;
                let quests: Vec<QuestView> = data.quests.iter().map(|q| {
                    let creator_id_short: String = q.creator.iter().take(8).map(|b| format!("{:02x}", b)).collect();
                    let is_creator = q.creator == my_id;
                    let is_complete = q.completed_at_millis.is_some();

                    let claims: Vec<QuestClaimView> = q.claims.iter().map(|c| {
                        let claimant_id_short: String = c.claimant.iter().take(8).map(|b| format!("{:02x}", b)).collect();
                        QuestClaimView {
                            claimant_id_short,
                            claimant_name: None,
                            verified: c.verified,
                            has_proof: c.has_proof(),
                            submitted_at: chrono::DateTime::from_timestamp_millis(c.submitted_at_millis)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_default(),
                        }
                    }).collect();

                    let pending_claim_count = q.pending_claims().len();
                    let verified_claim_count = q.verified_claims().len();

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
                state.write().peer_realm_quests = quests;
            }

            // Load notes
            if let Ok(doc) = realm.notes().await {
                let data = doc.read().await;
                let notes: Vec<NoteView> = data.notes.iter().map(|n| {
                    NoteView {
                        id: hex_id(&n.id),
                        title: n.title.clone(),
                        content_preview: n.content.chars().take(100).collect(),
                    }
                }).collect();
                drop(data);
                state.write().peer_realm_notes = notes;
            }

            // Load artifacts from artifact key registry if available
            if let Ok(doc) = realm.artifact_key_registry().await {
                let data = doc.read().await;
                let artifacts: Vec<ArtifactDisplayInfo> = data.artifacts.values().map(|a| {
                    ArtifactDisplayInfo {
                        id: a.hash.iter().map(|b| format!("{:02x}", b)).collect(),
                        name: a.name.clone(),
                        size: a.size,
                        mime_type: a.mime_type.clone(),
                        status: if a.status.is_shared() {
                            ArtifactDisplayStatus::Active
                        } else {
                            ArtifactDisplayStatus::Recalled
                        },
                        data_url: None,
                        grant_count: 0,
                        owner_label: Some(format!("Shared by {}", &a.sharer[..8.min(a.sharer.len())])),
                    }
                }).collect();
                drop(data);
                state.write().peer_realm_artifacts = artifacts;
            }
        }
        Err(e) => {
            tracing::error!("load_shared_realm_data: realm() failed: {}", e);
        }
    }
}

#[component]
pub fn PeerRealmScreen(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: [u8; 32],
) -> Element {
    // Initial data load
    use_effect(move || {
        spawn(async move {
            let net = {
                let guard = network.read();
                guard.as_ref().cloned()
            };
            if let Some(net) = net {
                load_shared_realm_data(&net, peer_id, &mut state).await;
            }
        });
    });

    // 3-second polling loop for updates
    use_effect(move || {
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let net = {
                    let guard = network.read();
                    guard.as_ref().cloned()
                };
                if let Some(net) = net {
                    let current_step = state.read().step.clone();
                    if current_step != GenesisStep::PeerRealm(peer_id) {
                        break;
                    }
                    load_shared_realm_data(&net, peer_id, &mut state).await;
                }
            }
        });
    });

    // Read state for rendering
    let s = state.read();
    let contact_name = s
        .peer_realm_contact_name
        .clone()
        .unwrap_or_else(|| "Contact".to_string());
    let messages = s.peer_realm_messages.clone();
    let message_count = s.peer_realm_message_count;
    let quests = s.peer_realm_quests.clone();
    let quest_count = quests.len();
    let notes = s.peer_realm_notes.clone();
    let note_count = notes.len();
    let artifacts = s.peer_realm_artifacts.clone();
    let artifact_count = artifacts.len();
    let draft = s.peer_realm_draft.clone();
    let action_menu_open = s.peer_realm_action_menu_open;
    let note_form_open = s.peer_realm_note_form_open;
    drop(s);

    let draft_empty = draft.trim().is_empty();

    rsx! {
        div {
            class: "genesis-screen peer-realm-screen shared-realm-view",

            // Header
            header {
                class: "peer-realm-header shared-realm-header",

                button {
                    class: "genesis-btn-secondary",
                    onclick: move |_| {
                        // Clear peer realm state when leaving
                        let mut s = state.write();
                        s.peer_realm_quests.clear();
                        s.peer_realm_notes.clear();
                        s.peer_realm_artifacts.clear();
                        s.peer_realm_messages.clear();
                        s.peer_realm_note_form_open = false;
                        s.peer_realm_claiming_quest_id = None;
                        s.step = GenesisStep::HomeRealm;
                    },
                    "\u{2190} Back"
                }

                h1 {
                    class: "shared-realm-title",
                    "Shared Realm with {contact_name}"
                }

                div {
                    class: "shared-realm-stats",
                    span { "{quest_count} quests" }
                    span { class: "stat-divider", "\u{b7}" }
                    span { "{note_count} notes" }
                    span { class: "stat-divider", "\u{b7}" }
                    span { "{message_count} messages" }
                }
            }

            // Main content - multi-panel layout
            div {
                class: "shared-realm-layout",

                // Left column - Quests and Notes
                div {
                    class: "shared-realm-main",

                    // Quests panel
                    section {
                        class: "home-panel shared-realm-quests",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Shared Quests" }
                            span { class: "panel-count", "{quest_count}" }
                            button {
                                class: "genesis-btn-secondary",
                                onclick: move |_| {
                                    // TODO: Create quest form
                                },
                                "+ New Quest"
                            }
                        }

                        if quests.is_empty() {
                            div {
                                class: "panel-empty",
                                p { "No shared quests yet." }
                                p { class: "panel-empty-hint", "Create a quest to collaborate on tasks together." }
                            }
                        } else {
                            div {
                                class: "quests-list",
                                for quest in quests.iter() {
                                    {render_shared_quest_item(quest, state, network, peer_id)}
                                }
                            }
                        }
                    }

                    // Notes panel
                    section {
                        class: "home-panel shared-realm-notes",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Shared Notes" }
                            span { class: "panel-count", "{note_count}" }
                            button {
                                class: "genesis-btn-secondary",
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.peer_realm_note_form_open = !s.peer_realm_note_form_open;
                                    if !s.peer_realm_note_form_open {
                                        s.peer_realm_note_draft_title.clear();
                                        s.peer_realm_note_draft_content.clear();
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
                                    value: "{state.read().peer_realm_note_draft_title}",
                                    oninput: move |evt| {
                                        state.write().peer_realm_note_draft_title = evt.value();
                                    },
                                }
                                textarea {
                                    class: "genesis-input note-form-textarea",
                                    placeholder: "Write your shared note...",
                                    rows: "4",
                                    value: "{state.read().peer_realm_note_draft_content}",
                                    oninput: move |evt| {
                                        state.write().peer_realm_note_draft_content = evt.value();
                                    },
                                }
                                button {
                                    class: "genesis-btn-primary",
                                    disabled: state.read().peer_realm_note_draft_title.trim().is_empty(),
                                    onclick: move |_| {
                                        let title = state.read().peer_realm_note_draft_title.clone();
                                        let content = state.read().peer_realm_note_draft_content.clone();
                                        spawn(async move {
                                            let net = network.read();
                                            if let Some(ref net) = *net {
                                                let my_id = net.id();
                                                let peers = vec![my_id, peer_id];
                                                if let Ok(realm) = get_peer_realm(net, peers).await {
                                                    if let Ok(_note_id) = realm.create_note(
                                                        title,
                                                        content,
                                                        my_id,
                                                        vec![],
                                                    ).await {
                                                        {
                                                            let mut s = state.write();
                                                            s.peer_realm_note_draft_title.clear();
                                                            s.peer_realm_note_draft_content.clear();
                                                            s.peer_realm_note_form_open = false;
                                                        }
                                                        load_shared_realm_data(net, peer_id, &mut state).await;
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
                                p { "No shared notes yet." }
                                p { class: "panel-empty-hint", "Create a note to share information." }
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
                        class: "home-panel shared-realm-artifacts",

                        div {
                            class: "panel-header",
                            h2 { class: "panel-title", "Shared Artifacts" }
                            span { class: "panel-count", "{artifact_count}" }
                        }

                        if artifacts.is_empty() {
                            div {
                                class: "panel-empty",
                                p { "No shared artifacts yet." }
                                p { class: "panel-empty-hint", "Share files in the chat to see them here." }
                            }
                        } else {
                            ArtifactGallery { artifacts: artifacts.clone() }
                        }
                    }
                }

                // Right column - Chat
                aside {
                    class: "shared-realm-chat",

                    div {
                        class: "chat-panel-header",
                        h2 { class: "panel-title", "Chat" }
                        span { class: "panel-count", "{message_count}" }
                    }

                    // Messages area
                    div {
                        class: "chat-messages",

                        if messages.is_empty() {
                            div {
                                class: "panel-empty",
                                "No messages yet. Send the first message!"
                            }
                        }

                        for (i, msg) in messages.iter().enumerate() {
                            {render_chat_message(msg, i, state)}
                        }
                    }

                    // Input bar
                    div {
                        class: "chat-input-container",

                        div {
                            class: "chat-input-wrapper",

                            button {
                                class: "chat-action-btn",
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.peer_realm_action_menu_open = !s.peer_realm_action_menu_open;
                                },
                                "+"
                            }

                            if action_menu_open {
                                div {
                                    class: "chat-action-menu",

                                    button {
                                        class: "action-menu-item",
                                        onclick: move |_| {
                                            state.write().peer_realm_action_menu_open = false;
                                        },
                                        "\u{1f4ce} Artifact"
                                    }
                                    button {
                                        class: "action-menu-item",
                                        onclick: move |_| {
                                            state.write().peer_realm_action_menu_open = false;
                                        },
                                        "\u{1f4c4} Document"
                                    }
                                    button {
                                        class: "action-menu-item",
                                        onclick: move |_| {
                                            state.write().peer_realm_action_menu_open = false;
                                        },
                                        "\u{2713} Proof of Service"
                                    }
                                }
                            }
                        }

                        input {
                            class: "chat-input",
                            r#type: "text",
                            placeholder: "Type a message...",
                            value: "{draft}",
                            oninput: move |evt| {
                                state.write().peer_realm_draft = evt.value();
                            },
                            onkeypress: move |evt| {
                                if evt.key() == Key::Enter {
                                    let draft = state.read().peer_realm_draft.clone();
                                    if !draft.trim().is_empty() {
                                        send_message(state, network, peer_id);
                                    }
                                }
                            },
                        }

                        button {
                            class: "chat-send-btn",
                            disabled: draft_empty,
                            onclick: move |_| {
                                send_message(state, network, peer_id);
                            },
                            "Send"
                        }
                    }
                }
            }
        }
    }
}

/// Render a shared quest item with claims and verification.
fn render_shared_quest_item(
    quest: &QuestView,
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: [u8; 32],
) -> Element {
    let quest_id = quest.id.clone();
    let quest_id_for_claim = quest.id.clone();
    let quest_id_for_complete = quest.id.clone();
    let is_complete = quest.is_complete;
    let title = quest.title.clone();
    let description = quest.description.clone();
    let status = quest.status.clone();
    let claims = quest.claims.clone();
    let pending_count = quest.pending_claim_count;
    let verified_count = quest.verified_claim_count;
    let is_creator = quest.is_creator;

    let showing_claim_form = state.read().peer_realm_claiming_quest_id.as_ref() == Some(&quest_id);

    let status_badge = match status {
        QuestStatus::Open => "Open",
        QuestStatus::Claimed => "Claimed",
        QuestStatus::Verified => "Verified",
        QuestStatus::Completed => "Complete",
    };

    let status_class = match status {
        QuestStatus::Open => "quest-status-open",
        QuestStatus::Claimed => "quest-status-claimed",
        QuestStatus::Verified => "quest-status-verified",
        QuestStatus::Completed => "quest-status-complete",
    };

    rsx! {
        div {
            key: "{quest_id}",
            class: if is_complete { "quest-item quest-complete" } else { "quest-item" },

            div {
                class: if is_complete { "quest-checkbox" } else { "quest-checkbox quest-checkbox-clickable" },
                onclick: move |_| {
                    if !is_complete {
                        let qid = quest_id_for_complete.clone();
                        let mut state = state;
                        let network = network;
                        spawn(async move {
                            let net = network.read();
                            if let Some(ref net) = *net {
                                let my_id = net.id();
                                let peers = vec![my_id, peer_id];
                                if let Ok(realm) = get_peer_realm(net, peers).await {
                                    if let Some(id_bytes) = hex_to_quest_id(&qid) {
                                        if let Ok(()) = realm.complete_quest(id_bytes).await {
                                            load_shared_realm_data(net, peer_id, &mut state).await;
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

                p { class: "quest-description", "{description}" }

                // Claims section
                if !claims.is_empty() {
                    div {
                        class: "quest-claims",
                        div {
                            class: "quest-claims-header",
                            span { class: "quest-claims-title", "Claims ({pending_count} pending, {verified_count} verified)" }
                        }
                        for (idx, claim) in claims.iter().enumerate() {
                            {render_shared_quest_claim(claim, idx, &quest_id, is_creator, state, network, peer_id)}
                        }
                    }
                }

                // Action buttons
                if !is_complete {
                    div {
                        class: "quest-actions",

                        if !showing_claim_form {
                            button {
                                class: "genesis-btn-secondary quest-claim-btn",
                                onclick: move |_| {
                                    state.write().peer_realm_claiming_quest_id = Some(quest_id_for_claim.clone());
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
                            value: "{state.read().peer_realm_claim_proof_text}",
                            oninput: move |evt| {
                                state.write().peer_realm_claim_proof_text = evt.value();
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
                                            let my_id = net.id();
                                            let peers = vec![my_id, peer_id];
                                            if let Ok(realm) = get_peer_realm(net, peers).await {
                                                if let Some(id_bytes) = hex_to_quest_id(&qid) {
                                                    if let Ok(_idx) = realm.submit_quest_claim(id_bytes, my_id, None).await {
                                                        state.write().peer_realm_claiming_quest_id = None;
                                                        state.write().peer_realm_claim_proof_text.clear();
                                                        load_shared_realm_data(net, peer_id, &mut state).await;
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
                                    state.write().peer_realm_claiming_quest_id = None;
                                    state.write().peer_realm_claim_proof_text.clear();
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

/// Render a quest claim for shared realm.
fn render_shared_quest_claim(
    claim: &QuestClaimView,
    claim_index: usize,
    quest_id: &str,
    is_creator: bool,
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: [u8; 32],
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
                                let my_id = net.id();
                                let peers = vec![my_id, peer_id];
                                if let Ok(realm) = get_peer_realm(net, peers).await {
                                    if let Some(id_bytes) = hex_to_quest_id(&qid) {
                                        if let Ok(()) = realm.verify_quest_claim(id_bytes, idx).await {
                                            load_shared_realm_data(net, peer_id, &mut state).await;
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

/// Send the current draft as a text message.
fn send_message(
    mut state: Signal<GenesisState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    peer_id: [u8; 32],
) {
    let draft = state.read().peer_realm_draft.clone();
    if draft.trim().is_empty() {
        return;
    }
    spawn(async move {
        let net = {
            let guard = network.read();
            guard.as_ref().cloned()
        };
        if let Some(net) = net {
            let my_id = net.id();
            let peers = vec![my_id, peer_id];
            if let Ok(realm) = get_peer_realm(&net, peers).await {
                if realm.send(draft.as_str()).await.is_ok() {
                    state.write().peer_realm_draft.clear();
                    load_shared_realm_data(&net, peer_id, &mut state).await;
                }
            }
        }
    });
}

/// Render a single chat message based on its type.
fn render_chat_message(
    msg: &PeerMessageView,
    index: usize,
    mut state: Signal<GenesisState>,
) -> Element {
    let color_class = member_color_class(&msg.sender_id_short);
    let sender = msg.sender_name.clone();
    let timestamp = msg.timestamp.clone();
    let is_me = msg.is_me;
    let is_edited = msg.is_edited;
    let edited_at = msg.edited_at.clone();
    let msg_seq = msg.seq;

    let is_editing = state.read().editing_message_seq == Some(msg_seq);

    match &msg.message_type {
        PeerMessageType::Text { content } => {
            let content = content.clone();
            let content_for_edit = content.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message text-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }

                        if is_editing {
                            input {
                                class: "chat-edit-input",
                                r#type: "text",
                                value: "{state.read().edit_message_draft}",
                                oninput: move |evt| {
                                    state.write().edit_message_draft = evt.value();
                                },
                                onkeypress: move |evt| {
                                    if evt.key() == Key::Enter {
                                        state.write().editing_message_seq = None;
                                        state.write().edit_message_draft.clear();
                                    } else if evt.key() == Key::Escape {
                                        state.write().editing_message_seq = None;
                                        state.write().edit_message_draft.clear();
                                    }
                                },
                            }
                            button {
                                class: "chat-edit-cancel",
                                onclick: move |_| {
                                    state.write().editing_message_seq = None;
                                    state.write().edit_message_draft.clear();
                                },
                                "\u{2717}"
                            }
                        } else {
                            span { class: "chat-content", "{content}" }

                            if is_edited {
                                span {
                                    class: "chat-edited-indicator",
                                    title: if let Some(ref t) = edited_at { "Edited at {t}" } else { "Edited" },
                                    "(edited)"
                                }
                            }

                            if is_me {
                                button {
                                    class: "chat-edit-btn",
                                    onclick: move |_| {
                                        state.write().editing_message_seq = Some(msg_seq);
                                        state.write().edit_message_draft = content_for_edit.clone();
                                    },
                                    "\u{270e}"
                                }
                            }
                        }
                    }
                }
            }
        }

        PeerMessageType::Image {
            data_url,
            filename,
            alt_text,
        } => {
            let alt = alt_text
                .clone()
                .or_else(|| filename.clone())
                .unwrap_or_else(|| "Image".to_string());
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message image-message",

                    div {
                        class: "chat-message-header",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-content", "shared an image" }
                    }

                    if let Some(url) = data_url {
                        div {
                            class: "chat-image-container",
                            img {
                                class: "chat-inline-image",
                                src: "{url}",
                                alt: "{alt}",
                            }
                        }
                    } else {
                        div {
                            class: "chat-image-placeholder",
                            "\u{1f5bc} {alt}"
                        }
                    }
                }
            }
        }

        PeerMessageType::System { content } => {
            let content = content.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message system-message",
                    span { class: "chat-content", "{content}" }
                }
            }
        }

        PeerMessageType::Artifact {
            name,
            size,
            mime_type,
        } => {
            let name = name.clone();
            let size_str = format_size(*size);
            let mime = mime_type
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message proof-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-icon", "\u{1f4ce}" }
                        span { class: "chat-content", "{name} ({size_str}, {mime})" }
                    }
                }
            }
        }

        PeerMessageType::ProofSubmitted {
            quest_id_short,
            claimant_name,
        } => {
            let quest_id_short = quest_id_short.clone();
            let claimant_name = claimant_name.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message proof-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span { class: "chat-icon", "\u{1f4ce}" }
                        span { class: "chat-content", "Proof submitted for quest {quest_id_short} by {claimant_name}" }
                    }
                }
            }
        }

        PeerMessageType::BlessingGiven {
            claimant_name,
            duration,
        } => {
            let claimant_name = claimant_name.clone();
            let duration = duration.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message blessing-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span { class: "chat-icon", "\u{2728}" }
                        span { class: "chat-content",
                            if duration.is_empty() {
                                "Blessing given to {claimant_name}"
                            } else {
                                "Blessing given to {claimant_name} ({duration})"
                            }
                        }
                    }
                }
            }
        }

        PeerMessageType::ProofFolderSubmitted {
            narrative_preview,
            artifact_count,
        } => {
            let preview = narrative_preview.clone();
            let count = *artifact_count;
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message proof-folder-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span { class: "chat-icon", "\u{1f4cb}" }
                        span { class: "chat-content", "{preview}" }
                    }
                    span { class: "proof-artifact", "{count} attachment(s)" }
                }
            }
        }

        PeerMessageType::Gallery { title, item_count } => {
            let title_str = title
                .clone()
                .unwrap_or_else(|| "Gallery".to_string());
            let count = *item_count;
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message gallery-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-icon", "\u{1f5bc}" }
                        span { class: "gallery-title", "{title_str}" }
                        span { class: "gallery-count", "({count} items)" }
                    }
                }
            }
        }

        PeerMessageType::Reaction { emoji } => {
            let emoji = emoji.clone();
            rsx! {
                div {
                    key: "{index}",
                    class: "chat-message text-message",

                    div {
                        class: "chat-message-row",
                        span { class: "chat-tick", "{timestamp}" }
                        span {
                            class: "chat-sender {color_class}",
                            if is_me { "You" } else { "{sender}" }
                        }
                        span { class: "chat-content", "{emoji}" }
                    }
                }
            }
        }
    }
}

/// Format a byte size into a human-readable string.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
