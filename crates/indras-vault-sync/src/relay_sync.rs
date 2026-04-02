//! Relay-backed blob synchronization.
//!
//! Pushes file blobs to relay servers after local storage, and pulls
//! missing blobs from relays when `SyncToDisk` cannot find them locally.
//!
//! Each vault gets a deterministic `vault-blobs` InterfaceId derived from
//! the realm's interface ID, so all peers in the same vault exchange blobs
//! through the same relay interface.
//!
//! Both peers connect to each other's relays: each pushes to its own relay
//! and pulls from all connected relays, ensuring bidirectional blob flow.

use indras_core::{EventId, InterfaceId};
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

/// A single relay connection with its own cursor for incremental retrieval.
struct RelayLink {
    session: Mutex<RelaySession>,
    /// Cursor: last retrieved event ID (for incremental pulls).
    last_event_id: Mutex<Option<EventId>>,
    /// Label for logging (e.g. "own" or "peer").
    label: String,
}

/// Shared relay state for blob push/pull operations.
///
/// Manages connections to multiple relays (own + peers). Pushes blobs
/// to the local relay and pulls from all connected relays.
pub struct RelayBlobSync {
    /// Connected relays. First entry is the "own" relay (push target).
    relays: Vec<RelayLink>,
    /// The vault-blobs interface ID (same for all relays in this vault).
    interface_id: InterfaceId,
}

impl RelayBlobSync {
    /// Create a new relay blob sync with an initial (own) relay session.
    fn new(session: RelaySession, interface_id: InterfaceId, label: &str) -> Self {
        Self {
            relays: vec![RelayLink {
                session: Mutex::new(session),
                last_event_id: Mutex::new(None),
                label: label.to_string(),
            }],
            interface_id,
        }
    }

    /// Add an additional relay connection (e.g. a peer's relay).
    fn add_relay(&mut self, session: RelaySession, label: &str) {
        self.relays.push(RelayLink {
            session: Mutex::new(session),
            last_event_id: Mutex::new(None),
            label: label.to_string(),
        });
    }

    /// Push a blob to ALL connected relays.
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

        for link in &self.relays {
            let mut session = link.session.lock().await;
            match session
                .store_event(StorageTier::Public, self.interface_id, payload.clone())
                .await
            {
                Ok(ack) => {
                    if ack.accepted {
                        debug!(
                            hash = %hex::encode(&hash[..6]),
                            size = data.len(),
                            relay = %link.label,
                            "Pushed blob to relay"
                        );
                    } else {
                        warn!(
                            hash = %hex::encode(&hash[..6]),
                            reason = ?ack.reason,
                            relay = %link.label,
                            "Relay rejected blob store"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        hash = %hex::encode(&hash[..6]),
                        error = %e,
                        relay = %link.label,
                        "Failed to push blob to relay"
                    );
                }
            }
        }

        Ok(())
    }

    /// Pull new blobs from ALL connected relays into the local blob store.
    ///
    /// Retrieves events after each relay's cursor, parses the hash header,
    /// and stores each blob locally. Returns the total number of new blobs.
    pub async fn pull_blobs(&self, blob_store: &BlobStore) -> Result<usize, String> {
        let mut total = 0;
        for link in &self.relays {
            match Self::pull_from_link(link, self.interface_id, blob_store).await {
                Ok(n) => total += n,
                Err(e) => {
                    warn!(
                        relay = %link.label,
                        error = %e,
                        "Failed to pull blobs from relay"
                    );
                }
            }
        }
        Ok(total)
    }

