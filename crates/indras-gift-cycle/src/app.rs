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
use crate::components::profile_grants::ProfileGrantsPanel;
use crate::components::proof_submit::ProofSubmit;
use crate::components::token_wallet::TokenWallet;
use crate::data::{self, IntentionCardData, IntentionViewData, P2pLogEntry, PeerDisplayInfo, ProfileFieldVisibility, TokenCardData};
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
    homepage_store_sig: &mut Signal<Option<Arc<dyn indras_homepage::HomepageStore>>>,
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
    // Start homepage server
    let homepage_port: u16 = std::env::var("INDRAS_HOMEPAGE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);

    // Create BlobStore-backed persistent store for homepage data
    let data_dir = default_data_dir();
    let homepage_store: Option<Arc<dyn indras_homepage::HomepageStore>> =
        match indras_relay::blob_store::BlobStore::open(&data_dir.join("homepage-events.redb")) {
            Ok(bs) => Some(Arc::new(
                indras_relay::blob_homepage::BlobStoreHomepageStore::new(
                    std::sync::Arc::new(bs),
                    &player_id,
                ),
            ) as Arc<dyn indras_homepage::HomepageStore>),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create homepage blob store (non-fatal)");
                None
            }
        };

    let mut server = indras_homepage::HomepageServer::new(player_id);
    if let Some(ref store) = homepage_store {
        server = server.with_store(Arc::clone(store));
    }
    let fields_handle = server.fields_handle();
    let artifacts_handle = server.artifacts_handle();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], homepage_port));
    tokio::spawn(async move {
        if let Err(e) = server.serve(addr).await {
            tracing::error!(error = %e, "Homepage server failed");
        }
    });
    tracing::info!(port = homepage_port, "Homepage server started at http://localhost:{}", homepage_port);

    // Create/update ProfileIdentityDocument
    let public_key_hex: String = player_id.iter().map(|b| format!("{b:02x}")).collect();
    let username = player_name.to_lowercase().replace(' ', "-");
    if let Ok(doc) = home.document::<indras_sync_engine::ProfileIdentityDocument>("_profile_identity").await {
        let pname = player_name.clone();
        let uname = username.clone();
        let pk = public_key_hex.clone();
        let _ = doc.update(move |d| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            // Only update if our data is newer or doc is empty
            if d.display_name.is_empty() || d.updated_at == 0 {
                d.display_name = pname;
                d.username = uname;
                d.public_key = pk;
                d.updated_at = now;
            }
        }).await;
    }

    // Register each profile field as an artifact with default Public grant
    let all_field_names = [
        indras_homepage::fields::DISPLAY_NAME,
        indras_homepage::fields::USERNAME,
        indras_homepage::fields::BIO,
        indras_homepage::fields::PUBLIC_KEY,
        indras_homepage::fields::INTENTION_COUNT,
        indras_homepage::fields::TOKEN_COUNT,
        indras_homepage::fields::BLESSINGS_GIVEN,
        indras_homepage::fields::ATTENTION_CONTRIBUTED,
        indras_homepage::fields::CONTACT_COUNT,
        indras_homepage::fields::HUMANNESS_FRESHNESS,
        indras_homepage::fields::ACTIVE_QUESTS,
        indras_homepage::fields::ACTIVE_OFFERINGS,
    ];
    if let Ok(doc) = home.artifact_index().await {
        let _ = doc
            .update(|index| {
                for field_name in &all_field_names {
                    let field_id = indras_homepage::profile_field_artifact_id(&player_id, field_name);
                    let aid = indras_artifacts::ArtifactId::Doc(field_id);
                    if index.get(&aid).is_none() {
                        let entry = indras_network::artifact_index::HomeArtifactEntry {
                            id: aid,
                            name: format!("profile:{field_name}"),
                            mime_type: Some("application/x-indras-profile-field".to_string()),
                            size: 0,
                            created_at: 0,
                            encrypted_key: None,
                            status: indras_artifacts::ArtifactStatus::Active,
                            grants: vec![indras_artifacts::AccessGrant {
                                grantee: [0u8; 32],
                                mode: indras_artifacts::AccessMode::Public,
                                granted_at: 0,
                                granted_by: player_id,
                            }],
                            provenance: None,
                            location: None,
                        };
                        index.store(entry);
                    }
                }
            })
            .await;
    }

    // Pre-populate homepage from CRDT (faster than waiting for first poll)
    if let Ok(doc) = home.document::<indras_sync_engine::HomepageProfileDocument>("_homepage_profile").await {
        let data = doc.read().await;
        if !data.fields.is_empty() {
            let artifacts: Vec<indras_homepage::ProfileFieldArtifact> = data.fields.iter().map(|f| {
                indras_homepage::ProfileFieldArtifact {
                    field_name: f.name.clone(),
                    display_value: f.value.clone(),
                    grants: serde_json::from_str(&f.grants_json).unwrap_or_default(),
                }
            }).collect();
            *fields_handle.write().await = artifacts;
            tracing::info!("Pre-populated homepage from CRDT ({} fields)", data.fields.len());
        }
    }

    // Expose homepage store to polling loop via signal
    if let Some(store) = homepage_store {
        homepage_store_sig.set(Some(store));
    }

    let b = GiftCycleBridge::new(home, player_id, player_name, Arc::clone(&net))
        .with_homepage_fields(fields_handle)
        .with_homepage_artifacts(artifacts_handle);
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
    let mut homepage_store_sig = use_signal(|| None::<Arc<dyn indras_homepage::HomepageStore>>);
    // Relay is built into IndrasNetwork — no separate signals needed

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
    let mut profile_fields = use_signal(Vec::<ProfileFieldVisibility>::new);
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

        boot_network(net, &mut bridge, &mut boot_error, &mut homepage_store_sig).await;
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
                        // Skip noisy CRDT sync events — they fire every 5s per realm
                        if matches!(event, indras_network::SystemEvent::DocumentSynced { .. }) {
                            continue;
                        }
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

            // Build peer name lookup from known contacts
            let peer_names: std::collections::HashMap<_, _> = peers
                .read()
                .iter()
                .map(|p| (p.member_id, p.name.clone()))
                .collect();

            // Refresh feed cards
            let cards =
                data::build_intention_cards(&b.home, &b.network, b.member_id, &b.player_name, &peer_names).await;
            my_cards.set(cards.clone());

            // Merge home + community into unified feed
            let mut merged = cards;
            let comm =
                data::build_community_intention_cards(&b.network, b.member_id, &b.player_name, &peer_names)
                    .await;
            merged.extend(comm);
            all_cards.set(merged);

            // Refresh tokens
            let tokens = data::build_member_tokens(&b.home, b.member_id, &b.player_name, &peer_names).await;
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

            // Sync contacts to embedded relay for tier determination
            if let Some(auth) = b.network.relay_auth() {
                let contact_ids: Vec<[u8; 32]> = peers.read().iter().map(|p| p.member_id).collect();
                auth.sync_contacts(contact_ids);
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

            // Refresh homepage fields with live stats
            if let Some(ref fields_handle) = b.homepage_fields {
                let mut field_artifacts = Vec::new();

                // Read grants snapshot from artifact index (once)
                let default_public_grant = vec![indras_artifacts::AccessGrant {
                    grantee: [0u8; 32],
                    mode: indras_artifacts::AccessMode::Public,
                    granted_at: 0,
                    granted_by: b.member_id,
                }];
                let mut grants_map: std::collections::HashMap<String, Vec<indras_artifacts::AccessGrant>> =
                    std::collections::HashMap::new();
                let artifact_index = b.home.artifact_index().await.ok();
                if let Some(ref artifact_doc) = artifact_index {
                    let index = artifact_doc.read().await;
                    for field_name in &[
                        indras_homepage::fields::DISPLAY_NAME, indras_homepage::fields::USERNAME,
                        indras_homepage::fields::BIO, indras_homepage::fields::PUBLIC_KEY,
                        indras_homepage::fields::INTENTION_COUNT, indras_homepage::fields::TOKEN_COUNT,
                        indras_homepage::fields::BLESSINGS_GIVEN, indras_homepage::fields::ATTENTION_CONTRIBUTED,
                        indras_homepage::fields::CONTACT_COUNT, indras_homepage::fields::HUMANNESS_FRESHNESS,
                        indras_homepage::fields::ACTIVE_QUESTS, indras_homepage::fields::ACTIVE_OFFERINGS,
                    ] {
                        let aid = indras_artifacts::ArtifactId::Doc(
                            indras_homepage::profile_field_artifact_id(&b.member_id, field_name),
                        );
                        if let Some(entry) = index.get(&aid) {
                            grants_map.insert(field_name.to_string(), entry.grants.clone());
                        }
                    }
                }
                let field_grants = |field_name: &str| -> Vec<indras_artifacts::AccessGrant> {
                    grants_map.get(field_name).cloned().unwrap_or_else(|| default_public_grant.clone())
                };

                // Read identity from ProfileIdentityDocument
                if let Ok(doc) = b.home.document::<indras_sync_engine::ProfileIdentityDocument>("_profile_identity").await {
                    let identity = doc.read().await;
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::DISPLAY_NAME.to_string(),
                        display_value: identity.display_name.clone(),
                        grants: field_grants(indras_homepage::fields::DISPLAY_NAME),
                    });
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::USERNAME.to_string(),
                        display_value: identity.username.clone(),
                        grants: field_grants(indras_homepage::fields::USERNAME),
                    });
                    if let Some(ref bio) = identity.bio {
                        field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                            field_name: indras_homepage::fields::BIO.to_string(),
                            display_value: bio.clone(),
                            grants: field_grants(indras_homepage::fields::BIO),
                        });
                    }
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::PUBLIC_KEY.to_string(),
                        display_value: identity.public_key.clone(),
                        grants: field_grants(indras_homepage::fields::PUBLIC_KEY),
                    });
                }

                // Intention count + active quests/offerings
                if let Ok(doc) = b.home.document::<indras_sync_engine::IntentionDocument>("intentions").await {
                    let intentions = doc.read().await;
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::INTENTION_COUNT.to_string(),
                        display_value: intentions.intentions.len().to_string(),
                        grants: field_grants(indras_homepage::fields::INTENTION_COUNT),
                    });

                    let quests: Vec<indras_homepage::IntentionSummary> = intentions.intentions.iter()
                        .filter(|i| matches!(i.kind, indras_sync_engine::IntentionKind::Quest) && i.completed_at_millis.is_none() && !i.deleted)
                        .map(|i| indras_homepage::IntentionSummary {
                            title: i.title.clone(),
                            kind: format!("{:?}", i.kind),
                            status: "active".to_string(),
                        })
                        .collect();
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::ACTIVE_QUESTS.to_string(),
                        display_value: serde_json::to_string(&quests).unwrap_or_default(),
                        grants: field_grants(indras_homepage::fields::ACTIVE_QUESTS),
                    });

                    let offerings: Vec<indras_homepage::IntentionSummary> = intentions.intentions.iter()
                        .filter(|i| matches!(i.kind, indras_sync_engine::IntentionKind::Offering) && i.completed_at_millis.is_none() && !i.deleted)
                        .map(|i| indras_homepage::IntentionSummary {
                            title: i.title.clone(),
                            kind: format!("{:?}", i.kind),
                            status: "active".to_string(),
                        })
                        .collect();
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::ACTIVE_OFFERINGS.to_string(),
                        display_value: serde_json::to_string(&offerings).unwrap_or_default(),
                        grants: field_grants(indras_homepage::fields::ACTIVE_OFFERINGS),
                    });
                }

                // Token count
                if let Ok(doc) = b.home.document::<indras_sync_engine::TokenOfGratitudeDocument>("_tokens").await {
                    let tokens = doc.read().await;
                    let count = tokens.tokens_for_steward(&b.member_id).len();
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::TOKEN_COUNT.to_string(),
                        display_value: count.to_string(),
                        grants: field_grants(indras_homepage::fields::TOKEN_COUNT),
                    });
                }

                // Blessings given
                if let Ok(doc) = b.home.document::<indras_sync_engine::BlessingDocument>("blessings").await {
                    let blessings = doc.read().await;
                    let count = blessings.blessings_by_member(&b.member_id).len();
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::BLESSINGS_GIVEN.to_string(),
                        display_value: count.to_string(),
                        grants: field_grants(indras_homepage::fields::BLESSINGS_GIVEN),
                    });
                }

                // Attention contributed
                if let Ok(doc) = b.home.document::<indras_sync_engine::AttentionDocument>("attention").await {
                    let attention = doc.read().await;
                    let my_events: Vec<_> = attention.events().iter()
                        .filter(|e| e.member == b.member_id)
                        .collect();
                    let total_secs = my_events.len() as u64 * 2;
                    let hours = total_secs / 3600;
                    let mins = (total_secs % 3600) / 60;
                    let time_str = if hours > 0 {
                        format!("{hours}h {mins}m")
                    } else {
                        format!("{mins}m")
                    };
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::ATTENTION_CONTRIBUTED.to_string(),
                        display_value: time_str,
                        grants: field_grants(indras_homepage::fields::ATTENTION_CONTRIBUTED),
                    });
                }

                // Contact count
                if let Some(contacts_realm) = b.network.contacts_realm().await {
                    let count = contacts_realm.contact_count().await;
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::CONTACT_COUNT.to_string(),
                        display_value: count.to_string(),
                        grants: field_grants(indras_homepage::fields::CONTACT_COUNT),
                    });
                }

                // Humanness freshness
                if let Ok(doc) = b.home.document::<indras_sync_engine::HumannessDocument>("humanness").await {
                    let humanness = doc.read().await;
                    let now_millis = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    let freshness = humanness.freshness_at(&b.member_id, now_millis);
                    field_artifacts.push(indras_homepage::ProfileFieldArtifact {
                        field_name: indras_homepage::fields::HUMANNESS_FRESHNESS.to_string(),
                        display_value: format!("{freshness}"),
                        grants: field_grants(indras_homepage::fields::HUMANNESS_FRESHNESS),
                    });
                }

                // Push fields to homepage server
                let vis = crate::data::build_profile_field_visibility(&field_artifacts, &peer_names);
                profile_fields.set(vis);

                // Write computed fields to HomepageProfileDocument CRDT
                if let Ok(doc) = b.home.document::<indras_sync_engine::HomepageProfileDocument>("_homepage_profile").await {
                    let crdt_fields: Vec<indras_sync_engine::HomepageField> = field_artifacts.iter().map(|f| {
                        indras_sync_engine::HomepageField {
                            name: f.field_name.clone(),
                            value: f.display_value.clone(),
                            grants_json: serde_json::to_string(&f.grants).unwrap_or_default(),
                        }
                    }).collect();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let _ = doc.update(|d| {
                        d.fields = crdt_fields;
                        d.updated_at = now;
                    }).await;
                }

                // Persist to BlobStore for relay-hosted offline serving
                if let Some(ref store) = *homepage_store_sig.read() {
                    let _ = store.save_profile(&field_artifacts);
                }

                *fields_handle.write().await = field_artifacts;

                // Sync connections-only grants with current contact list
                let _ = b.sync_connections_only_grants().await;

                // Build content artifacts from non-profile entries in artifact index
                if let Some(ref artifact_doc) = artifact_index {
                    let index = artifact_doc.read().await;
                    let content_artifacts: Vec<indras_homepage::ContentArtifact> = index
                        .active_artifacts()
                        .filter(|entry| {
                            // Skip profile field artifacts
                            !entry.name.starts_with("profile:")
                        })
                        .map(|entry| {
                            let aid = match &entry.id {
                                indras_artifacts::ArtifactId::Doc(id) => *id,
                                _ => [0u8; 32],
                            };
                            indras_homepage::ContentArtifact {
                                artifact_id: aid,
                                name: entry.name.clone(),
                                mime_type: entry.mime_type.clone(),
                                size: entry.size,
                                created_at: entry.created_at,
                                grants: entry.grants.clone(),
                            }
                        })
                        .collect();
                    if let Some(ref artifacts_handle) = b.homepage_artifacts {
                        *artifacts_handle.write().await = content_artifacts;
                    }
                }
            }

            // Refresh detail view if showing one
            let view = current_view.read().clone();
            if let AppView::Detail(id) = view {
                let detail =
                    data::build_intention_view(&b.home, &b.network, id, b.member_id, &b.player_name, &peer_names).await;
                detail_data.set(detail);
            }
        }
    });

    // Build peer name lookup for render-time use
    let render_peer_names: std::collections::HashMap<_, _> = peers
        .read()
        .iter()
        .map(|p| (p.member_id, p.name.clone()))
        .collect();

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
                                        boot_network(net, &mut bridge, &mut boot_error, &mut homepage_store_sig).await;
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
                on_profile: move |_| current_view.set(AppView::Profile),
                relay_status: Some("Relay".to_string()),
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
                                peer_names: render_peer_names.clone(),
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
                                peer_names: render_peer_names.clone(),
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
                        AppView::Profile => rsx! {
                            ProfileGrantsPanel {
                                bridge: bridge().unwrap(),
                                profile_fields: profile_fields(),
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
