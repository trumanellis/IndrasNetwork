//! DTN manager for offline peer delivery
//!
//! Coordinates the DTN subsystem components:
//! - [`ProphetState`]: probabilistic routing via encounter history
//! - [`EpidemicRouter`]: spray-and-wait routing decisions
//! - [`CustodyManager`]: custody transfer protocol
//! - [`AgeManager`]: bundle expiration and priority demotion
//! - [`StrategySelector`]: adaptive strategy selection
//! - [`BundleStore`]: persistent bundle storage (redb)
//!
//! ## Usage
//!
//! The `DtnManager` sits alongside `SyncTask`. When a peer is detected as
//! offline, pending messages are handed to the DTN manager which wraps them
//! in bundles, stores them persistently, and manages relay forwarding.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::{debug, info};

use indras_core::packet::{EncryptedPayload, Packet, PacketId, Priority};
use indras_core::PeerIdentity;
use indras_dtn::{
    AgeManager, Bundle, BundleId, CustodyManager, DtnConfig, EpidemicRouter,
    ProphetState, StrategySelector,
};
use indras_transport::IrohIdentity;

use crate::bundle_store::BundleStore;
use crate::error::{NodeError, NodeResult};
use crate::message_handler::SignedNetworkMessage;

/// DTN message for relay forwarding
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DtnBundleMessage {
    /// The serialized bundle (contains the already-encrypted SignedNetworkMessage as payload)
    pub bundle_bytes: Vec<u8>,
    /// Sender's ProphetState summary for transitive probability updates
    pub prophet_summary: Option<Vec<(Vec<u8>, f64)>>,
}

/// DTN custody protocol message
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DtnCustodyMessage {
    /// The custody protocol message
    pub custody_bytes: Vec<u8>,
}

/// Manages DTN store-and-forward for offline peer delivery
pub struct DtnManager {
    /// Probabilistic routing via encounter history
    prophet: ProphetState<IrohIdentity>,
    /// Epidemic/spray-and-wait routing decisions
    epidemic: EpidemicRouter<IrohIdentity>,
    /// Custody transfer management
    custody: CustodyManager<IrohIdentity>,
    /// Bundle expiration tracking
    age_manager: AgeManager<IrohIdentity>,
    /// Adaptive strategy selection (used for future condition-based routing)
    #[allow(dead_code)]
    strategy: StrategySelector,
    /// DTN configuration
    config: DtnConfig,
    /// Persistent bundle storage
    bundle_store: Arc<BundleStore>,
    /// Our identity
    local_identity: IrohIdentity,
    /// Sequence counter for PacketId generation
    sequence: AtomicU64,
}

impl DtnManager {
    /// Create a new DTN manager
    pub fn new(
        config: DtnConfig,
        bundle_store: Arc<BundleStore>,
        local_identity: IrohIdentity,
    ) -> Self {
        let prophet = ProphetState::with_defaults(local_identity.clone());
        let epidemic = EpidemicRouter::new(config.epidemic.clone());
        let custody = CustodyManager::new(config.custody.clone());
        let age_manager = AgeManager::new(config.expiration.clone());
        let strategy = StrategySelector::with_defaults();

        Self {
            prophet,
            epidemic,
            custody,
            age_manager,
            strategy,
            config,
            bundle_store,
            local_identity,
            sequence: AtomicU64::new(1),
        }
    }

    /// Enqueue a signed message for DTN delivery to an offline peer
    ///
    /// Wraps the `SignedNetworkMessage` (already encrypted+signed) as the
    /// opaque payload inside a `Bundle<IrohIdentity>`.
    pub fn enqueue(
        &self,
        signed_msg: &SignedNetworkMessage,
        destination: IrohIdentity,
        priority: Priority,
    ) -> NodeResult<BundleId> {
        let msg_bytes = signed_msg.to_bytes().map_err(|e| {
            NodeError::Io(format!("Failed to serialize message for DTN: {e}"))
        })?;

        let seq = self.sequence.fetch_add(1, Ordering::Relaxed);
        let source_hash = self.identity_hash();
        let packet_id = PacketId::new(source_hash, seq);

        let packet = Packet::new(
            packet_id,
            self.local_identity.clone(),
            destination,
            EncryptedPayload::plaintext(msg_bytes),
            vec![],
        )
        .with_priority(priority);

        let lifetime = chrono::Duration::from_std(self.config.expiration.default_lifetime)
            .unwrap_or(chrono::Duration::hours(1));
        let bundle = Bundle::from_packet(packet, lifetime);
        let bundle_id = bundle.bundle_id;

        // Track in age manager
        self.age_manager.track(&bundle);

        // Store persistently
        self.bundle_store.store_bundle(&bundle)?;

        info!(
            bundle_id = %bundle_id,
            destination = %bundle.packet.destination.short_id(),
            "Enqueued message for DTN delivery"
        );

        Ok(bundle_id)
    }

