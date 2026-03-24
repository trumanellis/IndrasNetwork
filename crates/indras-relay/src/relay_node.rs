//! Core relay node implementation
//!
//! The `RelayNode` combines an iroh transport endpoint with gossip subscription
//! and blob storage to provide blind store-and-forward relay services.
//!
//! ## Architecture
//!
//! The relay does NOT use `IndrasNode` or `IrohNetworkAdapter`. Instead it
//! creates its own iroh `Endpoint` and `Gossip` instance, accepting connections
//! on the `indras/1` ALPN protocol and subscribing to gossip topics for
//! registered interfaces.
//!
//! Gossip topics are derived using the same algorithm as `DiscoveryService::topic_for_interface`
//! in indras-transport so that the relay observes the same topics peers publish to.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_lite::StreamExt;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, SecretKey};
use iroh_gossip::api::{Event, GossipReceiver};
use iroh_gossip::net::GOSSIP_ALPN;
use iroh_gossip::proto::TopicId;
use iroh_gossip::Gossip;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use indras_core::InterfaceId;
use indras_core::identity::PeerIdentity;
use indras_transport::identity::IrohIdentity;
use indras_transport::protocol::{
    RelayAuthAckMessage, RelayContactsSyncAckMessage, RelayDeliveryMessage,
    RelayRegisterAckMessage, RelayStoreAckMessage, StorageTier, StoredEvent, TierQuotaInfo,
    WireMessage, frame_message, parse_framed_message, ALPN_INDRAS, MAX_MESSAGE_SIZE,
};

use crate::admin::{self, AdminState};
use crate::auth::AuthService;
use crate::blob_store::BlobStore;
use crate::config::RelayConfig;
use crate::error::{RelayError, RelayResult};
use crate::quota::{QuotaManager, TieredQuotaManager};
use crate::registration::RegistrationState;

/// Relay service that processes relay protocol on pre-accepted bi streams.
/// Does not own an endpoint — composable into any iroh-based node.
pub struct RelayService {
    config: RelayConfig,
    blob_store: Arc<BlobStore>,
    registrations: Arc<RegistrationState>,
    auth: Arc<AuthService>,
    quota: Arc<QuotaManager>,
    tiered_quota: Arc<TieredQuotaManager>,
    gossip: Option<Gossip>,
}

impl RelayService {
    /// Create a new relay service with the given configuration.
    ///
    /// Initializes blob store, registration state, auth, and quota managers.
    /// Does not create a shutdown token — the parent node manages lifecycle.
    pub async fn new(config: RelayConfig) -> RelayResult<Self> {
        // Ensure data directory exists
        std::fs::create_dir_all(&config.data_dir).map_err(|e| {
            RelayError::Config(format!("Failed to create data dir: {e}"))
        })?;

        // Initialize blob store
        let db_path = config.data_dir.join("events.redb");
        let blob_store = Arc::new(BlobStore::open(&db_path)?);

        // Initialize registration state
        let reg_path = config.data_dir.join("registrations.json");
        let registrations = Arc::new(RegistrationState::new(reg_path));
        registrations.load()?;

        // Initialize quota manager
        let quota = Arc::new(QuotaManager::new(config.quota.clone()));

        // Initialize auth service
        let auth = Arc::new(AuthService::new(&config));

        // Load persisted contacts
        let contacts_path = config.data_dir.join("contacts.json");
        if let Err(e) = auth.load_contacts(&contacts_path) {
            tracing::warn!(error = %e, "Failed to load contacts, starting with empty list");
        }

        // Initialize tiered quota manager
        let tiered_quota = Arc::new(TieredQuotaManager::new(config.tiers.clone()));

        Ok(Self {
            config,
            blob_store,
            registrations,
            auth,
            quota,
            tiered_quota,
            gossip: None,
        })
    }

    /// Attach a gossip instance (for interface subscription on register).
    pub fn with_gossip(mut self, gossip: Gossip) -> Self {
        self.gossip = Some(gossip);
        self
    }

    /// Return a reference to the auth service.
    pub fn auth(&self) -> &Arc<AuthService> {
        &self.auth
    }

    /// Return a reference to the blob store.
    pub fn blob_store(&self) -> &Arc<BlobStore> {
        &self.blob_store
    }

    /// Handle the relay protocol on a pre-accepted bidirectional stream.
    ///
    /// The streams must already be accepted by the caller — this method does
    /// not call `conn.accept_bi()`. `peer_identity` is the transport identity
    /// of the remote peer extracted from the connection.
    pub async fn handle_bi_stream(
        &self,
        peer_identity: IrohIdentity,
        mut send_stream: iroh::endpoint::SendStream,
        mut recv_stream: iroh::endpoint::RecvStream,
    ) -> RelayResult<()> {
        let peer_id = peer_identity;
        let peer_key = peer_id.public_key();
        debug!(peer = %peer_key.fmt_short(), "Peer connected");

        let mut authenticated = false;

        loop {
            // Read 4-byte length prefix
            let mut len_buf = [0u8; 4];
            match recv_stream.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) => {
                    debug!(peer = %peer_key.fmt_short(), "Stream closed: {e}");
                    break;
                }
            }

            let msg_len = u32::from_be_bytes(len_buf) as usize;
            if msg_len > MAX_MESSAGE_SIZE {
                warn!(
                    peer = %peer_key.fmt_short(),
                    size = msg_len,
                    "Message too large, closing connection"
                );
                break;
            }

            // Read message body
            let mut msg_buf = vec![0u8; msg_len];
            if let Err(e) = recv_stream.read_exact(&mut msg_buf).await {
                debug!(peer = %peer_key.fmt_short(), "Failed to read message body: {e}");
                break;
            }

