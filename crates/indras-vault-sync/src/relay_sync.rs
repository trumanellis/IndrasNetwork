//! Relay-backed blob synchronization.
//!
//! Pushes file blobs to peer relay servers after local storage, and pulls
//! missing blobs from the local embedded relay or peer relays when
//! `SyncToDisk` cannot find them in the local vault blob store.
//!
//! ## Design
//!
//! - **Push**: stores blob in local relay (direct) + pushes to peer relays
//!   via fresh QUIC connections (on-demand, no stale sessions)
//! - **Pull**: reads from local relay blob store first, falls back to
//!   pulling from peer relays via QUIC

use indras_core::{EventId, InterfaceId};
use indras_relay::RelayService;
use indras_storage::BlobStore;
use indras_transport::protocol::StorageTier;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Maximum blob size we'll push through the relay (900 KB, leaving room
/// for framing overhead within the 1 MB wire limit).
const MAX_RELAY_BLOB_SIZE: usize = 900 * 1024;

/// Header: 32-byte BLAKE3 hash prepended to each relay blob event,
/// so the receiver can index by content hash.
const BLOB_HEADER_SIZE: usize = 32;

/// Timeout for individual QUIC relay operations.
const RELAY_OP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Derive a deterministic `vault-blobs` InterfaceId from the realm's ID.
pub fn vault_blob_interface_id(realm_id: &InterfaceId) -> InterfaceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vault-blobs-v1:");
    hasher.update(realm_id.as_bytes());
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Shared relay state for blob push/pull operations.
pub struct RelayBlobSync {
    /// Peer relay addresses (connect on-demand for each push/pull).
    peer_addrs: Mutex<Vec<iroh::EndpointAddr>>,
    /// Local relay service for direct blob store reads/writes.
    local_relay: Option<Arc<RelayService>>,
    /// The vault-blobs interface ID.
    interface_id: InterfaceId,
    /// Cursor: last event ID retrieved from local relay.
    last_local_event_id: Mutex<Option<EventId>>,
    /// Shared relay client for on-demand QUIC connections.
    relay_client: indras_transport::relay_client::RelayClient,
    /// Shared QUIC endpoint (one per process).
    shared_endpoint: iroh::Endpoint,
}

impl RelayBlobSync {
    /// Push a blob to local relay + all peer relays.
    pub async fn push_blob(
        &self,
        hash: &[u8; 32],
        data: &[u8],
    ) -> Result<(), String> {
        if data.len() > MAX_RELAY_BLOB_SIZE {
            debug!(
                size = data.len(),
                hash = %hex::encode(&hash[..6]),
                "Blob too large for relay, skipping"
            );
            return Ok(());
        }

        // Build payload: [32-byte hash][blob data]
        let mut payload = Vec::with_capacity(BLOB_HEADER_SIZE + data.len());
        payload.extend_from_slice(hash);
        payload.extend_from_slice(data);

        // Store in local relay (always works, no QUIC needed)
        if let Some(ref relay_service) = self.local_relay {
            let event_id = EventId::new(
                rand::random::<u64>(),
                chrono::Utc::now().timestamp_millis() as u64,
            );
            let stored = indras_transport::protocol::StoredEvent::new(
                event_id,
                payload.clone(),
                [0u8; 12],
            );
            let _ = relay_service
                .blob_store()
                .store_event_tiered(StorageTier::Public, self.interface_id, &stored);
        }

        // Push to peer relays via fresh QUIC connections
        let addrs = self.peer_addrs.lock().await.clone();
        for (i, addr) in addrs.iter().enumerate() {
            self.push_to_peer(addr, &payload, i).await;
        }

        Ok(())
    }

    /// Push blob payload to a single peer relay via a fresh QUIC connection.
    async fn push_to_peer(
        &self,
        addr: &iroh::EndpointAddr,
        payload: &[u8],
        index: usize,
    ) {
        let result = tokio::time::timeout(RELAY_OP_TIMEOUT, async {
            let mut session = self.relay_client.connect_with_endpoint(&self.shared_endpoint, addr.clone()).await?;
            session.authenticate().await?;
            session.register(vec![self.interface_id]).await?;
            session
                .store_event(StorageTier::Public, self.interface_id, payload.to_vec())
                .await
        })
        .await;

        match result {
            Ok(Ok(ack)) if ack.accepted => {
                debug!(relay = index, "Pushed blob to peer relay");
            }
            Ok(Ok(ack)) => {
                warn!(relay = index, reason = ?ack.reason, "Peer relay rejected blob");
            }
            Ok(Err(e)) => {
                warn!(relay = index, error = %e, "Failed to push to peer relay");
            }
            Err(_) => {
                warn!(relay = index, "Timed out pushing to peer relay");
            }
        }
    }

