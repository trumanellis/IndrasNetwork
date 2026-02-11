//! Root application component with state machine routing.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{IdentityCode, IndrasNetwork};
use indras_sync_engine::{HomeRealmQuests, HomeRealmNotes};

use crate::state::{AsyncStatus, ContactView, ContactSentiment, EventDirection, EventLogEntry, GenesisState, GenesisStep, NoteView, QuestAttentionView, QuestClaimView, QuestStatus, QuestView};
use indras_ui::{ArtifactDisplayInfo, ArtifactDisplayStatus, ContactInviteOverlay, ThemedRoot};

use super::display_name::DisplayNameScreen;
use super::home_realm::HomeRealmScreen;
use super::pass_story_flow::PassStoryFlow;
use super::peer_realm::PeerRealmScreen;
use super::welcome::WelcomeScreen;

/// Get the default data directory for Indras Network.
///
/// Respects `INDRAS_DATA_DIR` env var for multi-instance mode.
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("INDRAS_DATA_DIR") {
        return PathBuf::from(dir);
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Application Support/indras-network");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_DIR") {
            return PathBuf::from(xdg).join("indras-network");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/share/indras-network");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("indras-network");
        }
    }
    PathBuf::from(".").join("indras-network")
}

/// Helper to hex-encode a 16-byte ID.
fn hex_id(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Push an event to the log (newest first, capped at 50).
pub fn log_event(state: &mut Signal<GenesisState>, direction: EventDirection, message: impl Into<String>) {
    let now = chrono::Local::now();
    let entry = EventLogEntry {
        timestamp: now.format("%H:%M:%S").to_string(),
        direction,
        message: message.into(),
    };
    let mut s = state.write();
    s.event_log.insert(0, entry);
    s.event_log.truncate(50);
}

/// Root application component.
#[component]
pub fn App() -> Element {
    // Set theme inside component where Dioxus runtime is available
    use_hook(|| {
        *indras_ui::CURRENT_THEME.write() = indras_ui::Theme::MinimalTerminal;
    });

    let mut state = use_signal(GenesisState::new);
    let mut network: Signal<Option<Arc<IndrasNetwork>>> = use_signal(|| None);

    // On shutdown: save world view and stop network
    let network_for_cleanup = network;
    use_drop(move || {
        if let Some(net) = network_for_cleanup.read().as_ref() {
            let net = net.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = net.save_world_view().await {
                        tracing::error!(error = %e, "Failed to save world view on shutdown");
                    }
                    if let Err(e) = net.stop().await {
                        tracing::error!(error = %e, "Failed to stop network on shutdown");
                    }
                });
            })
            .join()
            .ok();
        }
    });

    // On mount: check if returning user
    use_effect(move || {
        spawn(async move {
            let data_dir = default_data_dir();

            if !IndrasNetwork::is_first_run(&data_dir) {
                // Returning user: load existing network
                tracing::info!("Returning user detected, loading network");
                log_event(&mut state, EventDirection::System, "Loading existing identity...");
                state.write().status = AsyncStatus::Loading;

                match IndrasNetwork::new(&data_dir).await {
                    Ok(net) => {
                        let id = net.id();
                        let id_short = id.iter()
                            .take(8)
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        let name = net.display_name().unwrap_or("").to_string();
                        log_event(&mut state, EventDirection::System, format!("Identity loaded: {} ({})", name, id_short));

                        let net = Arc::new(net);

                        // Start the network (enables inbox listener for incoming connections)
                        log_event(&mut state, EventDirection::System, "Starting network...");
                        if let Err(e) = net.start().await {
                            tracing::warn!(error = %e, "Failed to start network (non-fatal)");
                            log_event(&mut state, EventDirection::System, format!("Network start warning: {}", e));
                        }

                        // Load home realm
                        log_event(&mut state, EventDirection::System, "Joining home realm...");
                        if let Ok(home) = net.home_realm().await {
                            // Load quests with full claim information
                            if let Ok(doc) = home.quests().await {
                                let data = doc.read().await;
                                let quests: Vec<QuestView> = data.quests.iter().map(|q| {
                                    let creator_id_short: String = q.creator.iter().take(8).map(|b| format!("{:02x}", b)).collect();
                                    let is_creator = q.creator == id;
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

                        // Generate identity code (compact bech32m — no async needed)
                        let identity_uri = net.identity_uri();
                        state.write().invite_code_uri = Some(identity_uri.clone());
                        log_event(&mut state, EventDirection::System, format!("Identity code ready ({})", &identity_uri[..20.min(identity_uri.len())]));

                        // Join contacts realm (must be joined, not just read from cache)
                        log_event(&mut state, EventDirection::System, "Joining contacts realm...");
                        match net.join_contacts_realm().await {
                            Ok(_) => log_event(&mut state, EventDirection::System, "Contacts realm joined"),
                            Err(e) => {
                                tracing::debug!("Contacts realm join: {}", e);
                                log_event(&mut state, EventDirection::System, format!("Contacts realm: {}", e));
                            }
                        }

                        // Load contacts
                        log_event(&mut state, EventDirection::System, "Loading contacts...");
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
                                let count = contacts.len();
                                drop(data);
                                state.write().contacts = contacts;
                                log_event(&mut state, EventDirection::System, format!("Contacts: loaded {} connection(s)", count));
                            }
                        }

                        network.set(Some(net));

                        let mut s = state.write();
                        s.display_name = name;
                        s.member_id_short = Some(id_short);
                        s.status = AsyncStatus::Idle;
                        s.step = GenesisStep::HomeRealm;
                    }
                    Err(e) => {
                        tracing::error!("Failed to load network: {}", e);
                        log_event(&mut state, EventDirection::System, format!("ERROR: Failed to load network: {}", e));
                        state.write().status = AsyncStatus::Error(e.to_string());
                        // Fall through to genesis flow
                    }
                }
            } else if let Ok(auto_name) = std::env::var("INDRAS_NAME") {
                // Auto-create mode: skip onboarding, create identity with given name
                tracing::info!("Auto-create mode: creating identity for '{}'", auto_name);
                create_identity_and_load(
                    Some(auto_name),
                    &mut state,
                    &mut network,
                ).await;
            }
        });
    });

    let current_step = state.read().step.clone();
    let pass_story_active = state.read().pass_story_active;
    let contact_invite_open = state.read().contact_invite_open;

    // Contact invite URI comes from pre-computed state (async, includes transport info)
    let invite_uri = use_memo(move || {
        state.read().invite_code_uri.clone().unwrap_or_default()
    });

    let ci_display_name = use_memo(move || {
        state.read().display_name.clone()
    });

    let ci_member_id_short = use_memo(move || {
        state.read().member_id_short.clone().unwrap_or_default()
    });

    let ci_input = use_signal(|| state.read().contact_invite_input.clone());
    let mut ci_open = use_signal(|| state.read().contact_invite_open);

    // Keep signals in sync with state
    use_effect(move || {
        ci_open.set(state.read().contact_invite_open);
    });

    let ci_status = use_memo(move || {
        state.read().contact_invite_status.clone()
    });

    let ci_parsed_name = use_memo(move || {
        state.read().contact_parsed_name.clone()
    });

    let ci_copy_feedback = use_memo(move || {
        state.read().contact_copy_feedback
    });

    // Close handler: sync is_open signal back to state.
    // Use peek() so this effect only triggers when ci_open changes,
    // NOT when state changes (which would race with the forward sync).
    use_effect(move || {
        if !ci_open() && state.peek().contact_invite_open {
            state.write().contact_invite_open = false;
            state.write().contact_invite_status = None;
        }
    });

    rsx! {
        ThemedRoot {
            div {
                class: "genesis-app",

                match current_step {
                    GenesisStep::Welcome => rsx! {
                        WelcomeScreen { state }
                    },
                    GenesisStep::DisplayName => rsx! {
                        DisplayNameScreen { state, network }
                    },
                    GenesisStep::HomeRealm => rsx! {
                        HomeRealmScreen { state, network }
                    },
                    GenesisStep::PeerRealm(peer_id) => rsx! {
                        PeerRealmScreen { state, network, peer_id }
                    },
                }

                if pass_story_active {
                    PassStoryFlow { state, network }
                }

                if contact_invite_open {
                    ContactInviteOverlay {
                        is_open: ci_open,
                        invite_uri,
                        display_name: ci_display_name,
                        member_id_short: ci_member_id_short,
                        connect_input: ci_input,
                        connect_status: ci_status,
                        parsed_inviter_name: ci_parsed_name,
                        on_connect: move |uri: String| {
                            let mut state = state;
                            let network = network;
                            spawn(async move {
                                tracing::info!(uri = %uri, "on_connect: parsing identity/invite code");
                                state.write().contact_connecting = true;

                                // Clone the Arc to avoid holding Signal read guard across awaits
                                let net = {
                                    let guard = network.read();
                                    guard.as_ref().cloned()
                                };
                                let Some(net) = net else {
                                    tracing::error!("on_connect: network is None");
                                    state.write().contact_connecting = false;
                                    return;
                                };

                                // Parse identity code (indra1...)
                                let (_code, name) = match IdentityCode::parse_uri(&uri) {
                                    Ok(parsed) => parsed,
                                    Err(e) => {
                                        log_event(&mut state, EventDirection::System, format!("ERROR: Invalid identity code: {}", e));
                                        let mut s = state.write();
                                        s.contact_invite_status = Some("error:Invalid identity code. Paste an indra1... code.".to_string());
                                        s.contact_connecting = false;
                                        return;
                                    }
                                };
                                let peer_name = name.unwrap_or_else(|| "peer".to_string());
                                log_event(&mut state, EventDirection::System, format!("Connecting to {}...", peer_name));
                                let connect_result = net.connect_by_code(&uri).await.map(|_| peer_name);

                                match connect_result {
                                    Ok(peer_name) => {
                                        tracing::info!("on_connect: connection established");
                                        log_event(&mut state, EventDirection::Sent, format!("Connected to {}", peer_name));
                                        // Reload contacts
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
                                                let count = contacts.len();
                                                drop(data);
                                                state.write().contacts = contacts;
                                                log_event(&mut state, EventDirection::System, format!("Contacts: {} connection(s)", count));
                                            }
                                        }
                                        let mut s = state.write();
                                        s.contact_invite_input.clear();
                                        s.contact_parsed_name = None;
                                        s.contact_invite_status = None;
                                        s.contact_connecting = false;
                                        s.contact_invite_open = false;
                                    }
                                    Err(e) => {
                                        let err_str = e.to_string();
                                        tracing::error!(error = %err_str, "on_connect: connect failed");
                                        log_event(&mut state, EventDirection::System, format!("ERROR: {}", err_str));
                                        let mut s = state.write();
                                        s.contact_invite_status = Some(format!("error:Connection failed: {}", err_str));
                                        s.contact_connecting = false;
                                    }
                                }
                            });
                        },
                        on_parse_input: move |input: String| {
                            state.write().contact_invite_input = input.clone();
                            // Parse identity code to extract display name
                            match IdentityCode::parse_uri(&input) {
                                Ok((_code, name)) => {
                                    state.write().contact_parsed_name = name;
                                }
                                Err(_) => {
                                    state.write().contact_parsed_name = None;
                                }
                            }
                        },
                        copy_feedback: ci_copy_feedback,
                        on_copy: move |_| {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let uri = invite_uri();
                                let _ = clipboard.set_text(uri);
                                state.write().contact_copy_feedback = true;
                                // Close modal immediately after copying
                                state.write().contact_invite_open = false;
                                // Reset feedback after 2 seconds
                                spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                    state.write().contact_copy_feedback = false;
                                });
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Create identity and load home realm after display name entry.
pub async fn create_identity_and_load(
    name: Option<String>,
    state: &mut Signal<GenesisState>,
    network: &mut Signal<Option<Arc<IndrasNetwork>>>,
) {
    let data_dir = default_data_dir();

    // Ensure data dir exists before building (profile save needs it)
    let _ = std::fs::create_dir_all(&data_dir);

    log_event(state, EventDirection::System, "Creating new identity...");
    state.write().status = AsyncStatus::Loading;

    // Build network with display name
    let mut builder = IndrasNetwork::builder().data_dir(&data_dir);
    if let Some(ref n) = name {
        if !n.is_empty() {
            builder = builder.display_name(n.as_str());
        }
    }

    match builder.build().await {
        Ok(net) => {
            let id = net.id();
            let id_short = id.iter()
                .take(8)
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            let display = net.display_name().unwrap_or("").to_string();
            log_event(state, EventDirection::System, format!("Identity created: {} ({})", display, id_short));

            let net = Arc::new(net);

            // Start the network (enables inbox listener for incoming connections)
            log_event(state, EventDirection::System, "Starting network...");
            if let Err(e) = net.start().await {
                tracing::warn!(error = %e, "Failed to start network (non-fatal)");
                log_event(state, EventDirection::System, format!("Network start warning: {}", e));
            }

            // Load home realm - quests and notes
            log_event(state, EventDirection::System, "Joining home realm...");
            if let Ok(home) = net.home_realm().await {
                if let Ok(doc) = home.quests().await {
                    let data = doc.read().await;
                    let quests: Vec<QuestView> = data.quests.iter().map(|q| {
                        let creator_id_short: String = q.creator.iter().take(8).map(|b| format!("{:02x}", b)).collect();
                        let is_creator = q.creator == id;
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
                    state.write().quests = quests;
                }

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

            // Join contacts realm eagerly (enables store-and-forward inbox delivery)
            log_event(state, EventDirection::System, "Joining contacts realm...");
            match net.join_contacts_realm().await {
                Ok(_) => log_event(state, EventDirection::System, "Contacts realm joined"),
                Err(e) => {
                    tracing::debug!("Contacts realm join: {}", e);
                    log_event(state, EventDirection::System, format!("Contacts realm: {}", e));
                }
            }

            // Generate compact identity code for sharing
            log_event(state, EventDirection::System, "Generating identity code...");
            let identity_uri = net.identity_uri();
            state.write().invite_code_uri = Some(identity_uri);
            log_event(state, EventDirection::System, "Identity code ready");

            // Load contacts
            log_event(state, EventDirection::System, "Loading contacts...");
            if let Some(contacts_realm) = net.contacts_realm().await {
                if let Ok(doc) = contacts_realm.contacts().await {
                    let data = doc.read().await;
                    let contacts: Vec<ContactView> = data.contacts.iter().map(|(mid, entry)| {
                        ContactView {
                            member_id: *mid,
                            member_id_short: mid.iter().take(8).map(|b| format!("{:02x}", b)).collect(),
                            display_name: entry.display_name.clone(),
                            status: "confirmed".to_string(),
                        }
                    }).collect();
                    let count = contacts.len();
                    drop(data);
                    state.write().contacts = contacts;
                    log_event(state, EventDirection::System, format!("Contacts: loaded {} connection(s)", count));
                }
            }

            network.set(Some(net));

            let mut s = state.write();
            s.display_name = display;
            let id_log = id_short.clone();
            s.member_id_short = Some(id_short);
            s.status = AsyncStatus::Idle;
            s.step = GenesisStep::HomeRealm;

            tracing::info!("Identity created, member ID: {}", id_log);
        }
        Err(e) => {
            tracing::error!("Failed to create identity: {}", e);
            log_event(state, EventDirection::System, format!("ERROR: Failed to create identity: {}", e));
            state.write().status = AsyncStatus::Error(format!("Failed to create identity: {}", e));
        }
    }
}