            let msg: WireMessage = match postcard::from_bytes(&msg_buf) {
                Ok(m) => m,
                Err(e) => {
                    warn!(peer = %peer_key.fmt_short(), error = %e, "Deserialize error");
                    continue;
                }
            };

            match msg {
                WireMessage::RelayAuth(auth_msg) => {
                    let result = self.auth.authenticate(
                        &peer_id,
                        &auth_msg.credential,
                        &auth_msg.player_id,
                    );

                    let response = match result {
                        Ok(session) => {
                            authenticated = true;
                            let tier_quotas: Vec<TierQuotaInfo> = session.granted_tiers.iter().map(|t| {
                                TierQuotaInfo {
                                    tier: *t,
                                    max_bytes: crate::tier::tier_max_bytes(*t, self.tiered_quota.tier_config()),
                                    used_bytes: self.tiered_quota.peer_tier_bytes(&peer_id, *t),
                                    max_interfaces: crate::tier::tier_max_interfaces(*t, self.tiered_quota.tier_config()),
                                }
                            }).collect();

                            RelayAuthAckMessage {
                                authenticated: true,
                                granted_tiers: session.granted_tiers,
                                tier_quotas,
                                timestamp_millis: chrono::Utc::now().timestamp_millis(),
                            }
                        }
                        Err(e) => {
                            warn!(peer = %peer_key.fmt_short(), error = %e, "Authentication failed");
                            RelayAuthAckMessage {
                                authenticated: false,
                                granted_tiers: vec![],
                                tier_quotas: vec![],
                                timestamp_millis: chrono::Utc::now().timestamp_millis(),
                            }
                        }
                    };

                    let framed = frame_message(&WireMessage::RelayAuthAck(response))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&framed).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send auth ack: {e}"))
                    })?;
                }

                WireMessage::RelayRegister(register) => {
                    if !authenticated {
                        warn!(peer = %peer_key.fmt_short(), "Unauthenticated peer attempted operation");
                        continue;
                    }

                    self.registrations.touch(&peer_id);
                    let response = if let Some(gossip) = &self.gossip {
                        handle_register(
                            &peer_id,
                            register,
                            &self.registrations,
                            &self.quota,
                            gossip,
                            &self.blob_store,
                            &self.auth,
                            &self.tiered_quota,
                        )
                        .await
                    } else {
                        // No gossip attached — reject all registrations
                        let rejected = register.interfaces
                            .into_iter()
                            .map(|iface| (iface, "Gossip not available".to_string()))
                            .collect();
                        RelayRegisterAckMessage::new(vec![]).with_rejected(rejected)
                    };

                    let framed = frame_message(&WireMessage::RelayRegisterAck(response))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&framed).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send ack: {e}"))
                    })?;
                }

                WireMessage::RelayUnregister(unregister) => {
                    if !authenticated {
                        warn!(peer = %peer_key.fmt_short(), "Unauthenticated peer attempted operation");
                        continue;
                    }

                    let count = unregister.interfaces.len();
                    self.registrations.unregister(&peer_id, &unregister.interfaces)?;
                    self.quota.record_unregistration(&peer_id, count);
                    info!(peer = %peer_key.fmt_short(), count, "Unregistered interfaces");
                }

                WireMessage::RelayRetrieve(retrieve) => {
                    if !authenticated {
                        warn!(peer = %peer_key.fmt_short(), "Unauthenticated peer attempted operation");
                        continue;
                    }

                    self.registrations.touch(&peer_id);
                    let tier = retrieve.tier.unwrap_or(StorageTier::Connections);

                    if !self.auth.has_tier_access(&peer_id, tier) {
                        warn!(peer = %peer_key.fmt_short(), ?tier, "No access to retrieve from tier");
                        continue;
                    }

                    const RETRIEVE_PAGE_SIZE: usize = 100;

                    let mut events = self.blob_store
                        .events_after_tiered(tier, retrieve.interface_id, retrieve.after_event_id)?;

                    let has_more = events.len() > RETRIEVE_PAGE_SIZE;
                    if has_more {
                        events.truncate(RETRIEVE_PAGE_SIZE);
                    }

                    let delivery = if has_more {
                        RelayDeliveryMessage::new(retrieve.interface_id, events).with_more()
                    } else {
                        RelayDeliveryMessage::new(retrieve.interface_id, events)
                    };
                    let framed = frame_message(&WireMessage::RelayDelivery(delivery))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&framed).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send delivery: {e}"))
                    })?;

                    debug!(
                        peer = %peer_key.fmt_short(),
                        interface = %short_hex(retrieve.interface_id.as_bytes()),
                        "Delivered stored events"
                    );
                }

                WireMessage::RelayStore(store_msg) => {
                    if !authenticated {
                        warn!(peer = %peer_key.fmt_short(), "Unauthenticated store attempt");
                        continue;
                    }

                    let has_access = self.auth.has_tier_access(&peer_id, store_msg.tier);
                    if !has_access {
                        let ack = RelayStoreAckMessage {
                            accepted: false,
                            reason: Some(format!("No access to {:?} tier", store_msg.tier)),
                            timestamp_millis: chrono::Utc::now().timestamp_millis(),
                        };
                        let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                            .map_err(|e| RelayError::Serialization(e.to_string()))?;
                        send_stream.write_all(&framed).await.map_err(|e| {
                            RelayError::Transport(format!("Failed to send store ack: {e}"))
                        })?;
                        continue;
                    }

                    let data_len = store_msg.data.len() as u64;
                    if let Err(e) = self.tiered_quota.can_store_tiered(&peer_id, store_msg.tier, data_len) {
                        let ack = RelayStoreAckMessage {
                            accepted: false,
                            reason: Some(e.to_string()),
                            timestamp_millis: chrono::Utc::now().timestamp_millis(),
                        };
                        let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                            .map_err(|e| RelayError::Serialization(e.to_string()))?;
                        send_stream.write_all(&framed).await.map_err(|e| {
                            RelayError::Transport(format!("Failed to send store ack: {e}"))
                        })?;
                        continue;
                    }

                    let total_usage = self.blob_store.total_usage_bytes().unwrap_or(0);
                    if let Err(e) = self.quota.can_store(&peer_id, data_len, total_usage) {
                        let ack = RelayStoreAckMessage {
                            accepted: false,
                            reason: Some(e.to_string()),
                            timestamp_millis: chrono::Utc::now().timestamp_millis(),
                        };
                        let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                            .map_err(|e| RelayError::Serialization(e.to_string()))?;
                        send_stream.write_all(&framed).await.map_err(|e| {
                            RelayError::Transport(format!("Failed to send store ack: {e}"))
                        })?;
                        continue;
                    }

                    let event_id = indras_core::EventId::new(0, chrono::Utc::now().timestamp_millis() as u64);
                    let stored = StoredEvent::new(event_id, store_msg.data, [0u8; 12]);
                    match self.blob_store.store_event_tiered(store_msg.tier, store_msg.interface_id, &stored) {
                        Ok(()) => {
                            self.tiered_quota.record_storage_tiered(peer_id, store_msg.tier, data_len);

                            if store_msg.metadata.pin {
                                if let Err(e) = self.blob_store.pin_event(
                                    store_msg.tier,
                                    &store_msg.interface_id,
                                    &event_id,
                                ) {
                                    warn!(error = %e, "Failed to pin event");
                                }
                            }

                            if let Some(ttl_days) = store_msg.metadata.ttl_override_days {
                                let clamped = ttl_days.min(self.config.storage.max_event_ttl_days);
                                if let Err(e) = self.blob_store.set_ttl_override(
                                    &store_msg.interface_id,
                                    &event_id,
                                    clamped,
                                ) {
                                    warn!(error = %e, "Failed to set TTL override");
                                }
                            }

                            let ack = RelayStoreAckMessage {
                                accepted: true,
                                reason: None,
                                timestamp_millis: chrono::Utc::now().timestamp_millis(),
                            };
                            let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                                .map_err(|e| RelayError::Serialization(e.to_string()))?;
                            send_stream.write_all(&framed).await.map_err(|e| {
                                RelayError::Transport(format!("Failed to send store ack: {e}"))
                            })?;
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to store tiered data");
                            let ack = RelayStoreAckMessage {
                                accepted: false,
                                reason: Some(e.to_string()),
                                timestamp_millis: chrono::Utc::now().timestamp_millis(),
                            };
                            let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                                .map_err(|e| RelayError::Serialization(e.to_string()))?;
                            send_stream.write_all(&framed).await.map_err(|e| {
                                RelayError::Transport(format!("Failed to send store ack: {e}"))
                            })?;
                        }
                    }
                }

                WireMessage::Ping(n) => {
                    let pong = frame_message(&WireMessage::Pong(n))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&pong).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send pong: {e}"))
                    })?;
                }

                WireMessage::RelayContactsSync(sync_msg) => {
                    if !authenticated {
                        warn!(peer = %peer_key.fmt_short(), "Unauthenticated contacts sync attempt");
                        continue;
                    }

                    let is_owner = self.auth.has_tier_access(&peer_id, StorageTier::Self_);
                    let response = if is_owner {
                        self.auth.sync_contacts(sync_msg.contacts);
                        let contacts_path = self.config.data_dir.join("contacts.json");
                        if let Err(e) = self.auth.save_contacts(&contacts_path) {
                            warn!(error = %e, "Failed to persist contacts");
                        }
                        info!(
                            peer = %peer_key.fmt_short(),
                            count = self.auth.contact_count(),
                            "Contacts synced from owner"
                        );
                        RelayContactsSyncAckMessage {
                            accepted: true,
                            contact_count: self.auth.contact_count() as u32,
                        }
                    } else {
                        warn!(peer = %peer_key.fmt_short(), "Non-owner attempted contacts sync");
                        RelayContactsSyncAckMessage {
                            accepted: false,
                            contact_count: 0,
                        }
                    };

                    let framed = frame_message(&WireMessage::RelayContactsSyncAck(response))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&framed).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send contacts sync ack: {e}"))
                    })?;
                }

                other => {
                    debug!(
                        peer = %peer_key.fmt_short(),
                        variant = ?std::mem::discriminant(&other),
                        "Ignoring unhandled message variant"
                    );
                }
            }
        }

        self.auth.remove_session(&peer_id);

        Ok(())
    }
}

