//! PeeringRuntime — the core lifecycle manager.

use std::path::Path;
use std::sync::Arc;
use tokio::sync::{broadcast, watch, Mutex};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use indras_network::{IndrasNetwork, MemberId, Realm};

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
        }
    }

    /// Join the contacts realm (idempotent) and spawn background tasks.
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

        let mut handles = self.task_handles.lock().await;
        handles.extend([h1, h2, h3]);

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

        members
            .iter()
            .find(|m| m.id() != my_id)
            .map(|m| PeerInfo {
                member_id: m.id(),
                display_name: m.name(),
                connected_at: chrono::Utc::now().timestamp(),
            })
            .ok_or_else(|| PeeringError::Other("no remote peer in realm".into()))
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
    pub async fn shutdown(self) -> crate::Result<()> {
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