    /// Pull blobs from local relay, then peer relays as fallback.
    pub async fn pull_blobs(&self, vault_blob_store: &BlobStore) -> Result<usize, String> {
        // Try local relay first (direct, fast)
        let count = self.pull_from_local(vault_blob_store).await?;
        if count > 0 {
            return Ok(count);
        }

        // Fallback: try peer relays via QUIC
        self.pull_from_peers(vault_blob_store).await
    }

    /// Pull from local relay blob store (no QUIC).
    async fn pull_from_local(&self, vault_blob_store: &BlobStore) -> Result<usize, String> {
        let Some(ref relay_service) = self.local_relay else {
            return Ok(0);
        };

        let relay_blobs = relay_service.blob_store();
        let after = { *self.last_local_event_id.lock().await };

        let events = relay_blobs
            .events_after_tiered(StorageTier::Public, self.interface_id, after)
            .map_err(|e| e.to_string())?;

        let mut count = 0;
        let mut last_id = after;

        for event in &events {
            let data = &event.encrypted_event;
            if data.len() < BLOB_HEADER_SIZE {
                continue;
            }
            if let Ok(cr) = vault_blob_store.store(&data[BLOB_HEADER_SIZE..]).await {
                debug!(hash = %cr.short_hash(), "Pulled blob from local relay");
                count += 1;
            }
            last_id = Some(event.event_id);
        }

        if last_id != after {
            *self.last_local_event_id.lock().await = last_id;
        }
        if count > 0 {
            info!(count, "Pulled blobs from local relay");
        }

        Ok(count)
    }

    /// Pull from peer relays via fresh QUIC connections.
    async fn pull_from_peers(&self, vault_blob_store: &BlobStore) -> Result<usize, String> {
        let addrs = self.peer_addrs.lock().await.clone();
        for (i, addr) in addrs.iter().enumerate() {
            let result = tokio::time::timeout(RELAY_OP_TIMEOUT, async {
                let mut session = self
                    .relay_client
                    .connect_with_endpoint(&self.shared_endpoint, addr.clone())
                    .await?;
                session.authenticate().await?;
                session.register(vec![self.interface_id]).await?;
                session
                    .retrieve(self.interface_id, None, Some(StorageTier::Public))
                    .await
            })
            .await;

            let events = match result {
                Ok(Ok(delivery)) => delivery.events,
                _ => continue,
            };

            let mut count = 0;
            for event in &events {
                let data = &event.encrypted_event;
                if data.len() < BLOB_HEADER_SIZE {
                    continue;
                }
                if let Ok(cr) = vault_blob_store.store(&data[BLOB_HEADER_SIZE..]).await {
                    debug!(hash = %cr.short_hash(), relay = i, "Pulled blob from peer relay");
                    count += 1;
                }
            }

            if count > 0 {
                info!(count, relay = i, "Pulled blobs from peer relay");
                return Ok(count);
            }
        }

        Ok(0)
    }
}

/// Set up relay blob sync for a vault.
pub async fn connect_relays(
    network: &indras_network::IndrasNetwork,
    peer_addr: Option<iroh::EndpointAddr>,
    realm_id: InterfaceId,
) -> Option<Arc<RelayBlobSync>> {
    let interface_id = vault_blob_interface_id(&realm_id);
    let local_relay = network.relay_service().cloned();

    let peer_addrs = match peer_addr {
        Some(addr) => vec![addr],
        None => vec![],
    };

    // We need at least a local relay or a peer address to be useful
    if local_relay.is_none() && peer_addrs.is_empty() {
        return None;
    }

    // Get or create the shared relay client + endpoint
    let (relay_client, shared_endpoint) = match network.relay_blob_endpoint().await {
        Ok((c, e)) => (c.clone(), e.clone()),
        Err(e) => {
            warn!(error = %e, "Failed to create relay blob endpoint");
            return None;
        }
    };

    info!(
        local_relay = local_relay.is_some(),
        peer_addrs = peer_addrs.len(),
        "Relay blob sync ready"
    );

    Some(Arc::new(RelayBlobSync {
        peer_addrs: Mutex::new(peer_addrs),
        local_relay,
        interface_id,
        last_local_event_id: Mutex::new(None),
        relay_client,
        shared_endpoint,
    }))
}

/// Add a peer relay address to an existing RelayBlobSync.
pub async fn add_peer_relay(
    relay_sync: &RelayBlobSync,
    _network: &indras_network::IndrasNetwork,
    peer_addr: iroh::EndpointAddr,
) -> bool {
    relay_sync.peer_addrs.lock().await.push(peer_addr);
    true
}