/// Core relay server
pub struct RelayNode {
    service: RelayService,
    shutdown: CancellationToken,
}

impl RelayNode {
    /// Create a new relay node with the given configuration
    pub async fn new(config: RelayConfig) -> RelayResult<Self> {
        let service = RelayService::new(config).await?;
        let shutdown = CancellationToken::new();
        Ok(Self { service, shutdown })
    }

    /// Run the relay server until shutdown
    pub async fn run(&self) -> RelayResult<()> {
        // Load or generate a persistent secret key
        let key_path = self.service.config.data_dir.join("secret.key");
        let secret_key = load_or_generate_key(&key_path)?;
        let node_id = secret_key.public();

        info!(
            node_id = %node_id.fmt_short(),
            name = %self.service.config.display_name,
            "Relay node starting"
        );

        // Create iroh endpoint
        let endpoint = Endpoint::builder()
            .secret_key(secret_key)
            .alpns(vec![ALPN_INDRAS.to_vec()])
            .bind()
            .await
            .map_err(|e| RelayError::Transport(format!("Failed to create endpoint: {e}")))?;

        // Create gossip instance
        let gossip = Gossip::builder().spawn(endpoint.clone());

        // Set up connection handler channel
        let (conn_tx, mut conn_rx) = mpsc::channel::<Connection>(256);

        // Build protocol router (gossip + indras protocol)
        let conn_handler = ConnectionHandler { sender: conn_tx };
        let router = Router::builder(endpoint.clone())
            .accept(ALPN_INDRAS, conn_handler)
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();

        // Re-subscribe to gossip topics for existing registrations so we don't
        // miss events from before new peers connect after a restart.
        for iface in self.service.registrations.registered_interfaces() {
            let topic_id = topic_for_interface(&iface);
            match gossip.subscribe(topic_id, vec![]).await {
                Ok(topic) => {
                    let (_sender, receiver) = topic.split();
                    spawn_topic_observer(iface, receiver, self.service.blob_store.clone());
                    debug!(
                        interface = %short_hex(iface.as_bytes()),
                        "Re-subscribed to gossip topic on startup"
                    );
                }
                Err(e) => {
                    warn!(
                        interface = %short_hex(iface.as_bytes()),
                        error = %e,
                        "Failed to re-subscribe to gossip topic"
                    );
                }
            }
        }

        // Start admin API
        let admin_state = Arc::new(AdminState {
            config: self.service.config.clone(),
            blob_store: self.service.blob_store.clone(),
            registrations: self.service.registrations.clone(),
            auth: self.service.auth.clone(),
            started_at: std::time::Instant::now(),
        });
        let admin_router = admin::admin_router(admin_state);
        let admin_bind = self.service.config.admin_bind;
        let admin_shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(admin_bind)
                .await
                .expect("Failed to bind admin API");
            info!(bind = %admin_bind, "Admin API started");
            axum::serve(listener, admin_router)
                .with_graceful_shutdown(async move { admin_shutdown.cancelled().await })
                .await
                .ok();
        });

        // Spawn cleanup task
        let cleanup_store = self.service.blob_store.clone();
        let cleanup_tier_config = self.service.config.tiers.clone();
        let cleanup_interval = self.service.config.storage.cleanup_interval_secs;
        let cleanup_auth = self.service.auth.clone();
        let cleanup_shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            run_cleanup(cleanup_store, cleanup_tier_config, cleanup_interval, cleanup_auth, cleanup_shutdown).await;
        });

        // Main connection handling loop
        info!("Relay node ready, accepting connections");
        loop {
            tokio::select! {
                Some(conn) = conn_rx.recv() => {
                    let blob_store = self.service.blob_store.clone();
                    let registrations = self.service.registrations.clone();
                    let quota = self.service.quota.clone();
                    let gossip_clone = gossip.clone();
                    let auth = self.service.auth.clone();
                    let tiered_quota = self.service.tiered_quota.clone();
                    let data_dir = self.service.config.data_dir.clone();
                    let config = self.service.config.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(
                            conn,
                            blob_store,
                            registrations,
                            quota,
                            gossip_clone,
                            auth,
                            tiered_quota,
                            data_dir,
                            config,
                        ).await {
                            warn!(error = %e, "Connection handling error");
                        }
                    });
                }
                _ = self.shutdown.cancelled() => {
                    info!("Relay node shutting down");
                    break;
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Received Ctrl+C, shutting down");
                    self.shutdown.cancel();
                    break;
                }
            }
        }

        router.shutdown().await.map_err(|e| {
            RelayError::Transport(format!("Router shutdown error: {e}"))
        })?;

        Ok(())
    }

    /// Return a clone of the shutdown token for external shutdown signaling
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Start the relay and return its endpoint address
    ///
    /// Convenience method for tests and embedders. Spawns the relay in a
    /// background task and returns the endpoint address once ready.
    pub async fn start(
        self,
    ) -> RelayResult<(iroh::EndpointAddr, tokio::task::JoinHandle<RelayResult<()>>)> {
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let key_path = self.service.config.data_dir.join("secret.key");
        let secret_key = load_or_generate_key(&key_path)?;

        let endpoint = Endpoint::builder()
            .secret_key(secret_key)
            .alpns(vec![ALPN_INDRAS.to_vec()])
            .bind()
            .await
            .map_err(|e| RelayError::Transport(format!("Failed to create endpoint: {e}")))?;

        let handle = tokio::spawn(async move {
            self.run_with_endpoint(endpoint, addr_tx).await
        });

        let endpoint_addr = addr_rx.await.map_err(|_| {
            RelayError::Transport("Relay failed to start".into())
        })?;

        Ok((endpoint_addr, handle))
    }

    /// Run the relay with a pre-created endpoint, signaling readiness via `ready_tx`
    async fn run_with_endpoint(
        &self,
        endpoint: Endpoint,
        ready_tx: tokio::sync::oneshot::Sender<iroh::EndpointAddr>,
    ) -> RelayResult<()> {
        let node_id = endpoint.secret_key().public();
        info!(
            node_id = %node_id.fmt_short(),
            name = %self.service.config.display_name,
            "Relay node starting"
        );

        let gossip = Gossip::builder().spawn(endpoint.clone());
        let (conn_tx, mut conn_rx) = mpsc::channel::<Connection>(256);

        let conn_handler = ConnectionHandler { sender: conn_tx };
        let router = Router::builder(endpoint.clone())
            .accept(ALPN_INDRAS, conn_handler)
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();

        // Re-subscribe to gossip topics for existing registrations
        for iface in self.service.registrations.registered_interfaces() {
            let topic_id = topic_for_interface(&iface);
            match gossip.subscribe(topic_id, vec![]).await {
                Ok(topic) => {
                    let (_sender, receiver) = topic.split();
                    spawn_topic_observer(iface, receiver, self.service.blob_store.clone());
                    debug!(
                        interface = %short_hex(iface.as_bytes()),
                        "Re-subscribed to gossip topic on startup"
                    );
                }
                Err(e) => {
                    warn!(
                        interface = %short_hex(iface.as_bytes()),
                        error = %e,
                        "Failed to re-subscribe to gossip topic"
                    );
                }
            }
        }

        // Start admin API
        let admin_state = Arc::new(AdminState {
            config: self.service.config.clone(),
            blob_store: self.service.blob_store.clone(),
            registrations: self.service.registrations.clone(),
            auth: self.service.auth.clone(),
            started_at: std::time::Instant::now(),
        });
        let admin_router = admin::admin_router(admin_state);
        let admin_bind = self.service.config.admin_bind;
        let admin_shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(admin_bind)
                .await
                .expect("Failed to bind admin API");
            info!(bind = %admin_bind, "Admin API started");
            axum::serve(listener, admin_router)
                .with_graceful_shutdown(async move { admin_shutdown.cancelled().await })
                .await
                .ok();
        });

        // Spawn cleanup task
        let cleanup_store = self.service.blob_store.clone();
        let cleanup_tier_config = self.service.config.tiers.clone();
        let cleanup_interval = self.service.config.storage.cleanup_interval_secs;
        let cleanup_auth = self.service.auth.clone();
        let cleanup_shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            run_cleanup(
                cleanup_store,
                cleanup_tier_config,
                cleanup_interval,
                cleanup_auth,
                cleanup_shutdown,
            )
            .await;
        });

        // Signal ready with our endpoint address
        let addr = endpoint.addr();
        let _ = ready_tx.send(addr);

        // Main connection handling loop
        info!("Relay node ready, accepting connections");
        loop {
            tokio::select! {
                Some(conn) = conn_rx.recv() => {
                    let blob_store = self.service.blob_store.clone();
                    let registrations = self.service.registrations.clone();
                    let quota = self.service.quota.clone();
                    let gossip_clone = gossip.clone();
                    let auth = self.service.auth.clone();
                    let tiered_quota = self.service.tiered_quota.clone();
                    let data_dir = self.service.config.data_dir.clone();
                    let config = self.service.config.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(
                            conn,
                            blob_store,
                            registrations,
                            quota,
                            gossip_clone,
                            auth,
                            tiered_quota,
                            data_dir,
                            config,
                        )
                        .await
                        {
                            warn!(error = %e, "Connection handling error");
                        }
                    });
                }
                _ = self.shutdown.cancelled() => {
                    info!("Relay node shutting down");
                    break;
                }
            }
        }

        router.shutdown().await.map_err(|e| {
            RelayError::Transport(format!("Router shutdown error: {e}"))
        })?;

        Ok(())
    }
}

