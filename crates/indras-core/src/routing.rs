//! Routing types and decisions

use serde::{Deserialize, Serialize};

use crate::event::DropReason;
use crate::identity::PeerIdentity;

/// Possible outcomes of routing a packet
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub enum RoutingDecision<I: PeerIdentity> {
    /// Deliver directly to the destination (it's online and reachable)
    DirectDelivery {
        /// The destination peer
        destination: I,
    },

    /// Relay through one or more intermediate peers
    RelayThrough {
        /// Next hop(s) to try, in order of preference
        next_hops: Vec<I>,
    },

    /// Hold the packet for later (destination offline, no relay available)
    HoldForLater,

    /// Drop the packet
    Drop {
        /// Why the packet should be dropped
        reason: DropReason,
    },
}

impl<I: PeerIdentity> RoutingDecision<I> {
    /// Create a direct delivery decision
    pub fn direct(destination: I) -> Self {
        Self::DirectDelivery { destination }
    }

    /// Create a relay decision with a single next hop
    pub fn relay(next_hop: I) -> Self {
        Self::RelayThrough {
            next_hops: vec![next_hop],
        }
    }

    /// Create a relay decision with multiple next hops
    pub fn relay_multi(next_hops: Vec<I>) -> Self {
        Self::RelayThrough { next_hops }
    }

    /// Create a hold decision
    pub fn hold() -> Self {
        Self::HoldForLater
    }

    /// Create a drop decision
    pub fn drop(reason: DropReason) -> Self {
        Self::Drop { reason }
    }

    /// Check if this is a delivery decision
    pub fn is_delivery(&self) -> bool {
        matches!(self, Self::DirectDelivery { .. })
    }

    /// Check if this is a relay decision
    pub fn is_relay(&self) -> bool {
        matches!(self, Self::RelayThrough { .. })
    }

    /// Check if this is a hold decision
    pub fn is_hold(&self) -> bool {
        matches!(self, Self::HoldForLater)
    }

    /// Check if this is a drop decision
    pub fn is_drop(&self) -> bool {
        matches!(self, Self::Drop { .. })
    }
}

/// Information about a route to a destination
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub struct RouteInfo<I: PeerIdentity> {
    /// The destination peer
    pub destination: I,
    /// Next hop toward the destination
    pub next_hop: I,
    /// Estimated hop count to destination
    pub hop_count: u32,
    /// Route quality metric (lower is better)
    pub metric: u32,
    /// When this route was last confirmed working
    pub last_confirmed: Option<chrono::DateTime<chrono::Utc>>,
}

impl<I: PeerIdentity> RouteInfo<I> {
    /// Create new route info
    pub fn new(destination: I, next_hop: I, hop_count: u32) -> Self {
        Self {
            destination,
            next_hop,
            hop_count,
            metric: hop_count, // Default metric is hop count
            last_confirmed: None,
        }
    }

    /// Update the last confirmed time to now
    pub fn confirm(&mut self) {
        self.last_confirmed = Some(chrono::Utc::now());
    }

    /// Check if this route is stale (not confirmed recently)
    pub fn is_stale(&self, max_age: chrono::Duration) -> bool {
        match self.last_confirmed {
            Some(confirmed) => chrono::Utc::now() - confirmed > max_age,
            None => true,
        }
    }
}
