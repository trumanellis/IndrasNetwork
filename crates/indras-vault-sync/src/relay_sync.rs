//! Relay-backed blob synchronization.
//!
//! Pushes file blobs to peer relay servers after local storage, and pulls
//! missing blobs from the local embedded relay when `SyncToDisk` cannot
//! find them in the local vault blob store.
//!
//! Each vault gets a deterministic `vault-blobs` InterfaceId derived from
//! the realm's interface ID, so all peers in the same vault exchange blobs
//! through the same relay interface.
//!
//! ## Design
//!
//! iroh does not support connecting to yourself, so each peer:
//! - **Pushes** blobs to the peer's relay via QUIC (`RelaySession`)
//! - **Pulls** blobs from its own embedded relay's blob store directly
//!   (no QUIC needed — the peer already pushed there)

use indras_core::{EventId, InterfaceId};
use indras_relay::RelayService;
use indras_storage::BlobStore;
use indras_transport::protocol::StorageTier;
use indras_transport::relay_client::RelaySession;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Maximum blob size we'll push through the relay (900 KB, leaving room
/// for framing overhead within the 1 MB wire limit).
const MAX_RELAY_BLOB_SIZE: usize = 900 * 1024;

/// Header: 32-byte BLAKE3 hash prepended to each relay blob event,
/// so the receiver can index by content hash.
const BLOB_HEADER_SIZE: usize = 32;

/// Derive a deterministic `vault-blobs` InterfaceId from the realm's ID.
///
/// Both peers compute the same interface ID for the same vault,
/// enabling them to exchange blobs through a shared relay interface.
pub fn vault_blob_interface_id(realm_id: &InterfaceId) -> InterfaceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vault-blobs-v1:");
    hasher.update(realm_id.as_bytes());
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Shared relay state for blob push/pull operations.
///
/// Pushes blobs to peer relays via QUIC sessions, and pulls blobs
/// from the local embedded relay's blob store directly.
pub struct RelayBlobSync {
    /// QUIC sessions to peer relays (push targets).
    peer_sessions: Vec<PeerRelay>,
    /// Local relay service for direct blob store reads (pull source).
    local_relay: Option<Arc<RelayService>>,
    /// The vault-blobs interface ID.
    interface_id: InterfaceId,
    /// Cursor: last event ID retrieved from local relay.
    last_local_event_id: Mutex<Option<EventId>>,
}

/// A QUIC connection to a peer's relay.
struct PeerRelay {
    session: Mutex<RelaySession>,
    label: String,
}

impl RelayBlobSync {
    /// Push a blob to all connected peer relays.
    ///
    /// Prepends the 32-byte BLAKE3 hash as a header so receivers can
    /// index by content hash on retrieval.
    ///
    /// Silently skips blobs larger than `MAX_RELAY_BLOB_SIZE`.
    pub async fn push_blob(&self, hash: &[u8; 32], data: &[u8]) -> Result<(), String> {
        if data.len() > MAX_RELAY_BLOB_SIZE {
            debug!(
                size = data.len(),
                max = MAX_RELAY_BLOB_SIZE,
                hash = %hex::encode(&hash[..6]),
                "Blob too large for relay, skipping"
            );
            return Ok(());
        }

        // Build payload once: [32-byte hash][blob data]
        let mut payload = Vec::with_capacity(BLOB_HEADER_SIZE + data.len());
        payload.extend_from_slice(hash);
        payload.extend_from_slice(data);

        for peer in &self.peer_sessions {
            let mut session = peer.session.lock().await;
            match session
                .store_event(StorageTier::Public, self.interface_id, payload.clone())
                .await
            {
                Ok(ack) => {
                    if ack.accepted {
                        debug!(
                            hash = %hex::encode(&hash[..6]),
                            size = data.len(),
                            relay = %peer.label,
                            "Pushed blob to peer relay"
                        );
                    } else {
                        warn!(
                            hash = %hex::encode(&hash[..6]),
                            reason = ?ack.reason,
                            relay = %peer.label,
                            "Peer relay rejected blob store"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        hash = %hex::encode(&hash[..6]),
                        error = %e,
                        relay = %peer.label,
                        "Failed to push blob to peer relay"
                    );
                }
            }
        }

        Ok(())
    }

    /// Pull new blobs from the local embedded relay into the vault blob store.
    ///
    /// Reads directly from the relay's blob store (no QUIC) since peers
    /// push their blobs to our relay. Returns the number of new blobs stored.
    pub async fn pull_blobs(&self, vault_blob_store: &BlobStore) -> Result<usize, String> {
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
                debug!("Relay event too small to contain blob header, skipping");
                continue;
            }

            let blob_data = &data[BLOB_HEADER_SIZE..];

            // Store in vault's local blob store (deduplicates by content hash)
            match vault_blob_store.store(blob_data).await {
                Ok(content_ref) => {
                    debug!(
                        hash = %content_ref.short_hash(),
                        size = blob_data.len(),
                        "Pulled blob from local relay"
                    );
                    count += 1;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to store pulled blob locally");
                }
            }

            last_id = Some(event.event_id);
        }

