//! DTN routing strategy selection
//!
//! Different network conditions call for different routing strategies.
//! This module provides strategy types and a selector that chooses
//! the best strategy based on current conditions.

use std::time::Duration;

use indras_core::{NetworkTopology, PeerIdentity, Priority};

use crate::bundle::Bundle;

/// DTN routing strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtnStrategy {
    /// Standard store-and-forward routing (use existing router)
    ///
    /// Best for well-connected networks with occasional offline peers.
    StoreAndForward,

    /// Epidemic flooding - forward to all neighbors
    ///
    /// Best for sparse networks with unpredictable connectivity.
    /// Uses more bandwidth but maximizes delivery probability.
    Epidemic,

    /// Spray-and-wait with limited copy count
    ///
    /// Balanced approach: sprays limited copies, then waits.
    /// Good balance of delivery probability and resource usage.
    SprayAndWait { copies: u8 },

    /// PRoPHET (Probabilistic Routing Protocol using History)
    ///
    /// Uses delivery probability based on encounter history.
    /// Good for networks with predictable mobility patterns.
    Prophet,
}

impl Default for DtnStrategy {
    fn default() -> Self {
        DtnStrategy::SprayAndWait { copies: 4 }
    }
}

impl DtnStrategy {
    /// Get the initial copy count for this strategy
    pub fn initial_copies(&self) -> u8 {
        match self {
            DtnStrategy::SprayAndWait { copies } => *copies,
            DtnStrategy::Epidemic => 8, // Default flood copies
            _ => 1,
        }
    }

    /// Check if this strategy uses epidemic-style routing
    pub fn is_epidemic(&self) -> bool {
        matches!(
            self,
            DtnStrategy::Epidemic | DtnStrategy::SprayAndWait { .. }
        )
    }
}

/// Conditions for strategy selection rules
#[derive(Debug, Clone)]
pub enum StrategyCondition {
    /// Network connectivity is below a threshold
    ///
    /// Ratio of reachable peers to total known peers.
    LowConnectivity { threshold: f32 },

    /// Bundle has at least the specified priority
    PriorityAtLeast(Priority),

    /// Bundle age is above a threshold
    AgeAbove(Duration),

    /// Destination has been unreachable for at least this long
    DestinationUnreachable(Duration),

    /// Always match (catch-all rule)
    Always,
}

impl StrategyCondition {
    /// Check if this condition matches for a given bundle and topology
    pub fn matches<I: PeerIdentity, T: NetworkTopology<I>>(
        &self,
        bundle: &Bundle<I>,
        topology: &T,
    ) -> bool {
        match self {
            StrategyCondition::LowConnectivity { threshold } => {
                let peers = topology.peers();
                if peers.is_empty() {
                    return true;
                }
                let online = peers.iter().filter(|p| topology.is_online(p)).count();
                let ratio = online as f32 / peers.len() as f32;
                ratio < *threshold
            }
            StrategyCondition::PriorityAtLeast(min_priority) => {
                bundle.effective_priority() >= *min_priority
            }
            StrategyCondition::AgeAbove(threshold) => {
                let age = bundle.age();
                let age_duration =
                    Duration::from_millis(age.num_milliseconds().max(0) as u64);
                age_duration > *threshold
            }
            StrategyCondition::DestinationUnreachable(_duration) => {
                // Check if destination is currently offline
                // Note: In a full implementation, we'd track how long
                // the destination has been unreachable
                !topology.is_online(bundle.destination())
            }
            StrategyCondition::Always => true,
        }
    }
}

/// A rule mapping a condition to a strategy
#[derive(Debug, Clone)]
pub struct StrategyRule {
    /// Condition that must be met
    pub condition: StrategyCondition,
    /// Strategy to use when condition is met
    pub strategy: DtnStrategy,
}

impl StrategyRule {
    /// Create a new strategy rule
    pub fn new(condition: StrategyCondition, strategy: DtnStrategy) -> Self {
        Self { condition, strategy }
    }
}

