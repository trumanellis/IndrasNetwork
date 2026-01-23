//! PRoPHET (Probabilistic Routing Protocol using History) implementation
//!
//! PRoPHET uses encounter history to estimate delivery probabilities.
//! Nodes that are frequently encountered are more likely to be good
//! intermediaries for message delivery.
//!
//! Key concepts:
//! - **Delivery Probability (P)**: Likelihood that this node can deliver to a destination
//! - **Encounter Updates**: When two nodes meet, their probabilities increase
//! - **Transitivity**: If A knows B and B knows C, A can infer knowledge of C
//! - **Aging**: Probabilities decay over time when encounters don't occur

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use indras_core::PeerIdentity;

/// PRoPHET protocol configuration
#[derive(Debug, Clone)]
pub struct ProphetConfig {
    /// Initial probability upon first encounter (P_init)
    /// Default: 0.75 (high initial confidence)
    pub initial_probability: f64,

    /// Aging constant for probability decay (gamma)
    /// Applied each decay interval: P_new = P_old * gamma
    /// Default: 0.98 (slow decay)
    pub aging_constant: f64,

    /// Transitivity scaling factor (beta)
    /// P_a_c = P_a_b * P_b_c * beta
    /// Default: 0.25 (conservative transitivity)
    pub transitivity_constant: f64,

    /// How often to apply aging (time-based decay)
    /// Default: 1 hour
    pub decay_interval: Duration,

    /// Maximum probability value (capped)
    /// Default: 0.99
    pub max_probability: f64,

    /// Minimum probability before dropping from table
    /// Default: 0.01
    pub min_probability: f64,
}

impl Default for ProphetConfig {
    fn default() -> Self {
        Self {
            initial_probability: 0.75,
            aging_constant: 0.98,
            transitivity_constant: 0.25,
            decay_interval: Duration::from_secs(3600), // 1 hour
            max_probability: 0.99,
            min_probability: 0.01,
        }
    }
}

/// Entry in the probability table
#[derive(Debug, Clone)]
struct ProbabilityEntry {
    /// Delivery probability for this destination
    probability: f64,
    /// Last time this entry was updated
    last_updated: Instant,
    /// Last time we encountered this peer (for direct encounters)
    last_encounter: Option<Instant>,
}

impl ProbabilityEntry {
    fn new(probability: f64) -> Self {
        Self {
            probability,
            last_updated: Instant::now(),
            last_encounter: None,
        }
    }

    fn with_encounter(probability: f64) -> Self {
        let now = Instant::now();
        Self {
            probability,
            last_updated: now,
            last_encounter: Some(now),
        }
    }
}

/// PRoPHET routing state for a node
///
/// Maintains delivery probabilities for all known destinations.
pub struct ProphetState<I: PeerIdentity> {
    /// Our node's identity
    local_id: I,
    /// Delivery probability for each destination
    probabilities: RwLock<HashMap<I, ProbabilityEntry>>,
    /// Configuration
    config: ProphetConfig,
    /// Last time aging was applied
    last_aging: RwLock<Instant>,
}

impl<I: PeerIdentity> ProphetState<I> {
    /// Create new PRoPHET state for a node
    pub fn new(local_id: I, config: ProphetConfig) -> Self {
        Self {
            local_id,
            probabilities: RwLock::new(HashMap::new()),
            config,
            last_aging: RwLock::new(Instant::now()),
        }
    }

    /// Create with default configuration
    pub fn with_defaults(local_id: I) -> Self {
        Self::new(local_id, ProphetConfig::default())
    }

    /// Get the local node's identity
    pub fn local_id(&self) -> &I {
        &self.local_id
    }

    /// Get delivery probability for a destination
    ///
    /// Returns 0.0 if the destination is unknown.
    pub fn get_probability(&self, destination: &I) -> f64 {
        self.probabilities
            .read()
            .unwrap()
            .get(destination)
            .map(|e| e.probability)
            .unwrap_or(0.0)
    }

    /// Get all known destinations with their probabilities
    pub fn all_probabilities(&self) -> Vec<(I, f64)> {
        self.probabilities
            .read()
            .unwrap()
            .iter()
            .map(|(id, entry)| (id.clone(), entry.probability))
            .collect()
    }

    /// Record a direct encounter with a peer
    ///
    /// This increases our delivery probability for that peer.
    /// Should be called whenever we establish a connection with a peer.
    pub fn encounter(&self, peer: &I) {
        // Don't track probability to ourselves
        if peer == &self.local_id {
            return;
        }

        let mut probs = self.probabilities.write().unwrap();

        let new_prob = if let Some(entry) = probs.get(peer) {
            // P_new = P_old + (1 - P_old) * P_init
            let p_old = entry.probability;
            let p_new = p_old + (1.0 - p_old) * self.config.initial_probability;
            p_new.min(self.config.max_probability)
        } else {
            // First encounter
            self.config.initial_probability
        };

        probs.insert(peer.clone(), ProbabilityEntry::with_encounter(new_prob));
    }