    /// Drain all pending bundles for a destination (when peer reconnects)
    pub fn drain_for(
        &self,
        destination: &IrohIdentity,
    ) -> NodeResult<Vec<Bundle<IrohIdentity>>> {
        let bundles = self.bundle_store.pending_for(destination)?;

        if !bundles.is_empty() {
            info!(
                destination = %destination.short_id(),
                count = bundles.len(),
                "Draining DTN bundles for reconnected peer"
            );
        }

        Ok(bundles)
    }

    /// Remove a delivered bundle from storage
    pub fn mark_delivered(
        &self,
        bundle_id: &BundleId,
        destination: &IrohIdentity,
    ) -> NodeResult<()> {
        self.age_manager.untrack(bundle_id);
        self.bundle_store.delete_bundle(bundle_id, destination)?;
        debug!(bundle_id = %bundle_id, "Bundle delivered, removed from DTN store");
        Ok(())
    }

    /// Extract the original SignedNetworkMessage from a DTN bundle
    pub fn unwrap_bundle(
        bundle: &Bundle<IrohIdentity>,
    ) -> NodeResult<SignedNetworkMessage> {
        SignedNetworkMessage::from_bytes(bundle.packet.payload.as_bytes()).map_err(|e| {
            NodeError::Io(format!("Failed to deserialize message from bundle: {e}"))
        })
    }

    /// Record an encounter with a peer (updates delivery probabilities)
    pub fn record_encounter(&self, peer: &IrohIdentity) {
        self.prophet.encounter(peer);
        debug!(
            peer = %peer.short_id(),
            "Recorded DTN encounter"
        );
    }

    /// Process a ProphetSummary from a peer for transitive probability updates
    pub fn process_prophet_exchange(
        &self,
        peer: &IrohIdentity,
        probabilities: &[(IrohIdentity, f64)],
    ) {
        self.prophet.transitive_update(peer, probabilities);
    }

    /// Select the best relay candidate for a destination from connected peers
    ///
    /// Returns the peer with the highest delivery probability to the destination,
    /// but only if their probability exceeds ours.
    pub fn select_relay_candidate(
        &self,
        destination: &IrohIdentity,
        connected_peers: &[IrohIdentity],
    ) -> Option<IrohIdentity> {
        self.prophet.best_candidate(destination, connected_peers)
            .filter(|candidate| self.prophet.should_forward_to(destination, candidate))
    }

    /// Check if we should forward a bundle to a candidate peer
    pub fn should_forward_to(
        &self,
        destination: &IrohIdentity,
        candidate: &IrohIdentity,
    ) -> bool {
        self.prophet.should_forward_to(destination, candidate)
    }

    /// Get our ProphetState summary for exchange with peers
    pub fn prophet_summary(&self) -> Vec<(IrohIdentity, f64)> {
        self.prophet.all_probabilities()
    }

    /// Run periodic cleanup: expire bundles, age probabilities, clean custody
    pub fn cleanup(&self) -> NodeResult<usize> {
        // Age prophet probabilities
        self.prophet.age_all();

        // Clean expired bundles from persistent store
        let expired_count = self.bundle_store.cleanup_expired()?;

        // Clean seen bundles in epidemic router
        self.epidemic.cleanup_seen();

        // Clean expired custody records
        self.custody.cleanup_expired();

        if expired_count > 0 {
            info!(expired_count, "DTN cleanup completed");
        }

        Ok(expired_count)
    }

    /// Accept custody of a bundle relayed from another peer
    pub fn accept_relay_bundle(
        &self,
        bundle: Bundle<IrohIdentity>,
    ) -> NodeResult<()> {
        // Check if we've already seen this bundle
        if self.epidemic.have_seen(&bundle.bundle_id) {
            debug!(bundle_id = %bundle.bundle_id, "Duplicate DTN bundle, ignoring");
            return Ok(());
        }
        self.epidemic.mark_seen(bundle.bundle_id);

        // Track expiration
        self.age_manager.track(&bundle);

        // Store persistently
        self.bundle_store.store_bundle(&bundle)?;

        info!(
            bundle_id = %bundle.bundle_id,
            destination = %bundle.packet.destination.short_id(),
            "Accepted custody of relayed DTN bundle"
        );

        Ok(())
    }

    /// Get the count of stored bundles
    pub fn bundle_count(&self) -> NodeResult<usize> {
        self.bundle_store.count()
    }

    /// Get the underlying bundle store
    pub fn store(&self) -> &Arc<BundleStore> {
        &self.bundle_store
    }

    /// Hash our identity (for PacketId generation)
    fn identity_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.local_identity.hash(&mut hasher);
        hasher.finish()
    }
}
