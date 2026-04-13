//! Relay-backed blob synchronization with direct P2P transport.
//!
//! ## Design
//!
//! **Push path** (writer):
//! 1. Store blob in local relay blob store (for offline peers)
//! 2. Broadcast blob via realm's existing QUIC transport (`send_message`)
//!    to all connected peers — no extra connections needed
//!
//! **Pull path** (reader):
//! 1. Background listener receives blob broadcasts via realm events
//!    → stores in local vault BlobStore immediately
//! 2. SyncToDisk finds blob locally when processing CRDT change
//! 3. Fallback: pull from local relay blob store (for offline catch-up)

use indras_core::{EventId, InterfaceId};
use indras_node::IndrasNode;
use indras_relay::RelayService;
use indras_storage::BlobStore;
use indras_transport::protocol::StorageTier;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Maximum blob size for relay + transport (900 KB, under 1 MB wire limit).
const MAX_BLOB_SIZE: usize = 900 * 1024;

/// Header: 32-byte BLAKE3 hash prepended to each blob payload.
const BLOB_HEADER_SIZE: usize = 32;

/// Magic prefix to distinguish blob events from chat messages on the realm interface.
const BLOB_MAGIC: &[u8; 4] = b"BLOB";

/// Derive a deterministic `vault-blobs` InterfaceId from the realm's ID.
pub fn vault_blob_interface_id(realm_id: &InterfaceId) -> InterfaceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vault-blobs-v1:");
    hasher.update(realm_id.as_bytes());
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Shared relay state for blob push/pull operations.
pub struct RelayBlobSync {
    /// The realm's interface ID (for sending blobs via transport).
    realm_id: InterfaceId,
    /// Reference to the node (for send_message).
    node: Arc<IndrasNode>,
    /// Local relay service for direct blob store reads/writes.
    local_relay: Option<Arc<RelayService>>,
    /// The vault-blobs interface ID (for relay blob store keying).
    blob_interface_id: InterfaceId,
    /// Cursor: last event ID retrieved from local relay.
    last_local_event_id: Mutex<Option<EventId>>,
    /// Background blob listener task handle.
    _listener: Option<JoinHandle<()>>,
}

impl RelayBlobSync {
    /// Push a blob to all vault peers via direct transport + local relay.
    ///
    /// Prepends a 4-byte magic + 32-byte BLAKE3 hash header so receivers
    /// can identify and index blob events.
    pub async fn push_blob(&self, hash: &[u8; 32], data: &[u8]) -> Result<(), String> {
        if data.len() > MAX_BLOB_SIZE {
            debug!(
                size = data.len(),
                hash = %hex::encode(&hash[..6]),
                "Blob too large for transport, skipping"
            );
            return Ok(());
        }

        // Build payload: [BLOB magic][32-byte hash][blob data]
        let mut payload = Vec::with_capacity(4 + BLOB_HEADER_SIZE + data.len());
        payload.extend_from_slice(BLOB_MAGIC);
        payload.extend_from_slice(hash);
        payload.extend_from_slice(data);

        // Store in local relay blob store (for offline peers to pull later)
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
                .store_event_tiered(StorageTier::Public, self.blob_interface_id, &stored);
        }

        // Broadcast via realm's existing transport to all connected peers.
        // This uses the same QUIC connections already established for CRDT sync —
        // no extra connections, no auth handshakes, no stale sessions.
        match self.node.send_message(&self.realm_id, payload).await {
            Ok(_) => {
                info!(
                    hash = %hex::encode(&hash[..6]),
                    size = data.len(),
                    realm = %hex::encode(&self.realm_id.as_bytes()[..6]),
                    "Broadcast blob via transport"
                );
            }
            Err(e) => {
                // Non-fatal: blob is in local relay, peers can pull later
                warn!(
                    hash = %hex::encode(&hash[..6]),
                    error = %e,
                    "Failed to broadcast blob via transport (stored in relay)"
                );
            }
        }