// ============================================================================
// Protocol handler
// ============================================================================

/// Protocol handler that forwards accepted QUIC connections to the main loop
#[derive(Debug, Clone)]
struct ConnectionHandler {
    sender: mpsc::Sender<Connection>,
}

impl ProtocolHandler for ConnectionHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.sender.send(connection).await.map_err(|_| {
            AcceptError::from(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Connection channel closed",
            ))
        })
    }
}

// ============================================================================
// Connection handling
// ============================================================================

/// Handle a single peer connection: read framed WireMessages and dispatch them
async fn handle_connection(
    conn: Connection,
    blob_store: Arc<BlobStore>,
    registrations: Arc<RegistrationState>,
    quota: Arc<QuotaManager>,
    gossip: Gossip,
    auth: Arc<AuthService>,
    tiered_quota: Arc<TieredQuotaManager>,
    data_dir: std::path::PathBuf,
    config: RelayConfig,
) -> RelayResult<()> {
    let peer_key = conn.remote_id();
    let peer_id = IrohIdentity::new(peer_key);
    debug!(peer = %peer_key.fmt_short(), "Peer connected");

    let mut authenticated = false;

    // Accept a bidirectional stream for the relay protocol exchange
    let (mut send_stream, mut recv_stream) = conn.accept_bi().await.map_err(|e| {
        RelayError::Transport(format!("Failed to accept stream: {e}"))
    })?;

    loop {
        // Read 4-byte length prefix
        let mut len_buf = [0u8; 4];
        match recv_stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) => {
                debug!(peer = %peer_key.fmt_short(), "Stream closed: {e}");
                break;
            }
        }

        let msg_len = u32::from_be_bytes(len_buf) as usize;
        if msg_len > MAX_MESSAGE_SIZE {
            warn!(
                peer = %peer_key.fmt_short(),
                size = msg_len,
                "Message too large, closing connection"
            );
            break;
        }

        // Read message body
        let mut msg_buf = vec![0u8; msg_len];
        if let Err(e) = recv_stream.read_exact(&mut msg_buf).await {
            debug!(peer = %peer_key.fmt_short(), "Failed to read message body: {e}");
            break;
        }

        // Deserialize — the body is postcard bytes without the length prefix
        let msg: WireMessage = match postcard::from_bytes(&msg_buf) {
            Ok(m) => m,
            Err(e) => {
                warn!(peer = %peer_key.fmt_short(), error = %e, "Deserialize error");
                continue;
            }
        };

        match msg {
            WireMessage::RelayAuth(auth_msg) => {
                let result = auth.authenticate(
                    &peer_id,
                    &auth_msg.credential,
                    &auth_msg.player_id,
                );

                let response = match result {
                    Ok(session) => {
                        authenticated = true;
                        let tier_quotas: Vec<TierQuotaInfo> = session.granted_tiers.iter().map(|t| {
                            TierQuotaInfo {
                                tier: *t,
                                max_bytes: crate::tier::tier_max_bytes(*t, tiered_quota.tier_config()),
                                used_bytes: tiered_quota.peer_tier_bytes(&peer_id, *t),
                                max_interfaces: crate::tier::tier_max_interfaces(*t, tiered_quota.tier_config()),
                            }
                        }).collect();

                        RelayAuthAckMessage {
                            authenticated: true,
                            granted_tiers: session.granted_tiers,
                            tier_quotas,
                            timestamp_millis: chrono::Utc::now().timestamp_millis(),
                        }
                    }
                    Err(e) => {
                        warn!(peer = %peer_key.fmt_short(), error = %e, "Authentication failed");
                        RelayAuthAckMessage {
                            authenticated: false,
                            granted_tiers: vec![],
                            tier_quotas: vec![],
                            timestamp_millis: chrono::Utc::now().timestamp_millis(),
                        }
                    }
                };

                let framed = frame_message(&WireMessage::RelayAuthAck(response))
                    .map_err(|e| RelayError::Serialization(e.to_string()))?;
                send_stream.write_all(&framed).await.map_err(|e| {
                    RelayError::Transport(format!("Failed to send auth ack: {e}"))
                })?;
            }

            WireMessage::RelayRegister(register) => {
                if !authenticated {
                    warn!(peer = %peer_key.fmt_short(), "Unauthenticated peer attempted operation");
                    continue;
                }

                registrations.touch(&peer_id);
                let response = handle_register(
                    &peer_id,
                    register,
                    &registrations,
                    &quota,
                    &gossip,
                    &blob_store,
                    &auth,
                    &tiered_quota,
                )
                .await;

                let framed = frame_message(&WireMessage::RelayRegisterAck(response))
                    .map_err(|e| RelayError::Serialization(e.to_string()))?;
                send_stream.write_all(&framed).await.map_err(|e| {
                    RelayError::Transport(format!("Failed to send ack: {e}"))
                })?;
            }

            WireMessage::RelayUnregister(unregister) => {
                if !authenticated {
                    warn!(peer = %peer_key.fmt_short(), "Unauthenticated peer attempted operation");
                    continue;
                }

                let count = unregister.interfaces.len();
                registrations.unregister(&peer_id, &unregister.interfaces)?;
                quota.record_unregistration(&peer_id, count);
                info!(peer = %peer_key.fmt_short(), count, "Unregistered interfaces");
                // Gossip subscriptions from spawn_topic_observer remain active
                // until those tasks' receivers are dropped; since interfaces
                // may be re-registered we leave them running.
            }

            WireMessage::RelayRetrieve(retrieve) => {
                if !authenticated {
                    warn!(peer = %peer_key.fmt_short(), "Unauthenticated peer attempted operation");
                    continue;
                }

                registrations.touch(&peer_id);
                let tier = retrieve.tier.unwrap_or(StorageTier::Connections);

                // Check tier access
                if !auth.has_tier_access(&peer_id, tier) {
                    warn!(peer = %peer_key.fmt_short(), ?tier, "No access to retrieve from tier");
                    continue;
                }

                const RETRIEVE_PAGE_SIZE: usize = 100;

                let mut events = blob_store
                    .events_after_tiered(tier, retrieve.interface_id, retrieve.after_event_id)?;

                let has_more = events.len() > RETRIEVE_PAGE_SIZE;
                if has_more {
                    events.truncate(RETRIEVE_PAGE_SIZE);
                }

                let delivery = if has_more {
                    RelayDeliveryMessage::new(retrieve.interface_id, events).with_more()
                } else {
                    RelayDeliveryMessage::new(retrieve.interface_id, events)
                };
                let framed = frame_message(&WireMessage::RelayDelivery(delivery))
                    .map_err(|e| RelayError::Serialization(e.to_string()))?;
                send_stream.write_all(&framed).await.map_err(|e| {
                    RelayError::Transport(format!("Failed to send delivery: {e}"))
                })?;

                debug!(
                    peer = %peer_key.fmt_short(),
                    interface = %short_hex(retrieve.interface_id.as_bytes()),
                    "Delivered stored events"
                );
            }

            WireMessage::RelayStore(store_msg) => {
                if !authenticated {
                    warn!(peer = %peer_key.fmt_short(), "Unauthenticated store attempt");
                    continue;
                }

                // Check tier access
                let has_access = auth.has_tier_access(&peer_id, store_msg.tier);
                if !has_access {
                    let ack = RelayStoreAckMessage {
                        accepted: false,
                        reason: Some(format!("No access to {:?} tier", store_msg.tier)),
                        timestamp_millis: chrono::Utc::now().timestamp_millis(),
                    };
                    let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&framed).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send store ack: {e}"))
                    })?;
                    continue;
                }

                // Check tier quota
                let data_len = store_msg.data.len() as u64;
                if let Err(e) = tiered_quota.can_store_tiered(&peer_id, store_msg.tier, data_len) {
                    let ack = RelayStoreAckMessage {
                        accepted: false,
                        reason: Some(e.to_string()),
                        timestamp_millis: chrono::Utc::now().timestamp_millis(),
                    };
                    let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&framed).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send store ack: {e}"))
                    })?;
                    continue;
                }

                // Check flat quota (per-peer and global byte limits)
                let total_usage = blob_store.total_usage_bytes().unwrap_or(0);
                if let Err(e) = quota.can_store(&peer_id, data_len, total_usage) {
                    let ack = RelayStoreAckMessage {
                        accepted: false,
                        reason: Some(e.to_string()),
                        timestamp_millis: chrono::Utc::now().timestamp_millis(),
                    };
                    let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                        .map_err(|e| RelayError::Serialization(e.to_string()))?;
                    send_stream.write_all(&framed).await.map_err(|e| {
                        RelayError::Transport(format!("Failed to send store ack: {e}"))
                    })?;
                    continue;
                }

                // Store the data as a StoredEvent
                let event_id = indras_core::EventId::new(0, chrono::Utc::now().timestamp_millis() as u64);
                let stored = StoredEvent::new(event_id, store_msg.data, [0u8; 12]);
                match blob_store.store_event_tiered(store_msg.tier, store_msg.interface_id, &stored) {
                    Ok(()) => {
                        tiered_quota.record_storage_tiered(peer_id, store_msg.tier, data_len);

                        // Honor pin flag
                        if store_msg.metadata.pin {
                            if let Err(e) = blob_store.pin_event(
                                store_msg.tier,
                                &store_msg.interface_id,
                                &event_id,
                            ) {
                                warn!(error = %e, "Failed to pin event");
                            }
                        }

                        // Honor TTL override, clamped to the configured maximum
                        if let Some(ttl_days) = store_msg.metadata.ttl_override_days {
                            let clamped = ttl_days.min(config.storage.max_event_ttl_days);
                            if let Err(e) = blob_store.set_ttl_override(
                                &store_msg.interface_id,
                                &event_id,
                                clamped,
                            ) {
                                warn!(error = %e, "Failed to set TTL override");
                            }
                        }

                        let ack = RelayStoreAckMessage {
                            accepted: true,
                            reason: None,
                            timestamp_millis: chrono::Utc::now().timestamp_millis(),
                        };
                        let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                            .map_err(|e| RelayError::Serialization(e.to_string()))?;
                        send_stream.write_all(&framed).await.map_err(|e| {
                            RelayError::Transport(format!("Failed to send store ack: {e}"))
                        })?;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to store tiered data");
                        let ack = RelayStoreAckMessage {
                            accepted: false,
                            reason: Some(e.to_string()),
                            timestamp_millis: chrono::Utc::now().timestamp_millis(),
                        };
                        let framed = frame_message(&WireMessage::RelayStoreAck(ack))
                            .map_err(|e| RelayError::Serialization(e.to_string()))?;
                        send_stream.write_all(&framed).await.map_err(|e| {
                            RelayError::Transport(format!("Failed to send store ack: {e}"))
                        })?;
                    }
                }
            }

            WireMessage::Ping(n) => {
                let pong = frame_message(&WireMessage::Pong(n))
                    .map_err(|e| RelayError::Serialization(e.to_string()))?;
                send_stream.write_all(&pong).await.map_err(|e| {
                    RelayError::Transport(format!("Failed to send pong: {e}"))
                })?;
            }

            WireMessage::RelayContactsSync(sync_msg) => {
                if !authenticated {
                    warn!(peer = %peer_key.fmt_short(), "Unauthenticated contacts sync attempt");
                    continue;
                }

                // Only accept from owner (Self_ tier)
                let is_owner = auth.has_tier_access(&peer_id, StorageTier::Self_);
                let response = if is_owner {
                    auth.sync_contacts(sync_msg.contacts);
                    let contacts_path = data_dir.join("contacts.json");
                    if let Err(e) = auth.save_contacts(&contacts_path) {
                        warn!(error = %e, "Failed to persist contacts");
                    }
                    info!(
                        peer = %peer_key.fmt_short(),
                        count = auth.contact_count(),
                        "Contacts synced from owner"
                    );
                    RelayContactsSyncAckMessage {
                        accepted: true,
                        contact_count: auth.contact_count() as u32,
                    }
                } else {
                    warn!(peer = %peer_key.fmt_short(), "Non-owner attempted contacts sync");
                    RelayContactsSyncAckMessage {
                        accepted: false,
                        contact_count: 0,
                    }
                };

                let framed = frame_message(&WireMessage::RelayContactsSyncAck(response))
                    .map_err(|e| RelayError::Serialization(e.to_string()))?;
                send_stream.write_all(&framed).await.map_err(|e| {
                    RelayError::Transport(format!("Failed to send contacts sync ack: {e}"))
                })?;
            }

            other => {
                debug!(
                    peer = %peer_key.fmt_short(),
                    variant = ?std::mem::discriminant(&other),
                    "Ignoring unhandled message variant"
                );
            }
        }
    }

    auth.remove_session(&peer_id);

    Ok(())
}