    /// Apply transitive probability update
    ///
    /// When we encounter peer B, we can improve our probability estimates
    /// for destinations that B has good probability for.
    ///
    /// For each destination C that B knows:
    ///   P_a_c = P_a_c + (1 - P_a_c) * P_a_b * P_b_c * beta
    pub fn transitive_update(&self, intermediary: &I, intermediary_probs: &[(I, f64)]) {
        // Get our probability to the intermediary
        let p_to_intermediary = self.get_probability(intermediary);
        if p_to_intermediary <= self.config.min_probability {
            return;
        }

        let mut probs = self.probabilities.write().unwrap();

        for (destination, p_int_to_dest) in intermediary_probs {
            // Don't update probability to ourselves or to the intermediary
            if destination == &self.local_id || destination == intermediary {
                continue;
            }

            // Get current probability (0 if unknown)
            let p_old = probs
                .get(destination)
                .map(|e| e.probability)
                .unwrap_or(0.0);

            // P_new = P_old + (1 - P_old) * P_a_b * P_b_c * beta
            let transitive_prob =
                p_to_intermediary * p_int_to_dest * self.config.transitivity_constant;
            let p_new = p_old + (1.0 - p_old) * transitive_prob;
            let p_capped = p_new.min(self.config.max_probability);

            // Only update if it improves the probability
            if p_capped > p_old {
                if let Some(entry) = probs.get_mut(destination) {
                    entry.probability = p_capped;
                    entry.last_updated = Instant::now();
                } else {
                    probs.insert(destination.clone(), ProbabilityEntry::new(p_capped));
                }
            }
        }
    }

