//! PeeringRuntime — the core lifecycle manager.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, watch, Mutex, Notify};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use indras_network::contacts::{ContactEntry, ContactStatus};
use indras_network::{IndrasNetwork, MemberId, Realm, RealmId};
use indras_sync_engine::sentiment::SentimentView;

use crate::config::PeeringConfig;
use crate::error::PeeringError;
use crate::event::{PeerEvent, PeerInfo};
use crate::tasks;

/// Central peering runtime — owns background tasks and exposes peer state.
///
/// Created via [`boot`](Self::boot) (standalone) or [`attach`](Self::attach) (embedded).
/// Provides reactive peer tracking, event subscription, and graceful shutdown.
pub struct PeeringRuntime {
    network: Arc<IndrasNetwork>,
    /// Whether this runtime created (and therefore owns) the network.
    owns_network: bool,
    peers_tx: watch::Sender<Vec<PeerInfo>>,
    peers_rx: watch::Receiver<Vec<PeerInfo>>,
    event_tx: broadcast::Sender<PeerEvent>,
    cancel: CancellationToken,
    /// Handles for background tasks so we can join them on shutdown.
    task_handles: Mutex<Vec<JoinHandle<()>>>,
    /// Guard against double-shutdown.
    shutdown_called: AtomicBool,
    /// Notify the contact poller to run an immediate poll cycle.
    poll_notify: Arc<Notify>,
}

impl PeeringRuntime {
    /// Internal constructor — does NOT spawn tasks yet.
    fn new(network: Arc<IndrasNetwork>, owns_network: bool) -> Self {
        let (peers_tx, peers_rx) = watch::channel(Vec::new());
        let (event_tx, _) = broadcast::channel(256);
        let cancel = CancellationToken::new();

        Self {
            network,
            owns_network,
            peers_tx,
            peers_rx,
            event_tx,
            cancel,
            task_handles: Mutex::new(Vec::new()),
            shutdown_called: AtomicBool::new(false),
            poll_notify: Arc::new(Notify::new()),
        }
    }

    /// Join the contacts realm (idempotent) and spawn background tasks.
    ///
    /// **Note**: This calls `network.join_contacts_realm()` internally, so callers
    /// do NOT need to call it separately before `boot()` / `attach()`.
    async fn start_tasks(&self, config: &PeeringConfig) -> crate::Result<()> {
        // Ensure contacts realm is joined
        self.network.join_contacts_realm().await?;

        // Spawn background tasks and collect handles
        let h1 = tasks::spawn_contact_poller(
            Arc::clone(&self.network),
            self.peers_tx.clone(),
            self.event_tx.clone(),
            self.cancel.clone(),
            config.poll_interval,
            Arc::clone(&self.poll_notify),
        );
        let h2 = tasks::spawn_event_forwarder(
            Arc::clone(&self.network),
            self.event_tx.clone(),
            self.cancel.clone(),
        );
        let h3 = tasks::spawn_periodic_saver(
            Arc::clone(&self.network),
            self.event_tx.clone(),
            self.cancel.clone(),
            config.save_interval,
        );
        let h4 = tasks::spawn_task_supervisor(
            self.task_handles.lock().await.len(), // offset for existing handles
            self.event_tx.clone(),
            self.cancel.clone(),
        );

        let mut handles = self.task_handles.lock().await;
        handles.extend([h1, h2, h3]);

        // Supervisor watches the other handles, so store it separately after
        // we know the count. It monitors indices 0..handles.len().
        handles.push(h4);

        Ok(())
    }

    // ── Construction ─────────────────────────────────────────────────

    /// Check whether an identity already exists on disk.
    pub fn is_first_run(data_dir: impl AsRef<Path>) -> bool {
        IndrasNetwork::is_first_run(data_dir)
    }

    /// Create a brand-new identity, start the network, and spawn tasks.
    pub async fn create(
        name: &str,
        pass_story: Option<indras_crypto::PassStory>,
        config: PeeringConfig,
    ) -> crate::Result<Self> {
        let mut builder = IndrasNetwork::builder()
            .data_dir(&config.data_dir)
            .display_name(name);

        if let Some(story) = pass_story {
            builder = builder.pass_story(story);
        }

        let net = builder.build().await?;
        net.start().await?;
        let net = Arc::new(net);

        let runtime = Self::new(net, true);
        runtime.start_tasks(&config).await?;
        Ok(runtime)
    }