// ============================================================================
// Registration handling
// ============================================================================

/// Process a RelayRegister message: check quota, subscribe to gossip, persist
async fn handle_register(
    peer_id: &IrohIdentity,
    register: indras_transport::protocol::RelayRegisterMessage,
    registrations: &Arc<RegistrationState>,
    quota: &Arc<QuotaManager>,
    gossip: &Gossip,
    blob_store: &Arc<BlobStore>,
    auth: &Arc<AuthService>,
    tiered_quota: &Arc<TieredQuotaManager>,
) -> RelayRegisterAckMessage {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();

    // Check quota before processing individual interfaces
    if let Err(e) = quota.can_register(peer_id, register.interfaces.len()) {
        for iface in register.interfaces {
            rejected.push((iface, e.to_string()));
        }
        return RelayRegisterAckMessage::new(accepted).with_rejected(rejected);
    }

    // Determine the peer's highest tier for registration quota
    let peer_tier = auth
        .get_session(peer_id)
        .map(|s| s.highest_tier)
        .unwrap_or(StorageTier::Public);

    // Check tiered registration quota
    if let Err(e) = tiered_quota.can_register_tiered(peer_id, peer_tier, register.interfaces.len()) {
        for iface in register.interfaces {
            rejected.push((iface, e.to_string()));
        }
        return RelayRegisterAckMessage::new(accepted).with_rejected(rejected);
    }

    for iface in register.interfaces {
        let topic_id = topic_for_interface(&iface);
        match gossip.subscribe(topic_id, vec![]).await {
            Ok(topic) => {
                let (_sender, receiver) = topic.split();
                spawn_topic_observer(iface, receiver, blob_store.clone());
                accepted.push(iface);
                debug!(interface = %short_hex(iface.as_bytes()), "Subscribed to gossip topic");
            }
            Err(e) => {
                rejected.push((iface, format!("Failed to subscribe to gossip: {e}")));
            }
        }
    }

    if !accepted.is_empty() {
        if let Err(e) = registrations.register(
            *peer_id,
            accepted.clone(),
            register.display_name.clone(),
        ) {
            warn!(error = %e, "Failed to persist registration");
        }
        quota.record_registration(*peer_id, accepted.len());
        tiered_quota.record_registration_tiered(*peer_id, peer_tier, accepted.len());
    }

    info!(
        peer = %short_hex(&peer_id.as_bytes()),
        accepted = accepted.len(),
        rejected = rejected.len(),
        "Processed registration"
    );

    RelayRegisterAckMessage::new(accepted).with_rejected(rejected)
}

