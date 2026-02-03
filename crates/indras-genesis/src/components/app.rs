//! Root application component with state machine routing.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{ContactInviteCode, IndrasNetwork};

use crate::state::{AsyncStatus, ContactView, EventDirection, EventLogEntry, GenesisState, GenesisStep, NoteView, QuestView};
use indras_ui::{ArtifactDisplayInfo, ArtifactDisplayStatus, ContactInviteOverlay, ThemedRoot};

use super::display_name::DisplayNameScreen;
use super::home_realm::HomeRealmScreen;
use super::pass_story_flow::PassStoryFlow;
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

                        // Load home realm
                        log_event(&mut state, EventDirection::System, "Joining home realm...");
                        if let Ok(home) = net.home_realm().await {
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

                        // Pre-compute contact invite code (async, includes transport info)
                        log_event(&mut state, EventDirection::System, "Generating invite code...");
                        match net.contact_invite_code().await {
                            Ok(code) => {
                                state.write().invite_code_uri = Some(code.to_uri());
                                log_event(&mut state, EventDirection::System, "Invite code ready");
                            }
                            Err(e) => {
                                tracing::warn!("Failed to generate invite code: {}", e);
                                log_event(&mut state, EventDirection::System, format!("Invite code: {}", e));
                            }
                        }

                        // Process handshake inbox (picks up connection requests from others)
                        log_event(&mut state, EventDirection::Received, "Checking inbox for connection requests...");
                        match net.process_handshake_inbox().await {
                            Ok(count) => {
                                if count > 0 {
                                    log_event(&mut state, EventDirection::Received, format!("Inbox: processed {} connection request(s)", count));
                                } else {
                                    log_event(&mut state, EventDirection::System, "Inbox: no pending requests");
                                }
                            }
                            Err(e) => {
                                tracing::debug!("Handshake inbox processing: {}", e);
                                log_event(&mut state, EventDirection::System, format!("Inbox check: {}", e));
                            }
                        }

                        // Load contacts (use async read to avoid blocking in async context)
                        log_event(&mut state, EventDirection::System, "Loading contacts...");
                        if let Some(contacts_realm) = net.contacts_realm().await {
                            if let Ok(doc) = contacts_realm.contacts().await {
                                let data = doc.read().await;
                                let contacts: Vec<ContactView> = data.contacts.iter().map(|(mid, entry)| {
                                    ContactView {
                                        member_id_short: mid.iter().take(8).map(|b| format!("{:02x}", b)).collect(),
                                        display_name: entry.display_name.clone(),
                                        status: "confirmed".to_string(),
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

    // Close handler: sync is_open signal back to state
    use_effect(move || {
        if !ci_open() && state.read().contact_invite_open {
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
                                tracing::info!(uri = %uri, "on_connect: parsing invite URI");
                                match ContactInviteCode::parse(&uri) {
                                    Ok(code) => {
                                        tracing::info!("on_connect: parsed invite code OK");
                                        state.write().contact_connecting = true;
                                        let inviter_name = code.display_name().map(|s| s.to_string()).unwrap_or_else(|| "unknown".to_string());
                                        log_event(&mut state, EventDirection::System, format!("Parsed invite from {}", inviter_name));
                                        // Clone the Arc to avoid holding Signal read guard across awaits
                                        let net = {
                                            let guard = network.read();
                                            guard.as_ref().cloned()
                                        };
                                        if let Some(net) = net {
                                            tracing::info!("on_connect: calling accept_contact_invite");
                                            log_event(&mut state, EventDirection::Sent, format!("Accepting invite from {}...", inviter_name));
                                            match net.accept_contact_invite(&code).await {
                                                Ok(()) => {
                                                    tracing::info!("on_connect: accept_contact_invite succeeded, processing handshake inbox");
                                                    log_event(&mut state, EventDirection::Sent, format!("Connection request sent to {}", inviter_name));
                                                    // Process handshake inbox (in case the inviter already connected)
                                                    match net.process_handshake_inbox().await {
                                                        Ok(count) => {
                                                            if count > 0 {
                                                                log_event(&mut state, EventDirection::Received, format!("Inbox: processed {} request(s)", count));
                                                            }
                                                        }
                                                        Err(e) => {
                                                            tracing::debug!("on_connect: handshake inbox: {}", e);
                                                        }
                                                    }
                                                    // Reload contacts
                                                    match net.contacts_realm().await {
                                                        Some(contacts_realm) => {
                                                            tracing::info!("on_connect: got contacts realm, reading list");
                                                            if let Ok(doc) = contacts_realm.contacts().await {
                                                                let data = doc.read().await;
                                                                tracing::info!(count = data.contacts.len(), "on_connect: loaded contacts");
                                                                let contacts: Vec<ContactView> = data.contacts.iter().map(|(mid, entry)| {
                                                                    ContactView {
                                                                        member_id_short: mid.iter().take(8).map(|b| format!("{:02x}", b)).collect(),
                                                                        display_name: entry.display_name.clone(),
                                                                        status: "confirmed".to_string(),
                                                                    }
                                                                }).collect();
                                                                let count = contacts.len();
                                                                drop(data);
                                                                state.write().contacts = contacts;
                                                                log_event(&mut state, EventDirection::System, format!("Contacts: {} connection(s)", count));
                                                            }
                                                        }
                                                        None => {
                                                            tracing::warn!("on_connect: contacts_realm() returned None after accept");
                                                        }
                                                    }
                                                    tracing::info!("on_connect: closing overlay");
                                                    let mut s = state.write();
                                                    s.contact_invite_input.clear();
                                                    s.contact_parsed_name = None;
                                                    s.contact_invite_status = None;
                                                    s.contact_connecting = false;
                                                    s.contact_invite_open = false;
                                                    tracing::info!("on_connect: done");
                                                }
                                                Err(e) => {
                                                    tracing::error!(error = %e, "on_connect: accept_contact_invite failed");
                                                    log_event(&mut state, EventDirection::System, format!("ERROR: Accept failed: {}", e));
                                                    let mut s = state.write();
                                                    s.contact_invite_status = Some(format!("error:{}", e));
                                                    s.contact_connecting = false;
                                                }
                                            }
                                        } else {
                                            tracing::error!("on_connect: network is None");
                                        }
                                    }
                                    Err(e) => {
                                        log_event(&mut state, EventDirection::System, format!("ERROR: Invalid invite: {}", e));
                                        let mut s = state.write();
                                        s.contact_invite_status = Some(format!("error:Invalid invite: {}", e));
                                        s.contact_connecting = false;
                                    }
                                }
                            });
                        },
                        on_parse_input: move |input: String| {
                            state.write().contact_invite_input = input.clone();
                            match ContactInviteCode::parse(&input) {
                                Ok(code) => {
                                    state.write().contact_parsed_name = code.display_name().map(|s| s.to_string());
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

            // Load home realm - quests and notes
            log_event(state, EventDirection::System, "Joining home realm...");
            if let Ok(home) = net.home_realm().await {
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

            // Pre-compute contact invite code (async, includes transport info)
            log_event(state, EventDirection::System, "Generating invite code...");
            match net.contact_invite_code().await {
                Ok(code) => {
                    state.write().invite_code_uri = Some(code.to_uri());
                    log_event(state, EventDirection::System, "Invite code ready");
                }
                Err(e) => {
                    tracing::warn!("Failed to generate invite code: {}", e);
                    log_event(state, EventDirection::System, format!("Invite code: {}", e));
                }
            }

            // Process handshake inbox (picks up connection requests from others)
            log_event(state, EventDirection::Received, "Checking inbox...");
            match net.process_handshake_inbox().await {
                Ok(count) => {
                    if count > 0 {
                        log_event(state, EventDirection::Received, format!("Inbox: processed {} connection request(s)", count));
                    } else {
                        log_event(state, EventDirection::System, "Inbox: no pending requests");
                    }
                }
                Err(e) => {
                    tracing::debug!("Handshake inbox processing: {}", e);
                    log_event(state, EventDirection::System, format!("Inbox check: {}", e));
                }
            }

            // Load contacts
            log_event(state, EventDirection::System, "Loading contacts...");
            if let Some(contacts_realm) = net.contacts_realm().await {
                if let Ok(doc) = contacts_realm.contacts().await {
                    let data = doc.read().await;
                    let contacts: Vec<ContactView> = data.contacts.iter().map(|(mid, entry)| {
                        ContactView {
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
