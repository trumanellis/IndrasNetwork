//! Unified delivery status tracking across sync and DTN paths
//!
//! Provides a single view of message delivery state regardless of whether
//! a message was delivered via direct sync or store-and-forward DTN.
//!
//! ## Status flow
//!
//! ```text
//! Queued(sync) → Sent → Acked
//! Queued(sync) → Offline → DtnEnqueued → DtnRelayed(peer) → Delivered
//! Queued(sync) → Offline → DtnEnqueued → Delivered
//! ```
//!
//! The tracker is in-memory with [`NodeLog`] as the durable audit trail.
//! On restart, status is reconstructed from the node log.

use std::time::Instant;

use dashmap::DashMap;
use indras_core::{EventId, InterfaceId, PeerIdentity};
use indras_dtn::BundleId;
use indras_transport::IrohIdentity;

/// Delivery path taken by a message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryPath {
    /// Delivered directly via sync
    Sync,
    /// Handed to DTN for offline delivery
    Dtn,
}

/// Current delivery status for an event→peer pair
#[derive(Debug, Clone)]
pub enum DeliveryStatus {
    /// Event queued, not yet sent
    Queued {
        /// When the event was queued
        since: Instant,
    },
    /// Sent via sync, awaiting ack
    Sent {
        /// When the send succeeded
        at: Instant,
    },
    /// Peer went offline, handed to DTN
    DtnEnqueued {
        /// When the handoff occurred
        at: Instant,
        /// The DTN bundle ID wrapping this event
        bundle_id: BundleId,
    },
    /// DTN bundle relayed to a better candidate
    DtnRelayed {
        /// When the relay occurred
        at: Instant,
        /// The relay candidate peer
        relay_peer: IrohIdentity,
        /// The DTN bundle ID
        bundle_id: BundleId,
    },
    /// Successfully delivered (via either path)
    Delivered {
        /// When delivery was confirmed
        at: Instant,
        /// Which path was used
        path: DeliveryPath,
    },
    /// Acknowledged by the recipient
    Acked {
        /// When the ack was received
        at: Instant,
    },
}

impl DeliveryStatus {
    /// Whether this status represents a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, DeliveryStatus::Delivered { .. } | DeliveryStatus::Acked { .. })
    }

    /// Human-readable status label
    pub fn label(&self) -> &'static str {
        match self {
            DeliveryStatus::Queued { .. } => "queued",
            DeliveryStatus::Sent { .. } => "sent",
            DeliveryStatus::DtnEnqueued { .. } => "dtn_enqueued",
            DeliveryStatus::DtnRelayed { .. } => "dtn_relayed",
            DeliveryStatus::Delivered { .. } => "delivered",
            DeliveryStatus::Acked { .. } => "acked",
        }
    }
}

/// Key for tracking delivery of a specific event to a specific peer
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct DeliveryKey {
    interface_id: InterfaceId,
    event_id: EventId,
    destination: Vec<u8>,
}

/// Tracks delivery status across sync and DTN paths
///
/// In-memory tracker that provides a unified view of where each message
/// is in its delivery journey. The [`NodeLog`] serves as the durable
/// audit trail; this struct provides fast lookups and status queries.
pub struct DeliveryTracker {
    /// Current status for each (interface, event, peer) tuple
    statuses: DashMap<DeliveryKey, DeliveryStatus>,
    /// Reverse index: bundle_id → delivery keys (for DTN status updates)
    bundle_index: DashMap<BundleId, Vec<DeliveryKey>>,
    /// Maximum number of terminal entries to keep before pruning
    max_terminal: usize,
}

impl DeliveryTracker {
    /// Create a new delivery tracker
    pub fn new() -> Self {
        Self {
            statuses: DashMap::new(),
            bundle_index: DashMap::new(),
            max_terminal: 10_000,
        }
    }

    /// Record that an event was queued for delivery
    pub fn record_queued(
        &self,
        interface_id: InterfaceId,
        event_id: EventId,
        destination: &IrohIdentity,
    ) {
        let key = DeliveryKey {
            interface_id,
            event_id,
            destination: destination.as_bytes(),
        };
        self.statuses.insert(key, DeliveryStatus::Queued { since: Instant::now() });
    }

    /// Record that an event was sent via sync
    pub fn record_sent(
        &self,
        interface_id: InterfaceId,
        event_id: EventId,
        destination: &IrohIdentity,
    ) {
        let key = DeliveryKey {
            interface_id,
            event_id,
            destination: destination.as_bytes(),
        };
        self.statuses.insert(key, DeliveryStatus::Sent { at: Instant::now() });
    }

