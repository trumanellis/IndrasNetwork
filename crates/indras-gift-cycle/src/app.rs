//! Root component: boot sequence → main UI with polling loop.

use std::pin::Pin;
use std::sync::Arc;

use dioxus::prelude::*;

use indras_network::IndrasNetwork;

use futures::{Stream, StreamExt};

use crate::bridge::GiftCycleBridge;
use crate::components::blessing_view::BlessingView;
use crate::components::contact_invite::ContactInviteOverlay;
use crate::components::cycle_ring::CycleRing;
use crate::components::event_log::EventLogPanel;
use crate::components::intention_detail::IntentionDetail;
use crate::components::intention_feed::IntentionFeed;
use crate::components::intention_form::IntentionForm;
use crate::components::peer_bar::PeerBar;
use crate::components::proof_submit::ProofSubmit;
use crate::components::token_wallet::TokenWallet;
use crate::data::{self, IntentionCardData, IntentionViewData, P2pLogEntry, PeerDisplayInfo, TokenCardData};
use indras_sync_engine::IntentionId;
use crate::state::{AppView, CycleStage};

// ================================================================
// Network helpers (same as indras-workspace)
// ================================================================

fn default_data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("INDRAS_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library/Application Support/indras-network");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return std::path::PathBuf::from(xdg).join("indras-network");
        }
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(".local/share/indras-network");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return std::path::PathBuf::from(appdata).join("indras-network");
        }
    }
    std::path::PathBuf::from(".").join("indras-network")
}

fn is_first_run() -> bool {
    IndrasNetwork::is_first_run(default_data_dir())
}

async fn create_identity(name: &str) -> Result<Arc<IndrasNetwork>, String> {
    let data_dir = default_data_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let net = IndrasNetwork::builder()
        .data_dir(&data_dir)
        .display_name(name)
        .build()
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(net)
}

async fn load_identity() -> Result<Arc<IndrasNetwork>, String> {
    let data_dir = default_data_dir();
    let net = IndrasNetwork::new(&data_dir)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(net)
}

// ================================================================
// Boot helper
// ================================================================

async fn boot_network(
    net: Arc<IndrasNetwork>,
    bridge: &mut Signal<Option<GiftCycleBridge>>,
    boot_error: &mut Signal<Option<String>>,
) {
    let player_name = net.display_name().unwrap_or_else(|| "Unknown".to_string());
    let player_id = net.id();
    tracing::info!("Identity loaded: {}", player_name);
    if let Err(e) = net.start().await {
        tracing::warn!(error = %e, "Failed to start network (non-fatal)");
    }
    if let Err(e) = net.join_contacts_realm().await {
        tracing::warn!(error = %e, "Failed to join contacts realm (non-fatal)");
    }
    let home = match net.home_realm().await {
        Ok(hr) => hr,
        Err(e) => {
            boot_error.set(Some(format!("Failed to init home realm: {e}")));
            return;
        }
    };
    {
        use indras_sync_engine::HomeRealmIntentions;
        if let Err(e) = home.seed_welcome_intention_if_empty().await {
            tracing::warn!(error = %e, "Failed to seed welcome intention");
        }
    }
    let b = GiftCycleBridge::new(home, player_id, player_name, Arc::clone(&net));
    bridge.set(Some(b));
}

// ================================================================
// Root component
// ================================================================