    /// Pull blobs from a single relay link.
    async fn pull_from_link(
        link: &RelayLink,
        interface_id: InterfaceId,
        blob_store: &BlobStore,
    ) -> Result<usize, String> {
        let after = { *link.last_event_id.lock().await };

        let mut session = link.session.lock().await;
        let delivery = session
            .retrieve(interface_id, after, Some(StorageTier::Public))
            .await
            .map_err(|e| e.to_string())?;
        drop(session);

        let mut count = 0;
        let mut last_id = after;

        for event in &delivery.events {
            let data = &event.encrypted_event;
            if data.len() < BLOB_HEADER_SIZE {
                debug!("Relay event too small to contain blob header, skipping");
                continue;
            }

            let blob_data = &data[BLOB_HEADER_SIZE..];

            // Store in local blob store (deduplicates by content hash)
            match blob_store.store(blob_data).await {
                Ok(content_ref) => {
                    debug!(
                        hash = %content_ref.short_hash(),
                        size = blob_data.len(),
                        relay = %link.label,
                        "Pulled blob from relay"
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
            *link.last_event_id.lock().await = last_id;
        }

        if count > 0 {
            info!(count, relay = %link.label, "Pulled blobs from relay");
        }

        // If there are more pages, recurse
        if delivery.has_more {
            count += Box::pin(Self::pull_from_link(link, interface_id, blob_store)).await?;
        }

        Ok(count)
    }
}

/// Authenticate and register a relay session for vault blob sync.
///
/// Returns the authenticated session, or `None` if connection/auth fails.
async fn setup_relay_session(
    network: &indras_network::IndrasNetwork,
    relay_addr: iroh::EndpointAddr,
    interface_id: InterfaceId,
    label: &str,
) -> Option<RelaySession> {
    let client = network.relay_client();

    let mut session = match client.connect(relay_addr).await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, relay = label, "Failed to connect to relay for blob sync");
            return None;
        }
    };

    match session.authenticate().await {
        Ok(ack) if ack.authenticated => {
            debug!(tiers = ?ack.granted_tiers, relay = label, "Relay authenticated for blob sync");
        }
        Ok(_) => {
            warn!(relay = label, "Relay rejected authentication for blob sync");
            return None;
        }
        Err(e) => {
            warn!(error = %e, relay = label, "Relay authentication failed for blob sync");
            return None;
        }
    }

    match session.register(vec![interface_id]).await {
        Ok(ack) => {
            if ack.accepted.is_empty() {
                warn!(relay = label, "Relay rejected vault-blobs interface registration");
                return None;
            }
            debug!(relay = label, "Registered vault-blobs interface with relay");
        }
        Err(e) => {
            warn!(error = %e, relay = label, "Failed to register vault-blobs interface");
            return None;
        }
    }

    Some(session)
}

/// Connect to relays and set up blob sync for a vault.
///
/// Connects to the local node's own relay (for pushing), and optionally
/// to a peer's relay (for pulling). Both peers in a vault should call
/// this with each other's addresses so blobs flow bidirectionally.
///
/// Returns `None` if no relay connections succeed (vault still works
/// without relay, just no remote blob replication).
pub async fn connect_relays(
    network: &indras_network::IndrasNetwork,
    own_addr: Option<iroh::EndpointAddr>,
    peer_addr: Option<iroh::EndpointAddr>,
    realm_id: InterfaceId,
) -> Option<Arc<RelayBlobSync>> {
    let interface_id = vault_blob_interface_id(&realm_id);

    // Connect to own relay first (push target)
    let own_session = match own_addr {
        Some(addr) => setup_relay_session(network, addr, interface_id, "own").await,
        None => None,
    };

    let Some(own_session) = own_session else {
        // If we can't connect to own relay, try peer relay as fallback
        if let Some(addr) = peer_addr {
            if let Some(session) = setup_relay_session(network, addr, interface_id, "peer").await {
                info!("Relay blob sync connected (peer relay only)");
                return Some(Arc::new(RelayBlobSync::new(session, interface_id, "peer")));
            }
        }
        return None;
    };

    let mut sync = RelayBlobSync::new(own_session, interface_id, "own");

    // Connect to peer relay (pull source)
    if let Some(addr) = peer_addr {
        if let Some(peer_session) = setup_relay_session(network, addr, interface_id, "peer").await {
            sync.add_relay(peer_session, "peer");
            info!("Relay blob sync connected (own + peer relays)");
        } else {
            info!("Relay blob sync connected (own relay only)");
        }
    } else {
        info!("Relay blob sync connected (own relay only)");
    }

    Some(Arc::new(sync))
}