    /// Record that events were handed to DTN for an offline peer
    pub fn record_dtn_handoff(
        &self,
        interface_id: InterfaceId,
        event_id: EventId,
        destination: &IrohIdentity,
        bundle_id: BundleId,
    ) {
        let key = DeliveryKey {
            interface_id,
            event_id,
            destination: destination.as_bytes(),
        };
        self.statuses.insert(
            key.clone(),
            DeliveryStatus::DtnEnqueued {
                at: Instant::now(),
                bundle_id,
            },
        );
        self.bundle_index.entry(bundle_id).or_default().push(key);
    }

    /// Record that a DTN bundle was relayed to a better candidate
    pub fn record_dtn_relayed(
        &self,
        bundle_id: &BundleId,
        relay_peer: IrohIdentity,
    ) {
        if let Some(keys) = self.bundle_index.get(bundle_id) {
            let now = Instant::now();
            for key in keys.value() {
                self.statuses.insert(
                    key.clone(),
                    DeliveryStatus::DtnRelayed {
                        at: now,
                        relay_peer: relay_peer.clone(),
                        bundle_id: *bundle_id,
                    },
                );
            }
        }
    }

    /// Record that a DTN bundle was delivered (peer reconnected)
    pub fn record_dtn_delivered(
        &self,
        bundle_id: &BundleId,
    ) {
        if let Some(keys) = self.bundle_index.get(bundle_id) {
            let now = Instant::now();
            for key in keys.value() {
                self.statuses.insert(
                    key.clone(),
                    DeliveryStatus::Delivered {
                        at: now,
                        path: DeliveryPath::Dtn,
                    },
                );
            }
        }
        self.bundle_index.remove(bundle_id);
    }

    /// Record that a peer acknowledged events up to a given event ID
    pub fn record_ack(
        &self,
        interface_id: InterfaceId,
        destination: &IrohIdentity,
        up_to: EventId,
    ) {
        let dest_bytes = destination.as_bytes();
        let now = Instant::now();
        // Mark all events for this peer up to `up_to` as acked
        for mut entry in self.statuses.iter_mut() {
            let key = entry.key();
            if key.interface_id == interface_id
                && key.destination == dest_bytes
                && key.event_id <= up_to
                && !entry.value().is_terminal()
            {
                *entry.value_mut() = DeliveryStatus::Acked { at: now };
            }
        }
    }

    /// Get the delivery status for a specific event to a specific peer
    pub fn status(
        &self,
        interface_id: &InterfaceId,
        event_id: &EventId,
        destination: &IrohIdentity,
    ) -> Option<DeliveryStatus> {
        let key = DeliveryKey {
            interface_id: *interface_id,
            event_id: *event_id,
            destination: destination.as_bytes(),
        };
        self.statuses.get(&key).map(|e| e.value().clone())
    }

    /// Get a summary of delivery statuses for an interface
    pub fn interface_summary(&self, interface_id: &InterfaceId) -> DeliverySummary {
        let mut summary = DeliverySummary::default();
        for entry in self.statuses.iter() {
            if entry.key().interface_id == *interface_id {
                match entry.value() {
                    DeliveryStatus::Queued { .. } => summary.queued += 1,
                    DeliveryStatus::Sent { .. } => summary.sent += 1,
                    DeliveryStatus::DtnEnqueued { .. } => summary.dtn_enqueued += 1,
                    DeliveryStatus::DtnRelayed { .. } => summary.dtn_relayed += 1,
                    DeliveryStatus::Delivered { .. } => summary.delivered += 1,
                    DeliveryStatus::Acked { .. } => summary.acked += 1,
                }
            }
        }
        summary
    }

    /// Get all in-flight (non-terminal) deliveries for a peer
    pub fn pending_for(
        &self,
        destination: &IrohIdentity,
    ) -> Vec<(InterfaceId, EventId, DeliveryStatus)> {
        let dest_bytes = destination.as_bytes();
        self.statuses
            .iter()
            .filter(|e| e.key().destination == dest_bytes && !e.value().is_terminal())
            .map(|e| (e.key().interface_id, e.key().event_id, e.value().clone()))
            .collect()
    }