/// The root Gift Cycle application component.
#[component]
pub fn GiftCycleApp() -> Element {
    // Boot state
    let mut boot_error = use_signal(|| None::<String>);
    let mut bridge = use_signal(|| None::<GiftCycleBridge>);

    // Navigation state
    let mut current_view = use_signal(|| AppView::Feed);
    let mut current_stage = use_signal(|| CycleStage::Intention);

    // Data state — refreshed by polling
    let mut my_cards = use_signal(Vec::<IntentionCardData>::new);
    let mut all_cards = use_signal(Vec::<IntentionCardData>::new);
    let mut token_cards = use_signal(Vec::<TokenCardData>::new);
    let mut detail_data = use_signal(|| None::<IntentionViewData>);
    let mut peers = use_signal(Vec::<PeerDisplayInfo>::new);
    let mut p2p_log = use_signal(Vec::<P2pLogEntry>::new);
    let mut contact_invite_open = use_signal(|| false);
    let mut needs_onboarding = use_signal(|| false);
    let mut onboard_name = use_signal(String::new);

    // Boot sequence — runs once
    let _boot = use_resource(move || async move {
        let net = if is_first_run() {
            if let Ok(auto_name) = std::env::var("INDRAS_NAME") {
                match create_identity(&auto_name).await {
                    Ok(n) => n,
                    Err(e) => {
                        boot_error.set(Some(format!("Failed to create identity: {e}")));
                        return;
                    }
                }
            } else {
                needs_onboarding.set(true);
                return;
            }
        } else {
            match load_identity().await {
                Ok(n) => n,
                Err(e) => {
                    boot_error.set(Some(format!("Failed to load identity: {e}")));
                    return;
                }
            }
        };

        boot_network(net, &mut bridge, &mut boot_error).await;
    });

    // SystemEvent stream — push P2P events from all realms into the log.
    // Re-subscribes every 10s so newly-created DM realms get picked up.
    let _sys_events = use_resource(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let Some(b) = bridge.read().clone() else {
                continue;
            };

            // Collect realms (home + all DM realms), then build streams
            let mut realms = Vec::new();
            if let Some(r) = b.network.get_realm_by_id(&b.home.id()) {
                realms.push(r);
            }
            for rid in b.network.conversation_realms() {
                if let Some(r) = b.network.get_realm_by_id(&rid) {
                    realms.push(r);
                }
            }

            let streams: Vec<Pin<Box<dyn Stream<Item = indras_network::SystemEvent> + Send>>> =
                realms.iter().map(|r| Box::pin(r.system_events()) as Pin<Box<dyn Stream<Item = _> + Send>>).collect();

            let mut merged = futures::stream::select_all(streams);

            // Listen for up to 10s, then loop back to re-subscribe (catches new DM realms)
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                match tokio::time::timeout_at(deadline, merged.next()).await {
                    Ok(Some(event)) => {
                        let entry = P2pLogEntry {
                            timestamp: event.timestamp(),
                            message: event.display_text(),
                        };
                        let mut log = p2p_log.write();
                        log.push(entry);
                        if log.len() > 50 {
                            log.remove(0);
                        }
                    }
                    Ok(None) => break,    // all streams ended
                    Err(_) => break,      // timeout — re-subscribe
                }
            }
        }
    });

    // Network peer events — surface connection/peering activity in the P2P log.
    // Unlike system_events() which are per-realm, these are network-level:
    // new contacts, conversation openings, disconnections, etc.
    let _peer_events = use_resource(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let Some(b) = bridge.read().clone() else {
                continue;
            };

            let mut rx = b.network.peer_events();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let message = match &event {
                            indras_network::PeerEvent::PeerConnected { peer } => {
                                Some(format!("{} connected", peer.display_name))
                            }
                            indras_network::PeerEvent::PeerDisconnected { member_id } => {
                                let short: String = member_id.iter().take(4).map(|b| format!("{b:02x}")).collect();
                                Some(format!("{short}… disconnected"))
                            }
                            indras_network::PeerEvent::ConversationOpened { peer, .. } => {
                                Some(format!("DM opened with {}", peer.display_name))
                            }
                            indras_network::PeerEvent::PeerBlocked { member_id, .. } => {
                                let short: String = member_id.iter().take(4).map(|b| format!("{b:02x}")).collect();
                                Some(format!("{short}… blocked"))
                            }
                            indras_network::PeerEvent::SentimentChanged { member_id, sentiment } => {
                                let short: String = member_id.iter().take(4).map(|b| format!("{b:02x}")).collect();
                                let label = match sentiment {
                                    1 => "recommended",
                                    -1 => "not recommended",
                                    _ => "neutral",
                                };
                                Some(format!("{short}… marked as {label}"))
                            }
                            _ => None, // PeersChanged, WorldViewSaved, NetworkEvent, Warning — skip
                        };
                        if let Some(msg) = message {
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            let mut log = p2p_log.write();
                            log.push(P2pLogEntry {
                                timestamp: now_ms,
                                message: msg,
                            });
                            if log.len() > 50 {
                                log.remove(0);
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    });

    // Polling loop — refreshes data every 2 seconds
    let _poll = use_resource(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let Some(b) = bridge.read().clone() else {
                continue;
            };

            // Refresh feed cards
            let cards =
                data::build_intention_cards(&b.home, b.member_id, &b.player_name).await;
            my_cards.set(cards.clone());

            // Merge home + community into unified feed
            let mut merged = cards;
            let comm =
                data::build_community_intention_cards(&b.network, b.member_id, &b.player_name)
                    .await;
            merged.extend(comm);
            all_cards.set(merged);

            // Refresh tokens
            let tokens = data::build_member_tokens(&b.home, b.member_id, &b.player_name).await;
            token_cards.set(tokens);

            // Refresh peers from contacts realm (matches workspace poll_contacts)
            if let Some(contacts_realm) = b.network.contacts_realm().await {
                if let Ok(doc) = contacts_realm.contacts().await {
                    let _ = doc.refresh().await;  // Ensure CRDT state is current
                    let contacts_data = doc.read().await;
                    let current_count = peers.read().len();
                    if contacts_data.contacts.len() != current_count {
                        let peer_infos: Vec<PeerDisplayInfo> = contacts_data
                            .contacts
                            .iter()
                            .enumerate()
                            .map(|(i, (mid, entry))| {
                                let name = entry.display_name.clone().unwrap_or_else(|| {
                                    mid.iter().take(4).map(|b| format!("{b:02x}")).collect()
                                });
                                let letter = name.chars().next().unwrap_or('?').to_string();
                                let color_class = data::PEER_COLORS[i % data::PEER_COLORS.len()].to_string();
                                PeerDisplayInfo {
                                    name,
                                    letter,
                                    color_class,
                                    online: true,
                                    member_id: *mid,
                                }
                            })
                            .collect();
                        peers.set(peer_infos);
                    }
                }
            }

            // Fallback: scan DM realms for peers not yet in contacts
            let known_peers: std::collections::HashSet<_> = peers
                .read()
                .iter()
                .map(|p| p.name.clone())
                .collect();
            for rid in b.network.conversation_realms() {
                if let Some(peer_mid) = b.network.dm_peer_for_realm(&rid) {
                    let name: String = peer_mid
                        .iter()
                        .take(4)
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    if !known_peers.contains(&name) {
                        if let Some(contacts_realm) = b.network.contacts_realm().await {
                            if !contacts_realm.is_contact(&peer_mid).await {
                                let _ = contacts_realm.add_contact(peer_mid).await;
                            }
                        }
                    }
                }
            }

            // Proactive: connect to all known contacts to trigger re-notification
            // Since connect() early-return now re-notifies, this ensures mutual discovery
            for peer_info in peers.read().iter() {
                let _ = b.network.connect(peer_info.member_id).await;
            }

            // Refresh detail view if showing one
            let view = current_view.read().clone();
            if let AppView::Detail(id) = view {
                let detail =
                    data::build_intention_view(&b.home, id, b.member_id, &b.player_name).await;
                detail_data.set(detail);
            }
        }
    });

    // Render
    let has_bridge = bridge.read().is_some();
    let err = boot_error.read().clone();

    if let Some(err_msg) = err {
        return rsx! {
            div { class: "boot-error",
                div { class: "boot-error-title", "Boot Error" }
                div { class: "boot-error-msg", "{err_msg}" }
            }
        };
    }

    if needs_onboarding() {
        return rsx! {
            div { class: "onboarding",
                div { class: "onboarding-card",
                    h1 { "Welcome to the Gift Cycle" }
                    p { "Choose a name to get started." }
                    input {
                        class: "gc-input",
                        r#type: "text",
                        placeholder: "Your name",
                        value: "{onboard_name}",
                        oninput: move |e| onboard_name.set(e.value()),
                    }
                    button {
                        class: "gc-btn gc-btn-primary",
                        disabled: onboard_name.read().trim().is_empty(),
                        onclick: move |_| {
                            let name = onboard_name.read().trim().to_string();
                            needs_onboarding.set(false);
                            spawn(async move {
                                match create_identity(&name).await {
                                    Ok(net) => {
                                        boot_network(net, &mut bridge, &mut boot_error).await;
                                    }
                                    Err(e) => {
                                        boot_error.set(Some(e));
                                    }
                                }
                            });
                        },
                        "Start"
                    }
                }
            }
        };
    }

    if !has_bridge {
        return rsx! {
            div { class: "boot-loading",
                div { class: "boot-spinner" }
                div { class: "boot-loading-text", "Initializing Gift Cycle..." }
            }
        };
    }

    let b = bridge.read().clone().unwrap();
    let player_name = b.player_name.clone();
    let member_id = b.member_id;

    rsx! {
        div { class: "gift-cycle-app",
            PeerBar {
                player_name: player_name.clone(),
                member_id,
                peers: peers(),
                on_add_contact: move |_| contact_invite_open.set(true),
            }

            ContactInviteOverlay {
                bridge: bridge().unwrap(),
                is_open: contact_invite_open,
            }

            CycleRing {
                active_stage: current_stage(),
                on_stage_click: move |stage: CycleStage| {
                    current_stage.set(stage.clone());
                    match stage {
                        CycleStage::Intention => current_view.set(AppView::Feed),
                        CycleStage::Attention => current_view.set(AppView::Feed),
                        CycleStage::Service => current_view.set(AppView::Feed),
                        CycleStage::Blessing => current_view.set(AppView::Feed),
                        CycleStage::Token => current_view.set(AppView::Wallet),
                        CycleStage::Renewal => current_view.set(AppView::CreateIntention),
                    }
                },
            }

            div { class: "content-area",
                    match current_view() {
                        AppView::Feed => rsx! {
                            IntentionFeed {
                                cards: all_cards(),
                                on_select: move |id| {
                                    current_view.set(AppView::Detail(id));
                                    current_stage.set(CycleStage::Attention);
                                },
                                on_create: move |_| {
                                    current_view.set(AppView::CreateIntention);
                                    current_stage.set(CycleStage::Intention);
                                },
                            }
                        },
                        AppView::Detail(id) => rsx! {
                            IntentionDetail {
                                view_data: detail_data(),
                                intention_id: id,
                                is_creator: detail_data().map(|d| d.creator == member_id).unwrap_or(false),
                                bridge: bridge().unwrap(),
                                on_back: move |_| {
                                    current_view.set(AppView::Feed);
                                    current_stage.set(CycleStage::Intention);
                                },
                                on_submit_proof: move |id| {
                                    current_view.set(AppView::SubmitProof(id));
                                    current_stage.set(CycleStage::Service);
                                },
                                on_bless: move |(id, claimant)| {
                                    current_view.set(AppView::Bless(id, claimant));
                                    current_stage.set(CycleStage::Blessing);
                                },
                            }
                        },
                        AppView::CreateIntention => rsx! {
                            IntentionForm {
                                bridge: bridge().unwrap(),
                                available_tokens: token_cards(),
                                connected_peers: bridge().unwrap().connected_peers(),
                                on_created: move |id: IntentionId| {
                                    let short: String = id.iter().take(4).map(|b| format!("{b:02x}")).collect();
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64;
                                    let mut log = p2p_log.write();
                                    log.push(P2pLogEntry {
                                        timestamp: now_ms,
                                        message: format!("Intention created ({short}…)"),
                                    });
                                    if log.len() > 50 { log.remove(0); }
                                    drop(log);
                                    current_view.set(AppView::Detail(id));
                                    current_stage.set(CycleStage::Attention);
                                },
                                on_cancel: move |_| {
                                    current_view.set(AppView::Feed);
                                },
                            }
                        },
                        AppView::SubmitProof(id) => rsx! {
                            ProofSubmit {
                                intention_id: id,
                                bridge: bridge().unwrap(),
                                on_submitted: move |_| {
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64;
                                    let mut log = p2p_log.write();
                                    log.push(P2pLogEntry {
                                        timestamp: now_ms,
                                        message: "Proof submitted".to_string(),
                                    });
                                    if log.len() > 50 { log.remove(0); }
                                    drop(log);
                                    current_view.set(AppView::Detail(id));
                                    current_stage.set(CycleStage::Service);
                                },
                                on_cancel: move |_| {
                                    current_view.set(AppView::Detail(id));
                                },
                            }
                        },
                        AppView::Bless(id, claimant) => rsx! {
                            BlessingView {
                                intention_id: id,
                                claimant,
                                view_data: detail_data(),
                                bridge: bridge().unwrap(),
                                on_blessed: move |_| {
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64;
                                    let mut log = p2p_log.write();
                                    log.push(P2pLogEntry {
                                        timestamp: now_ms,
                                        message: "Blessing bestowed".to_string(),
                                    });
                                    if log.len() > 50 { log.remove(0); }
                                    drop(log);
                                    current_view.set(AppView::Detail(id));
                                    current_stage.set(CycleStage::Token);
                                },
                                on_cancel: move |_| {
                                    current_view.set(AppView::Detail(id));
                                },
                            }
                        },
                        AppView::Wallet => rsx! {
                            TokenWallet {
                                tokens: token_cards(),
                                bridge: bridge().unwrap(),
                                my_cards: my_cards(),
                                on_pledge: move |_| {},
                                on_back: move |_| {
                                    current_view.set(AppView::Feed);
                                    current_stage.set(CycleStage::Intention);
                                },
                            }
                        },
                    }
                }

            EventLogPanel { entries: p2p_log() }
        }
    }
}
