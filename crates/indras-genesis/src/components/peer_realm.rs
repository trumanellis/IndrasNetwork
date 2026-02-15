//! Peer realm screen - full shared realm view with a contact.
//!
//! Shows quests, notes, artifacts, and chat shared between you and a contact.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{IndrasNetwork, direct_connect::dm_realm_id};
use indras_sync_engine::{RealmQuests, RealmNotes};
use indras_ui::{ArtifactDisplayInfo, ArtifactDisplayStatus, ArtifactGallery};
use indras_ui::chat::ChatPanel;

use crate::state::{
    GenesisState, GenesisStep, NoteEditorMode, NoteView,
    QuestAttentionView, QuestClaimView, QuestEditorMode, QuestStatus, QuestView,
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

/// Get the DM realm for a peer using the correct dm_realm_id.
///
/// This uses the deterministic DM realm ID (with "dm-v1:" prefix) that matches
/// how DM realms are created during contact connection, ensuring both peers
/// read/write to the same realm.
fn get_peer_realm(
    net: &Arc<IndrasNetwork>,
    my_id: [u8; 32],
    peer_id: [u8; 32],
) -> Result<indras_network::Realm, indras_network::IndraError> {
    let dm_id = dm_realm_id(my_id, peer_id);
    net.get_realm_by_id(&dm_id).ok_or_else(|| {
        indras_network::IndraError::InvalidOperation(
            format!("DM realm not found for peer {:?}", &peer_id[..4])
        )
    })
}

/// Load all shared realm data: quests, notes, and artifacts.
async fn load_shared_realm_data(
    net: &Arc<IndrasNetwork>,
    peer_id: [u8; 32],
    state: &mut Signal<GenesisState>,
) {
    let my_id = net.id();

    match get_peer_realm(net, my_id, peer_id) {
        Ok(realm) => {
            // Load quests (refresh to get CRDT-synced state from peers)
            if let Ok(doc) = realm.quests().await {
                // Refresh to pull latest synced state from peers
                let _ = doc.refresh().await;
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

            // Load notes (refresh to get CRDT-synced state from peers)
            if let Ok(doc) = realm.notes().await {
                // Refresh to pull latest synced state from peers
                let _ = doc.refresh().await;
                let data = doc.read().await;
                let notes: Vec<NoteView> = data.notes.iter().map(|n| {
                    NoteView {
                        id: hex_id(&n.id),
                        title: n.title.clone(),
                        content: n.content.clone(),
                        content_preview: n.content.chars().take(100).collect(),
                    }
                }).collect();
                drop(data);
                state.write().peer_realm_notes = notes;
            }

            // Peer realm artifacts are now managed via grant-based access
            // through the owner's ArtifactIndex rather than a key registry.
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
    let quests = s.peer_realm_quests.clone();
    let quest_count = quests.len();
    let notes = s.peer_realm_notes.clone();
    let note_count = notes.len();
    let artifacts = s.peer_realm_artifacts.clone();
    let artifact_count = artifacts.len();
    drop(s);

    // Get the actual 32-byte my_id for ChatPanel
    let my_id_bytes = {
        let guard = network.read();
        guard.as_ref().map(|n| n.id()).unwrap_or([0u8; 32])
    };
    let peer_name_for_chat = contact_name.clone();

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
                                    // Open quest editor in Create mode
                                    let mut s = state.write();
                                    s.quest_editor_open = true;
                                    s.quest_editor_mode = QuestEditorMode::Create;
                                    s.quest_editor_id = None;
                                    s.quest_editor_title.clear();
                                    s.quest_editor_description.clear();
                                    s.quest_editor_preview_mode = true;
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
                                    s.note_editor_open = true;
                                    s.note_editor_mode = NoteEditorMode::Create;
                                    s.note_editor_id = None;
                                    s.note_editor_title.clear();
                                    s.note_editor_content.clear();
                                    s.note_editor_preview_mode = true;
                                },
                                "+ New Note"
                            }
                        }

                        if notes.is_empty() {
                            div {
                                class: "panel-empty",
                                p { "No shared notes yet." }
                                p { class: "panel-empty-hint", "Create a note to share information." }
                            }
                        } else {
                            div {
                                class: "notes-list",
                                for note in notes.iter() {
                                    {
                                        let note_id = note.id.clone();
                                        let note_title = note.title.clone();
                                        let note_content = note.content.clone();
                                        rsx! {
                                            div {
                                                key: "{note_id}",
                                                class: "note-card note-card-clickable",
                                                onclick: move |_| {
                                                    let mut s = state.write();
                                                    s.note_editor_open = true;
                                                    s.note_editor_mode = NoteEditorMode::View;
                                                    s.note_editor_id = Some(note_id.clone());
                                                    s.note_editor_title = note_title.clone();
                                                    s.note_editor_content = note_content.clone();
                                                    s.note_editor_preview_mode = true;
                                                },
                                                h3 { class: "note-title", "{note.title}" }
                                                p { class: "note-preview", "{note.content_preview}" }
                                            }
                                        }
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

                // Right column - Chat (component-based)
                aside {
                    class: "shared-realm-chat",

                    ChatPanel {
                        network,
                        peer_id,
                        my_id: my_id_bytes,
                        peer_name: peer_name_for_chat,
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
    let quest_id_for_click = quest.id.clone();
    let quest_id_for_claim = quest.id.clone();
    let quest_id_for_complete = quest.id.clone();
    let is_complete = quest.is_complete;
    let title = quest.title.clone();
    let title_for_click = quest.title.clone();
    let description = quest.description.clone();
    let description_for_click = quest.description.clone();
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
                                if let Ok(realm) = get_peer_realm(net, my_id, peer_id) {
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
                class: "quest-content quest-content-clickable",
                onclick: move |_| {
                    // Open quest editor in View mode
                    let mut s = state.write();
                    s.quest_editor_open = true;
                    s.quest_editor_mode = QuestEditorMode::View;
                    s.quest_editor_id = Some(quest_id_for_click.clone());
                    s.quest_editor_title = title_for_click.clone();
                    s.quest_editor_description = description_for_click.clone();
                    s.quest_editor_preview_mode = true;
                },

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
                                            if let Ok(realm) = get_peer_realm(net, my_id, peer_id) {
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
                                if let Ok(realm) = get_peer_realm(net, my_id, peer_id) {
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