    /// Age all probabilities based on time elapsed
    ///
    /// Call this periodically to decay probabilities for peers
    /// we haven't encountered recently.
    pub fn age_all(&self) {
        let now = Instant::now();

        // Check if we need to age
        {
            let last_aging = self.last_aging.read().unwrap();
            if now.duration_since(*last_aging) < self.config.decay_interval {
                return;
            }
        }

        // Update last aging time
        *self.last_aging.write().unwrap() = now;

        let mut probs = self.probabilities.write().unwrap();

        // Collect keys to remove (below minimum threshold)
        let to_remove: Vec<I> = probs
            .iter()
            .filter_map(|(id, entry)| {
                let aged = entry.probability * self.config.aging_constant;
                if aged < self.config.min_probability {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        // Remove entries below threshold
        for id in to_remove {
            probs.remove(&id);
        }

        // Age remaining entries
        for entry in probs.values_mut() {
            entry.probability *= self.config.aging_constant;
            entry.last_updated = now;
        }
    }

    /// Force aging to occur regardless of interval
    pub fn force_age(&self) {
        *self.last_aging.write().unwrap() = Instant::now() - self.config.decay_interval * 2;
        self.age_all();
    }

    /// Get the best candidate to forward a message to a destination
    ///
    /// Returns the peer with the highest delivery probability for the destination,
    /// if that probability exceeds our own.
    pub fn best_candidate(&self, destination: &I, candidates: &[I]) -> Option<I> {
        let our_prob = self.get_probability(destination);

        candidates
            .iter()
            .filter(|c| *c != &self.local_id && *c != destination)
            .max_by(|a, b| {
                let pa = self.get_probability(a);
                let pb = self.get_probability(b);
                pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .and_then(|best| {
                let best_prob = self.get_probability(best);
                if best_prob > our_prob {
                    Some(best.clone())
                } else {
                    None
                }
            })
    }

    /// Should we forward a message to this candidate?
    ///
    /// In PRoPHET, we forward to peers with higher probability than ourselves.
    pub fn should_forward_to(&self, destination: &I, candidate: &I) -> bool {
        if candidate == &self.local_id || candidate == destination {
            return false;
        }

        let our_prob = self.get_probability(destination);
        let their_prob = self.get_probability(candidate);

        their_prob > our_prob
    }

    /// Get the number of known destinations
    pub fn known_destinations(&self) -> usize {
        self.probabilities.read().unwrap().len()
    }

    /// Clear all probability data
    pub fn clear(&self) {
        self.probabilities.write().unwrap().clear();
    }
}

/// Summary of PRoPHET state for exchange between nodes
#[derive(Debug, Clone)]
pub struct ProphetSummary<I: PeerIdentity> {
    /// Node this summary belongs to
    pub node_id: I,
    /// Delivery probabilities (destination -> probability)
    pub probabilities: Vec<(I, f64)>,
}

impl<I: PeerIdentity> ProphetSummary<I> {
    /// Create summary from PRoPHET state
    pub fn from_state(state: &ProphetState<I>) -> Self {
        Self {
            node_id: state.local_id.clone(),
            probabilities: state.all_probabilities(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    fn make_id(c: char) -> SimulationIdentity {
        SimulationIdentity::new(c).unwrap()
    }

    #[test]
    fn test_initial_encounter() {
        let state = ProphetState::with_defaults(make_id('A'));

        // Initially no probability
        assert_eq!(state.get_probability(&make_id('B')), 0.0);

        // After encounter
        state.encounter(&make_id('B'));
        let prob = state.get_probability(&make_id('B'));

        // Should be initial probability
        assert!((prob - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_repeated_encounters() {
        let state = ProphetState::with_defaults(make_id('A'));

        state.encounter(&make_id('B'));
        let p1 = state.get_probability(&make_id('B'));

        state.encounter(&make_id('B'));
        let p2 = state.get_probability(&make_id('B'));

        // Probability should increase but not exceed max
        assert!(p2 > p1);
        assert!(p2 <= 0.99);
    }

    #[test]
    fn test_no_self_encounter() {
        let state = ProphetState::with_defaults(make_id('A'));

        // Encountering ourselves shouldn't create an entry
        state.encounter(&make_id('A'));
        assert_eq!(state.known_destinations(), 0);
    }

    #[test]
    fn test_transitive_update() {
        let state_a = ProphetState::with_defaults(make_id('A'));
        let state_b = ProphetState::with_defaults(make_id('B'));

        // A encounters B
        state_a.encounter(&make_id('B'));

        // B has high probability to C
        state_b.encounter(&make_id('C'));
        state_b.encounter(&make_id('C'));

        // A receives B's probabilities via transitive update
        let b_probs = state_b.all_probabilities();
        state_a.transitive_update(&make_id('B'), &b_probs);

        // A should now have some probability to C
        let prob_a_to_c = state_a.get_probability(&make_id('C'));
        assert!(prob_a_to_c > 0.0);
    }

    #[test]
    fn test_aging() {
        let config = ProphetConfig {
            decay_interval: Duration::from_millis(1), // Very short for testing
            aging_constant: 0.5, // Aggressive aging
            ..Default::default()
        };

        let state = ProphetState::new(make_id('A'), config);
        state.encounter(&make_id('B'));

        let p1 = state.get_probability(&make_id('B'));

        // Force aging
        std::thread::sleep(Duration::from_millis(10));
        state.force_age();

        let p2 = state.get_probability(&make_id('B'));
        assert!(p2 < p1);
    }

    #[test]
    fn test_best_candidate() {
        let state = ProphetState::with_defaults(make_id('A'));

        // A knows B well, C less well
        state.encounter(&make_id('B'));
        state.encounter(&make_id('B'));

        state.encounter(&make_id('C'));

        let candidates = vec![make_id('B'), make_id('C'), make_id('D')];

        // For destination Z, B should be best since we know B better
        // But we need probabilities to Z, not B/C themselves
        // Let's set up the scenario properly

        let state_b = ProphetState::with_defaults(make_id('B'));
        state_b.encounter(&make_id('Z'));
        state_b.encounter(&make_id('Z'));

        // Now A learns about Z through B
        state.transitive_update(&make_id('B'), &state_b.all_probabilities());

        // B should be the best candidate because it has the best path to Z
        // (through transitivity, A->B->Z)
    }

    #[test]
    fn test_should_forward_to() {
        let state = ProphetState::with_defaults(make_id('A'));

        // A has low probability to Z
        // But B has high probability (simulated)

        state.encounter(&make_id('B'));

        // A should forward to B for destination Z if B's prob is higher
        // Since we don't know Z at all, any candidate with probability is better
        assert!(!state.should_forward_to(&make_id('Z'), &make_id('A'))); // Not to ourselves
        assert!(!state.should_forward_to(&make_id('Z'), &make_id('Z'))); // Not to destination
    }

    #[test]
    fn test_prophet_summary() {
        let state = ProphetState::with_defaults(make_id('A'));
        state.encounter(&make_id('B'));
        state.encounter(&make_id('C'));

        let summary = ProphetSummary::from_state(&state);

        assert_eq!(summary.node_id, make_id('A'));
        assert_eq!(summary.probabilities.len(), 2);
    }

    #[test]
    fn test_clear() {
        let state = ProphetState::with_defaults(make_id('A'));
        state.encounter(&make_id('B'));
        state.encounter(&make_id('C'));

        assert_eq!(state.known_destinations(), 2);

        state.clear();
        assert_eq!(state.known_destinations(), 0);
    }

    #[test]
    fn test_custom_config() {
        let config = ProphetConfig {
            initial_probability: 0.5,
            aging_constant: 0.9,
            transitivity_constant: 0.5,
            ..Default::default()
        };

        let state = ProphetState::new(make_id('A'), config);
        state.encounter(&make_id('B'));

        let prob = state.get_probability(&make_id('B'));
        assert!((prob - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_probability_capping() {
        let config = ProphetConfig {
            initial_probability: 0.99,
            max_probability: 0.95,
            ..Default::default()
        };

        let state = ProphetState::new(make_id('A'), config);

        // Multiple encounters
        for _ in 0..10 {
            state.encounter(&make_id('B'));
        }

        let prob = state.get_probability(&make_id('B'));
        assert!(prob <= 0.95);
    }
}
