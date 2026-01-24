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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationIdentity;

    fn make_peer(c: char) -> SimulationIdentity {
        SimulationIdentity::new(c).unwrap()
    }

    #[test]
    fn test_routing_decision_direct() {
        let peer = make_peer('A');
        let decision = RoutingDecision::direct(peer);

        assert!(decision.is_delivery());
        assert!(!decision.is_relay());
        assert!(!decision.is_hold());
        assert!(!decision.is_drop());

        if let RoutingDecision::DirectDelivery { destination } = decision {
            assert_eq!(destination, peer);
        } else {
            panic!("Expected DirectDelivery");
        }
    }

    #[test]
    fn test_routing_decision_relay_single() {
        let peer = make_peer('B');
        let decision = RoutingDecision::relay(peer);

        assert!(!decision.is_delivery());
        assert!(decision.is_relay());
        assert!(!decision.is_hold());
        assert!(!decision.is_drop());

        if let RoutingDecision::RelayThrough { next_hops } = decision {
            assert_eq!(next_hops.len(), 1);
            assert_eq!(next_hops[0], peer);
        } else {
            panic!("Expected RelayThrough");
        }
    }

    #[test]
    fn test_routing_decision_relay_multi() {
        let peers = vec![make_peer('A'), make_peer('B'), make_peer('C')];
        let decision = RoutingDecision::relay_multi(peers.clone());

        assert!(decision.is_relay());

        if let RoutingDecision::RelayThrough { next_hops } = decision {
            assert_eq!(next_hops.len(), 3);
            assert_eq!(next_hops, peers);
        } else {
            panic!("Expected RelayThrough");
        }
    }

    #[test]
    fn test_routing_decision_hold() {
        let decision: RoutingDecision<SimulationIdentity> = RoutingDecision::hold();

        assert!(!decision.is_delivery());
        assert!(!decision.is_relay());
        assert!(decision.is_hold());
        assert!(!decision.is_drop());
    }

    #[test]
    fn test_routing_decision_drop() {
        use crate::DropReason;

        let decision: RoutingDecision<SimulationIdentity> =
            RoutingDecision::drop(DropReason::TtlExpired);

        assert!(!decision.is_delivery());
        assert!(!decision.is_relay());
        assert!(!decision.is_hold());
        assert!(decision.is_drop());

        if let RoutingDecision::Drop { reason } = decision {
            assert_eq!(reason, DropReason::TtlExpired);
        } else {
            panic!("Expected Drop");
        }
    }

    #[test]
    fn test_routing_decision_drop_various_reasons() {
        use crate::DropReason;

        let reasons = vec![
            DropReason::TtlExpired,
            DropReason::NoRoute,
            DropReason::Duplicate,
            DropReason::Expired,
            DropReason::SenderOffline,
            DropReason::StorageFull,
            DropReason::TooLarge,
        ];

        for reason in reasons {
            let decision: RoutingDecision<SimulationIdentity> = RoutingDecision::drop(reason);
            assert!(decision.is_drop());

            if let RoutingDecision::Drop { reason: r } = decision {
                assert_eq!(r, reason);
            }
        }
    }

    #[test]
    fn test_route_info_new() {
        let dest = make_peer('A');
        let next = make_peer('B');
        let route = RouteInfo::new(dest, next, 3);

        assert_eq!(route.destination, dest);
        assert_eq!(route.next_hop, next);
        assert_eq!(route.hop_count, 3);
        assert_eq!(route.metric, 3); // Default metric equals hop count
        assert!(route.last_confirmed.is_none());
    }

    #[test]
    fn test_route_info_confirm() {
        let dest = make_peer('A');
        let next = make_peer('B');
        let mut route = RouteInfo::new(dest, next, 2);

        assert!(route.last_confirmed.is_none());

        let before = chrono::Utc::now();
        route.confirm();
        let after = chrono::Utc::now();

        assert!(route.last_confirmed.is_some());
        let confirmed = route.last_confirmed.unwrap();
        assert!(confirmed >= before && confirmed <= after);
    }

    #[test]
    fn test_route_info_staleness_unconfirmed() {
        let dest = make_peer('A');
        let next = make_peer('B');
        let route = RouteInfo::new(dest, next, 1);

        // Unconfirmed routes are always stale
        assert!(route.is_stale(chrono::Duration::zero()));
        assert!(route.is_stale(chrono::Duration::hours(24)));
    }

    #[test]
    fn test_route_info_staleness_fresh() {
        let dest = make_peer('A');
        let next = make_peer('B');
        let mut route = RouteInfo::new(dest, next, 1);

        route.confirm();

        // Just confirmed, should not be stale for any reasonable max_age
        assert!(!route.is_stale(chrono::Duration::hours(1)));
        assert!(!route.is_stale(chrono::Duration::minutes(1)));
    }

    #[test]
    fn test_route_info_staleness_zero_max_age() {
        let dest = make_peer('A');
        let next = make_peer('B');
        let mut route = RouteInfo::new(dest, next, 1);

        route.confirm();

        // With zero max_age, any route is immediately stale
        // (since Utc::now() - confirmed >= 0)
        // Actually this depends on timing, so we allow either result
        // The point is it shouldn't panic
        let _ = route.is_stale(chrono::Duration::zero());
    }

    #[test]
    fn test_routing_decision_serialization() {
        let peer = make_peer('A');
        let decision = RoutingDecision::direct(peer);

        let serialized = postcard::to_allocvec(&decision).unwrap();
        let deserialized: RoutingDecision<SimulationIdentity> =
            postcard::from_bytes(&serialized).unwrap();

        assert!(deserialized.is_delivery());
    }

    #[test]
    fn test_route_info_serialization() {
        let dest = make_peer('A');
        let next = make_peer('B');
        let route = RouteInfo::new(dest, next, 5);

        let serialized = postcard::to_allocvec(&route).unwrap();
        let deserialized: RouteInfo<SimulationIdentity> =
            postcard::from_bytes(&serialized).unwrap();

        assert_eq!(deserialized.destination, dest);
        assert_eq!(deserialized.next_hop, next);
        assert_eq!(deserialized.hop_count, 5);
    }
}
