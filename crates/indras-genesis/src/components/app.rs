//! Root application component with state machine routing.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::{ContactInviteCode, IndrasNetwork};

use crate::state::{AsyncStatus, GenesisState, GenesisStep, NoteView, QuestView};
use indras_ui::{ContactInviteOverlay, ThemedRoot};

use super::display_name::DisplayNameScreen;
use super::home_realm::HomeRealmScreen;
use super::pass_story_flow::PassStoryFlow;
use super::welcome::WelcomeScreen;

/// Get the default data directory for Indras Network.
pub fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Application Support/indras-network");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
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

/// Root application component.
#[component]
pub fn App() -> Element {
    let mut state = use_signal(GenesisState::new);
    let mut network: Signal<Option<Arc<IndrasNetwork>>> = use_signal(|| None);

    // On mount: check if returning user
    use_effect(move || {
        spawn(async move {
            let data_dir = default_data_dir();

            if !IndrasNetwork::is_first_run(&data_dir) {
                // Returning user: load existing network
                tracing::info!("Returning user detected, loading network");
                state.write().status = AsyncStatus::Loading;

                match IndrasNetwork::new(&data_dir).await {
                    Ok(net) => {
                        let id = net.id();
                        let id_short = id.iter()
                            .take(8)
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        let name = net.display_name().unwrap_or("").to_string();

                        let net = Arc::new(net);

                        // Load home realm
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
                        state.write().status = AsyncStatus::Error(e.to_string());
                        // Fall through to genesis flow
                    }
                }
            }
        });
    });

    let current_step = state.read().step.clone();
    let pass_story_active = state.read().pass_story_active;
    let contact_invite_open = state.read().contact_invite_open;

    // Contact invite signals
    let invite_uri = use_memo(move || {
        let net = network.read();
        match net.as_ref() {
            Some(n) => n.contact_invite_code().to_uri(),
            None => String::new(),
        }
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
                                match ContactInviteCode::parse(&uri) {
                                    Ok(code) => {
                                        let net = network.read();
                                        if let Some(ref net) = *net {
                                            match net.accept_contact_invite(&code).await {
                                                Ok(()) => {
                                                    let name = code.display_name().unwrap_or("unknown").to_string();
                                                    let mut s = state.write();
                                                    s.contact_invite_status = Some(format!("success:Connected with {}", name));
                                                    s.contact_invite_input.clear();
                                                    s.contact_parsed_name = None;
                                                }
                                                Err(e) => {
                                                    state.write().contact_invite_status = Some(format!("error:{}", e));
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        state.write().contact_invite_status = Some(format!("error:Invalid invite: {}", e));
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

            let net = Arc::new(net);

            // Load home realm - quests and notes
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
            state.write().status = AsyncStatus::Error(format!("Failed to create identity: {}", e));
        }
    }
}