    /// Boot from an existing identity on disk, start the network, and spawn tasks.
    pub async fn boot(config: PeeringConfig) -> crate::Result<Self> {
        let net = IndrasNetwork::new(&config.data_dir).await?;
        net.start().await?;
        let net = Arc::new(net);

        let runtime = Self::new(net, true);
        runtime.start_tasks(&config).await?;
        Ok(runtime)
    }

    /// Attach to an already-started `IndrasNetwork` and spawn tasks.
    ///
    /// The runtime does **not** own the network — `shutdown()` will not stop it.
    ///
    /// **Note**: Callers do NOT need to call `network.join_contacts_realm()` before
    /// this — `start_tasks()` handles it internally.
    pub async fn attach(
        network: Arc<IndrasNetwork>,
        config: PeeringConfig,
    ) -> crate::Result<Self> {
        let runtime = Self::new(network, false);
        runtime.start_tasks(&config).await?;
        Ok(runtime)
    }

    // ── Peer operations ──────────────────────────────────────────────

    /// Connect to a peer by identity code/URI and return the DM realm + peer info.
    pub async fn connect_by_code(&self, code: &str) -> crate::Result<(Realm, PeerInfo)> {
        let realm = self.network.connect_by_code(code).await?;
        let peer = self.extract_peer(&realm).await?;

        let _ = self.event_tx.send(PeerEvent::ConversationOpened {
            realm_id: realm.id(),
            peer: peer.clone(),
        });

        Ok((realm, peer))
    }

    /// Connect to a peer by raw `MemberId` and return the DM realm + peer info.
    pub async fn connect(&self, peer_id: MemberId) -> crate::Result<(Realm, PeerInfo)> {
        let realm = self.network.connect(peer_id).await?;
        let peer = self.extract_peer(&realm).await?;

        let _ = self.event_tx.send(PeerEvent::ConversationOpened {
            realm_id: realm.id(),
            peer: peer.clone(),
        });

        Ok((realm, peer))
    }

    /// Extract the remote peer from a DM realm's member list.
    async fn extract_peer(&self, realm: &Realm) -> crate::Result<PeerInfo> {
        let members = realm.member_list().await?;
        let my_id = self.network.id();

        let member = members
            .iter()
            .find(|m| m.id() != my_id)
            .ok_or(PeeringError::NoPeerInRealm)?;

        // Try to enrich with contact entry data (async-safe)
        let (sentiment, status) = if let Some(cr) = self.network.contacts_realm().await {
            match cr.get_contact_entry_async(&member.id()).await {
                Some(entry) => (entry.sentiment, entry.status),
                None => (0, ContactStatus::default()),
            }
        } else {
            (0, ContactStatus::default())
        };

        Ok(PeerInfo {
            member_id: member.id(),
            display_name: member.name(),
            connected_at: chrono::Utc::now().timestamp(),
            sentiment,
            status,
        })
    }

    // ── Contact management ───────────────────────────────────────────

    /// Block a contact: remove from contacts and leave all shared realms.
    ///
    /// Emits [`PeerEvent::PeerBlocked`] on success.
    /// Returns the list of realm IDs that were left as part of the cascade.
    pub async fn block_contact(&self, peer_id: MemberId) -> crate::Result<Vec<RealmId>> {
        let left_realms = self.network.block_contact(&peer_id).await?;

        let _ = self.event_tx.send(PeerEvent::PeerBlocked {
            member_id: peer_id,
            left_realms: left_realms.clone(),
        });

        Ok(left_realms)
    }

    /// Remove a contact without the realm cascade.
    ///
    /// Returns `true` if the contact was found and removed.
    pub async fn remove_contact(&self, peer_id: MemberId) -> crate::Result<bool> {
        let contacts = self.contacts_realm().await?;
        let removed = contacts.remove_contact(&peer_id).await?;

        if removed {
            let _ = self.event_tx.send(PeerEvent::PeerDisconnected {
                member_id: peer_id,
            });
        }

        Ok(removed)
    }

    /// Update sentiment toward a contact (-1, 0, or +1). Clamped to [-1, 1].
    ///
    /// Emits [`PeerEvent::SentimentChanged`] on success.
    pub async fn update_sentiment(
        &self,
        peer_id: MemberId,
        sentiment: i8,
    ) -> crate::Result<()> {
        let clamped = sentiment.clamp(-1, 1);
        let contacts = self.contacts_realm().await?;
        contacts.update_sentiment(&peer_id, clamped).await?;

        let _ = self.event_tx.send(PeerEvent::SentimentChanged {
            member_id: peer_id,
            sentiment: clamped,
        });

        Ok(())
    }