        Ok(())
    }

    /// Pull blobs from the local relay blob store (for offline catch-up).
    pub async fn pull_blobs(&self, vault_blob_store: &BlobStore) -> Result<usize, String> {
        let Some(ref relay_service) = self.local_relay else {
            return Ok(0);
        };

        let relay_blobs = relay_service.blob_store();
        let after = { *self.last_local_event_id.lock().await };

        let events = relay_blobs
            .events_after_tiered(StorageTier::Public, self.blob_interface_id, after)
            .map_err(|e| e.to_string())?;

        let mut count = 0;
        let mut last_id = after;

        for event in &events {
            let data = &event.encrypted_event;
            // Skip if too small or not a blob event
            if data.len() < 4 + BLOB_HEADER_SIZE || &data[..4] != BLOB_MAGIC {
                continue;
            }
            let blob_data = &data[4 + BLOB_HEADER_SIZE..];

            if let Ok(cr) = vault_blob_store.store(blob_data).await {
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
}

/// Start a background listener that receives blob broadcasts via realm
/// events and stores them in the local vault BlobStore.
fn start_blob_listener(
    node: &IndrasNode,
    realm_id: InterfaceId,
    vault_blob_store: Arc<BlobStore>,
) -> Option<JoinHandle<()>> {
    let mut rx: tokio::sync::broadcast::Receiver<indras_node::ReceivedEvent> =
        match node.events(&realm_id) {
            Ok(rx) => rx,
            Err(e) => {
                eprintln!("[blob_listener] Failed to subscribe to realm events: {e}");
                return None;
            }
        };

    debug!(realm = %hex::encode(&realm_id.as_bytes()[..6]), "Blob listener started");

    Some(tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Extract content from Message events
                    let content = match &event.event {
                        indras_core::InterfaceEvent::Message { content, .. } => content,
                        _ => continue,
                    };

                    // Check for blob magic prefix
                    if content.len() < 4 + BLOB_HEADER_SIZE || &content[..4] != BLOB_MAGIC {
                        continue;
                    }

                    let blob_data = &content[4 + BLOB_HEADER_SIZE..];
                    if let Ok(cr) = vault_blob_store.store(blob_data).await {
                        info!(hash = %cr.short_hash(), realm = %hex::encode(&realm_id.as_bytes()[..6]), "Received blob via transport");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Blob listener lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }))
}

/// Set up relay blob sync for a vault.
///
/// Uses the realm's existing transport for blob broadcast (no extra connections).
/// Local relay blob store provides offline catch-up.
pub async fn connect_relays(
    network: &indras_network::IndrasNetwork,
    node: Arc<IndrasNode>,
    _peer_addr: Option<iroh::EndpointAddr>,
    realm_id: InterfaceId,
) -> Option<Arc<RelayBlobSync>> {
    let blob_interface_id = vault_blob_interface_id(&realm_id);
    let local_relay = network.relay_service().cloned();

    info!("Relay blob sync ready (transport-based)");

    Some(Arc::new(RelayBlobSync {
        realm_id,
        node,
        local_relay,
        blob_interface_id,
        last_local_event_id: Mutex::new(None),
        _listener: None, // Started later when blob_store is available
    }))
}

/// Start the background blob listener for a vault.
///
/// Must be called after the vault's BlobStore is created.
pub fn start_listener(
    relay_sync: &mut Arc<RelayBlobSync>,
    vault_blob_store: Arc<BlobStore>,
) {
    let listener = start_blob_listener(&relay_sync.node, relay_sync.realm_id, vault_blob_store);
    // Safety: we're the only holder at this point (during Vault::setup)
    if let Some(sync) = Arc::get_mut(relay_sync) {
        sync._listener = listener;
    }
}

/// Start the blob listener as a detached task (doesn't require &mut Arc).
pub fn start_listener_spawned(
    relay_sync: &RelayBlobSync,
    vault_blob_store: Arc<BlobStore>,
) {
    start_blob_listener(&relay_sync.node, relay_sync.realm_id, vault_blob_store);
}

/// Add a peer relay address (no-op in transport-based design — peers
/// receive blobs automatically via the realm's existing connections).
pub async fn add_peer_relay(
    _relay_sync: &RelayBlobSync,
    _network: &indras_network::IndrasNetwork,
    _peer_addr: iroh::EndpointAddr,
) -> bool {
    // No-op: transport-based blob sync doesn't need per-peer relay connections.
    // Peers receive blobs via the realm's existing QUIC transport.
    true
}