// ============================================================================
// Gossip observer
// ============================================================================

/// Spawn a task that drains a `GossipReceiver` and stores `InterfaceEvent` messages
///
/// Each call spawns an independent task. The task terminates when the receiver
/// stream ends (i.e. when the gossip topic subscription is dropped by iroh-gossip).
fn spawn_topic_observer(
    interface_id: InterfaceId,
    mut receiver: GossipReceiver,
    blob_store: Arc<BlobStore>,
) {
    tokio::spawn(async move {
        while let Some(event_result) = receiver.next().await {
            match event_result {
                Ok(Event::Received(msg)) => {
                    store_gossip_event(&interface_id, &msg.content, &blob_store);
                }
                Ok(_) => {
                    // NeighborUp / NeighborDown — not relevant for relay storage
                }
                Err(e) => {
                    warn!(
                        interface = %short_hex(interface_id.as_bytes()),
                        error = %e,
                        "Gossip receiver error"
                    );
                    break;
                }
            }
        }
        debug!(
            interface = %short_hex(interface_id.as_bytes()),
            "Gossip observer task ended"
        );
    });
}

/// Parse gossip bytes as a framed WireMessage and, if it is an InterfaceEvent,
/// extract and persist the encrypted payload into the blob store.
fn store_gossip_event(interface_id: &InterfaceId, data: &Bytes, blob_store: &BlobStore) {
    let msg = match parse_framed_message(data) {
        Ok(m) => m,
        Err(e) => {
            debug!(error = %e, "Failed to parse gossip message");
            return;
        }
    };

    if let WireMessage::InterfaceEvent(event_msg) = msg {
        // Sanity check: only store events for the subscribed interface
        if event_msg.interface_id != *interface_id {
            return;
        }

        let stored = StoredEvent::new(
            event_msg.event_id,
            event_msg.encrypted_event,
            event_msg.nonce,
        );

        match blob_store.store_event(*interface_id, &stored) {
            Ok(()) => {
                debug!(
                    interface = %short_hex(interface_id.as_bytes()),
                    event_id = ?stored.event_id,
                    "Stored gossip event"
                );
            }
            Err(e) => {
                warn!(
                    interface = %short_hex(interface_id.as_bytes()),
                    error = %e,
                    "Failed to store gossip event"
                );
            }
        }
    }
}