    /// Prune terminal entries older than the max capacity
    pub fn prune(&self) {
        let terminal_count = self.statuses.iter().filter(|e| e.value().is_terminal()).count();
        if terminal_count <= self.max_terminal {
            return;
        }

        // Collect terminal entries sorted by completion time, remove oldest
        let mut terminals: Vec<(DeliveryKey, Instant)> = self
            .statuses
            .iter()
            .filter_map(|e| {
                let time = match e.value() {
                    DeliveryStatus::Delivered { at, .. } => Some(*at),
                    DeliveryStatus::Acked { at } => Some(*at),
                    _ => None,
                };
                time.map(|t| (e.key().clone(), t))
            })
            .collect();

        terminals.sort_by_key(|(_, t)| *t);

        let to_remove = terminal_count - self.max_terminal;
        for (key, _) in terminals.into_iter().take(to_remove) {
            self.statuses.remove(&key);
        }
    }
}

/// Summary of delivery statuses for an interface
#[derive(Debug, Clone, Default)]
pub struct DeliverySummary {
    /// Events queued but not yet sent
    pub queued: u32,
    /// Events sent via sync, awaiting ack
    pub sent: u32,
    /// Events handed to DTN
    pub dtn_enqueued: u32,
    /// Events relayed via DTN
    pub dtn_relayed: u32,
    /// Events confirmed delivered
    pub delivered: u32,
    /// Events acknowledged by recipient
    pub acked: u32,
}

impl DeliverySummary {
    /// Total in-flight (non-terminal) deliveries
    pub fn in_flight(&self) -> u32 {
        self.queued + self.sent + self.dtn_enqueued + self.dtn_relayed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::InterfaceId;

    fn make_id(n: u8) -> IrohIdentity {
        let secret = iroh::SecretKey::from_bytes(&{
            let mut bytes = [0u8; 32];
            bytes[0] = n;
            bytes
        });
        IrohIdentity::from(secret.public())
    }

    fn make_event_id(n: u64) -> EventId {
        EventId::new(0, n)
    }

    fn make_bundle_id(n: u32) -> BundleId {
        BundleId {
            source_hash: 1,
            creation_timestamp: 100,
            sequence: n,
        }
    }

    #[test]
    fn test_sync_delivery_lifecycle() {
        let tracker = DeliveryTracker::new();
        let iface = InterfaceId::generate();
        let event = make_event_id(1);
        let peer = make_id(1);

        tracker.record_queued(iface, event, &peer);
        assert_eq!(tracker.status(&iface, &event, &peer).unwrap().label(), "queued");

        tracker.record_sent(iface, event, &peer);
        assert_eq!(tracker.status(&iface, &event, &peer).unwrap().label(), "sent");

        tracker.record_ack(iface, &peer, event);
        assert_eq!(tracker.status(&iface, &event, &peer).unwrap().label(), "acked");
    }

    #[test]
    fn test_dtn_delivery_lifecycle() {
        let tracker = DeliveryTracker::new();
        let iface = InterfaceId::generate();
        let event = make_event_id(1);
        let peer = make_id(1);
        let bundle = make_bundle_id(42);

        tracker.record_queued(iface, event, &peer);
        tracker.record_dtn_handoff(iface, event, &peer, bundle);
        assert_eq!(tracker.status(&iface, &event, &peer).unwrap().label(), "dtn_enqueued");

        let relay = make_id(2);
        tracker.record_dtn_relayed(&bundle, relay);
        assert_eq!(tracker.status(&iface, &event, &peer).unwrap().label(), "dtn_relayed");

        tracker.record_dtn_delivered(&bundle);
        let status = tracker.status(&iface, &event, &peer).unwrap();
        assert_eq!(status.label(), "delivered");
        assert!(matches!(status, DeliveryStatus::Delivered { path: DeliveryPath::Dtn, .. }));
    }

    #[test]
    fn test_interface_summary() {
        let tracker = DeliveryTracker::new();
        let iface = InterfaceId::generate();
        let peer_a = make_id(1);
        let peer_b = make_id(2);

        tracker.record_queued(iface, make_event_id(1), &peer_a);
        tracker.record_sent(iface, make_event_id(2), &peer_a);
        tracker.record_queued(iface, make_event_id(3), &peer_b);

        let summary = tracker.interface_summary(&iface);
        assert_eq!(summary.queued, 2);
        assert_eq!(summary.sent, 1);
        assert_eq!(summary.in_flight(), 3);
    }

    #[test]
    fn test_pending_for_peer() {
        let tracker = DeliveryTracker::new();
        let iface = InterfaceId::generate();
        let peer = make_id(1);

        tracker.record_queued(iface, make_event_id(1), &peer);
        tracker.record_sent(iface, make_event_id(2), &peer);
        tracker.record_ack(iface, &peer, make_event_id(1));

        let pending = tracker.pending_for(&peer);
        // event 1 is acked (terminal), only event 2 should be pending
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, make_event_id(2));
    }
}