/// Selects routing strategy based on conditions
pub struct StrategySelector {
    /// Default strategy when no rules match
    default: DtnStrategy,
    /// Rules to evaluate (in order)
    rules: Vec<StrategyRule>,
}

impl StrategySelector {
    /// Create a new selector with a default strategy
    pub fn new(default: DtnStrategy) -> Self {
        Self {
            default,
            rules: Vec::new(),
        }
    }

    /// Create a selector with common default rules
    pub fn with_defaults() -> Self {
        let mut selector = Self::new(DtnStrategy::SprayAndWait { copies: 4 });

        // High priority bundles use more aggressive routing
        selector.add_rule(StrategyRule::new(
            StrategyCondition::PriorityAtLeast(Priority::Critical),
            DtnStrategy::Epidemic,
        ));

        // Low connectivity triggers epidemic routing
        selector.add_rule(StrategyRule::new(
            StrategyCondition::LowConnectivity { threshold: 0.3 },
            DtnStrategy::Epidemic,
        ));

        // Old bundles get spray-and-wait to balance delivery vs. overhead
        selector.add_rule(StrategyRule::new(
            StrategyCondition::AgeAbove(Duration::from_secs(600)),
            DtnStrategy::SprayAndWait { copies: 2 },
        ));

        selector
    }

    /// Add a rule to the selector
    ///
    /// Rules are evaluated in the order they are added.
    /// The first matching rule determines the strategy.
    pub fn add_rule(&mut self, rule: StrategyRule) {
        self.rules.push(rule);
    }

    /// Select a strategy for a bundle
    pub fn select<I: PeerIdentity, T: NetworkTopology<I>>(
        &self,
        bundle: &Bundle<I>,
        topology: &T,
    ) -> DtnStrategy {
        for rule in &self.rules {
            if rule.condition.matches(bundle, topology) {
                return rule.strategy;
            }
        }
        self.default
    }

    /// Get the default strategy
    pub fn default_strategy(&self) -> DtnStrategy {
        self.default
    }

    /// Set the default strategy
    pub fn set_default(&mut self, strategy: DtnStrategy) {
        self.default = strategy;
    }

    /// Get number of rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Clear all rules
    pub fn clear_rules(&mut self) {
        self.rules.clear();
    }
}

impl Default for StrategySelector {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use indras_core::{EncryptedPayload, Packet, PacketId, SimulationIdentity};
    use std::collections::HashMap;
    use std::sync::RwLock;

    use crate::bundle::Bundle;

    /// Simple test topology
    struct TestTopology {
        peers: Vec<SimulationIdentity>,
        connections: HashMap<SimulationIdentity, Vec<SimulationIdentity>>,
        online: RwLock<std::collections::HashSet<SimulationIdentity>>,
    }

    impl TestTopology {
        fn new(peers: Vec<SimulationIdentity>) -> Self {
            Self {
                peers,
                connections: HashMap::new(),
                online: RwLock::new(std::collections::HashSet::new()),
            }
        }

        fn set_online(&self, peer: SimulationIdentity) {
            self.online.write().unwrap().insert(peer);
        }

        fn set_all_online(&self) {
            let mut online = self.online.write().unwrap();
            for peer in &self.peers {
                online.insert(*peer);
            }
        }
    }

    impl NetworkTopology<SimulationIdentity> for TestTopology {
        fn peers(&self) -> Vec<SimulationIdentity> {
            self.peers.clone()
        }

        fn neighbors(&self, peer: &SimulationIdentity) -> Vec<SimulationIdentity> {
            self.connections.get(peer).cloned().unwrap_or_default()
        }

        fn are_connected(&self, a: &SimulationIdentity, b: &SimulationIdentity) -> bool {
            self.connections
                .get(a)
                .map(|n| n.contains(b))
                .unwrap_or(false)
        }