// ============================================================================
// Cleanup task
// ============================================================================

/// Run periodic cleanup of expired events
async fn run_cleanup(
    store: Arc<BlobStore>,
    tier_config: crate::config::TierConfig,
    interval_secs: u64,
    auth: Arc<AuthService>,
    shutdown: CancellationToken,
) {
    use indras_transport::protocol::StorageTier;

    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Clean each tier with its own TTL
                for (tier, ttl_days) in [
                    (StorageTier::Self_, tier_config.self_ttl_days),
                    (StorageTier::Connections, tier_config.connections_ttl_days),
                    (StorageTier::Public, tier_config.public_ttl_days),
                ] {
                    let max_age = Duration::from_secs(ttl_days * 86_400);
                    match store.cleanup_expired_tiered(tier, max_age) {
                        Ok(count) if count > 0 => {
                            info!(count, ?tier, "Cleanup removed expired events");
                        }
                        Ok(_) => {}
                        Err(e) => warn!(error = %e, ?tier, "Cleanup failed"),
                    }
                }

                // Sweep expired authentication sessions
                auth.sweep_expired_sessions();
            }
            _ = shutdown.cancelled() => {
                info!("Cleanup task shutting down");
                break;
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Derive a gossip `TopicId` from an `InterfaceId`
///
/// This MUST match `DiscoveryService::topic_for_interface` in indras-transport
/// exactly so the relay subscribes to the same topics that peers publish to.
fn topic_for_interface(interface_id: &InterfaceId) -> TopicId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras/realm/v1/");
    hasher.update(interface_id.as_bytes());
    let hash = hasher.finalize();
    TopicId::from(*hash.as_bytes())
}

/// Load a persistent 32-byte secret key from disk, or generate and save a new one
fn load_or_generate_key(path: &std::path::Path) -> RelayResult<SecretKey> {
    if path.exists() {
        let bytes = std::fs::read(path)?;
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(SecretKey::from_bytes(&arr))
        } else {
            Err(RelayError::Config(format!(
                "Invalid key file at {}: expected 32 bytes, got {}",
                path.display(),
                bytes.len()
            )))
        }
    } else {
        let key = SecretKey::generate(&mut rand::rng());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, key.to_bytes())?;
        info!("Generated new relay identity key at {}", path.display());
        Ok(key)
    }
}

/// Short hex display of a byte slice: first 2 and last 2 bytes
fn short_hex(bytes: &[u8]) -> String {
    if bytes.len() >= 4 {
        format!(
            "{:02x}{:02x}..{:02x}{:02x}",
            bytes[0],
            bytes[1],
            bytes[bytes.len() - 2],
            bytes[bytes.len() - 1]
        )
    } else {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