        // Update cursor
        if last_id != after {
            *self.last_local_event_id.lock().await = last_id;
        }

        if count > 0 {
            info!(count, "Pulled blobs from local relay");
        }

        Ok(count)
    }
}

/// Set up relay blob sync for a vault.
///
/// - Connects to peer relay(s) via QUIC for pushing blobs
/// - Uses the local embedded relay service for pulling blobs
///
/// Returns `None` if no relay connections succeed (vault still works
/// without relay, just no remote blob replication).
pub async fn connect_relays(
    network: &indras_network::IndrasNetwork,
    peer_addr: Option<iroh::EndpointAddr>,
    realm_id: InterfaceId,
) -> Option<Arc<RelayBlobSync>> {
    let interface_id = vault_blob_interface_id(&realm_id);
    let local_relay = network.relay_service().cloned();

    let mut peer_sessions = Vec::new();

    // Connect to peer relay via QUIC (for pushing our blobs to them)
    if let Some(addr) = peer_addr {
        if let Some(session) = setup_peer_session(network, addr, interface_id, "peer").await {
            peer_sessions.push(PeerRelay {
                session: Mutex::new(session),
                label: "peer".to_string(),
            });
        }
    }

    // We need at least a local relay or a peer session to be useful
    if local_relay.is_none() && peer_sessions.is_empty() {
        return None;
    }

    info!(
        local_relay = local_relay.is_some(),
        peer_relays = peer_sessions.len(),
        "Relay blob sync ready"
    );

    Some(Arc::new(RelayBlobSync {
        peer_sessions,
        local_relay,
        interface_id,
        last_local_event_id: Mutex::new(None),
    }))
}

/// Authenticate and register a QUIC relay session with a peer's relay.
async fn setup_peer_session(
    network: &indras_network::IndrasNetwork,
    relay_addr: iroh::EndpointAddr,
    interface_id: InterfaceId,
    label: &str,
) -> Option<RelaySession> {
    // Use a fresh transport key for the relay client connection.
    // Reusing the node's own key causes issues when both nodes share
    // the same process (the endpoint can't multiplex two connections
    // with the same key).
    let signing_key = {
        let secret = network.node().secret_key();
        let bytes = secret.to_bytes();
        ed25519_dalek::SigningKey::from_bytes(&bytes)
    };
    let transport_secret = iroh::SecretKey::generate(&mut rand::rng());
    let client = indras_transport::relay_client::RelayClient::new(signing_key, transport_secret);

    let connect_result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.connect(relay_addr),
    )
    .await;

    let mut session = match connect_result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            warn!(error = %e, relay = label, "Failed to connect to peer relay");
            return None;
        }
        Err(_) => {
            warn!(relay = label, "Timed out connecting to peer relay");
            return None;
        }
    };

    match session.authenticate().await {
        Ok(ack) if ack.authenticated => {
            debug!(tiers = ?ack.granted_tiers, relay = label, "Peer relay authenticated");
        }
        Ok(_) => {
            warn!(relay = label, "Peer relay rejected authentication");
            return None;
        }
        Err(e) => {
            warn!(error = %e, relay = label, "Peer relay authentication failed");
            return None;
        }
    }

    match session.register(vec![interface_id]).await {
        Ok(ack) => {
            if ack.accepted.is_empty() {
                warn!(relay = label, "Peer relay rejected interface registration");
                return None;
            }
            debug!(relay = label, "Registered vault-blobs interface with peer relay");
        }
        Err(e) => {
            warn!(error = %e, relay = label, "Failed to register interface with peer relay");
            return None;
        }
    }

    Some(session)
}