        fn is_online(&self, peer: &SimulationIdentity) -> bool {
            self.online.read().unwrap().contains(peer)
        }
    }

    fn make_test_bundle() -> Bundle<SimulationIdentity> {
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('Z').unwrap();
        let id = PacketId::new(0x1234, 1);

        let packet = Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(b"test".to_vec()),
            vec![],
        );

        Bundle::from_packet(packet, ChronoDuration::hours(1))
    }

    #[test]
    fn test_default_strategy() {
        let selector = StrategySelector::new(DtnStrategy::StoreAndForward);
        assert_eq!(
            selector.default_strategy(),
            DtnStrategy::StoreAndForward
        );
    }

    #[test]
    fn test_with_defaults() {
        let selector = StrategySelector::with_defaults();
        assert!(selector.rule_count() > 0);
    }

    #[test]
    fn test_low_connectivity_rule() {
        let mut selector = StrategySelector::new(DtnStrategy::StoreAndForward);
        selector.add_rule(StrategyRule::new(
            StrategyCondition::LowConnectivity { threshold: 0.5 },
            DtnStrategy::Epidemic,
        ));

        let peers: Vec<_> = ('A'..='J')
            .filter_map(SimulationIdentity::new)
            .collect();
        let topology = TestTopology::new(peers.clone());

        // Only 2 of 10 peers online = 20% connectivity
        topology.set_online(peers[0]);
        topology.set_online(peers[1]);

        let bundle = make_test_bundle();
        let strategy = selector.select(&bundle, &topology);

        assert_eq!(strategy, DtnStrategy::Epidemic);
    }

    #[test]
    fn test_high_connectivity_uses_default() {
        let mut selector = StrategySelector::new(DtnStrategy::StoreAndForward);
        selector.add_rule(StrategyRule::new(
            StrategyCondition::LowConnectivity { threshold: 0.5 },
            DtnStrategy::Epidemic,
        ));

        let peers: Vec<_> = ('A'..='J')
            .filter_map(SimulationIdentity::new)
            .collect();
        let topology = TestTopology::new(peers);
        topology.set_all_online(); // 100% connectivity

        let bundle = make_test_bundle();
        let strategy = selector.select(&bundle, &topology);

        assert_eq!(strategy, DtnStrategy::StoreAndForward);
    }

    #[test]
    fn test_priority_rule() {
        let mut selector = StrategySelector::new(DtnStrategy::StoreAndForward);
        selector.add_rule(StrategyRule::new(
            StrategyCondition::PriorityAtLeast(Priority::Critical),
            DtnStrategy::Epidemic,
        ));

        let peers = vec![SimulationIdentity::new('A').unwrap()];
        let topology = TestTopology::new(peers);

        // Normal priority bundle
        let bundle = make_test_bundle();
        assert_eq!(
            selector.select(&bundle, &topology),
            DtnStrategy::StoreAndForward
        );

        // Critical priority bundle
        let mut critical_packet = bundle.packet.clone();
        critical_packet.priority = Priority::Critical;
        let critical_bundle =
            Bundle::from_packet(critical_packet, ChronoDuration::hours(1));
        assert_eq!(
            selector.select(&critical_bundle, &topology),
            DtnStrategy::Epidemic
        );
    }

    #[test]
    fn test_strategy_copies() {
        assert_eq!(DtnStrategy::StoreAndForward.initial_copies(), 1);
        assert_eq!(DtnStrategy::Epidemic.initial_copies(), 8);
        assert_eq!(DtnStrategy::SprayAndWait { copies: 6 }.initial_copies(), 6);
    }

    #[test]
    fn test_is_epidemic() {
        assert!(!DtnStrategy::StoreAndForward.is_epidemic());
        assert!(DtnStrategy::Epidemic.is_epidemic());
        assert!(DtnStrategy::SprayAndWait { copies: 4 }.is_epidemic());
        assert!(!DtnStrategy::Prophet.is_epidemic());
    }
}