    /// Get sentiment toward a specific contact.
    pub async fn get_sentiment(&self, peer_id: MemberId) -> crate::Result<Option<i8>> {
        let contacts = self.contacts_realm().await?;
        Ok(contacts.get_sentiment_async(&peer_id).await)
    }

    /// Set whether sentiment toward a contact is relayable to second-degree peers.
    pub async fn set_relayable(
        &self,
        peer_id: MemberId,
        relayable: bool,
    ) -> crate::Result<()> {
        let contacts = self.contacts_realm().await?;
        contacts.set_relayable(&peer_id, relayable).await?;
        Ok(())
    }

    /// Get the full contact entry (sentiment, status, relayable, display_name).
    pub async fn get_contact_entry(
        &self,
        peer_id: MemberId,
    ) -> crate::Result<Option<ContactEntry>> {
        let contacts = self.contacts_realm().await?;
        Ok(contacts.get_contact_entry_async(&peer_id).await)
    }

    /// Build an aggregated sentiment view about a member from direct + relayed signals.
    ///
    /// Currently returns direct signals only (relayed signals require relay document sync).
    pub async fn sentiment_view(&self, about: MemberId) -> crate::Result<SentimentView> {
        let contacts = self.contacts_realm().await?;
        let direct = contacts.contacts_with_sentiment();

        // Filter to only the entries about the target member
        let direct_about: Vec<(MemberId, i8)> = direct
            .into_iter()
            .filter(|(id, _)| *id == about)
            .collect();

        Ok(SentimentView {
            direct: direct_about,
            relayed: vec![],
        })
    }

    /// Helper: get the contacts realm or return an error.
    async fn contacts_realm(&self) -> crate::Result<indras_network::ContactsRealm> {
        self.network
            .contacts_realm()
            .await
            .ok_or(PeeringError::ContactsRealmNotJoined)
    }

    // ── State access ─────────────────────────────────────────────────

    /// Snapshot of currently known peers.
    pub fn peers(&self) -> Vec<PeerInfo> {
        self.peers_rx.borrow().clone()
    }

    /// Reactive watcher for peer list changes.
    pub fn watch_peers(&self) -> watch::Receiver<Vec<PeerInfo>> {
        self.peers_rx.clone()
    }

    /// Subscribe to all peering events (peers, conversations, saves, etc.).
    pub fn subscribe(&self) -> broadcast::Receiver<PeerEvent> {
        self.event_tx.subscribe()
    }

    /// Atomically subscribe to events AND get the current peer snapshot.
    ///
    /// This avoids the race where peers connect between `subscribe()` and `peers()`:
    /// the receiver is created first, so any changes after the snapshot will arrive
    /// as events.
    pub fn subscribe_with_snapshot(&self) -> (broadcast::Receiver<PeerEvent>, Vec<PeerInfo>) {
        let rx = self.event_tx.subscribe();
        let peers = self.peers_rx.borrow().clone();
        (rx, peers)
    }

    /// Trigger an immediate contact poll cycle (instead of waiting for the next tick).
    pub fn refresh_peers(&self) {
        self.poll_notify.notify_one();
    }

    /// Access the underlying network.
    pub fn network(&self) -> &Arc<IndrasNetwork> {
        &self.network
    }

    /// The node's display name, if set.
    pub fn display_name(&self) -> Option<&str> {
        self.network.display_name()
    }

    /// Bech32m-encoded identity URI (includes display name query param).
    pub fn identity_uri(&self) -> String {
        self.network.identity_uri()
    }

    /// Bech32m-encoded identity code (no display name).
    pub fn identity_code(&self) -> String {
        self.network.identity_code()
    }

    /// This node's member ID.
    pub fn id(&self) -> MemberId {
        self.network.id()
    }

    // ── Lifecycle ────────────────────────────────────────────────────

    /// Gracefully shut down: cancel tasks, save world view, optionally stop network.
    ///
    /// Safe to call multiple times — subsequent calls return `AlreadyShutDown`.
    pub async fn shutdown(&self) -> crate::Result<()> {
        if self.shutdown_called.swap(true, Ordering::SeqCst) {
            return Err(PeeringError::AlreadyShutDown);
        }

        self.cancel.cancel();

        // Wait for all background tasks to finish (releases Arc<IndrasNetwork> clones)
        let mut handles = self.task_handles.lock().await;
        for handle in handles.drain(..) {
            let _ = handle.await;
        }
        drop(handles);

        // Best-effort save
        if let Err(e) = self.network.save_world_view().await {
            tracing::warn!(error = %e, "failed to save world view on shutdown");
        }

        // Only stop the network if we created it
        if self.owns_network {
            self.network.stop().await?;
        }

        Ok(())
    }
}
