//! Discovery-specific state for the Discovery dashboard tab.
//!
//! Provides state management for peer discovery scenario dashboards:
//! - TwoPeer (discovery_two_peer.lua) - Basic mutual discovery
//! - PeerGroup (discovery_peer_group.lua) - Multi-peer realms
//! - LateJoiner (discovery_late_joiner.lua) - IntroductionRequest
//! - RateLimit (discovery_rate_limit.lua) - Rate limiting
//! - Reconnect (discovery_reconnect.lua) - Disconnect/reconnect
//! - PQKeys (discovery_pq_keys.lua) - PQ key exchange validation
//! - Stress (discovery_stress.lua) - Churn stress test

use crate::state::{SimEvent, SimMetrics};
use std::collections::HashMap;

/// Which Discovery dashboard is currently active
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiscoveryDashboard {
    /// Basic mutual discovery between two peers
    #[default]
    TwoPeer,
    /// Multi-peer realm formation
    PeerGroup,
    /// Late joiner discovery via IntroductionRequest
    LateJoiner,
    /// Rate limiting verification
    RateLimit,
    /// Disconnect/reconnect handling
    Reconnect,
    /// Post-quantum key exchange validation
    PQKeys,
    /// High-churn stress test
    Stress,
}

impl DiscoveryDashboard {
    /// Get the scenario file name for this dashboard
    pub fn scenario_name(&self) -> &'static str {
        match self {
            DiscoveryDashboard::TwoPeer => "discovery_two_peer.lua",
            DiscoveryDashboard::PeerGroup => "discovery_peer_group.lua",
            DiscoveryDashboard::LateJoiner => "discovery_late_joiner.lua",
            DiscoveryDashboard::RateLimit => "discovery_rate_limit.lua",
            DiscoveryDashboard::Reconnect => "discovery_reconnect.lua",
            DiscoveryDashboard::PQKeys => "discovery_pq_keys.lua",
            DiscoveryDashboard::Stress => "discovery_stress.lua",
        }
    }

    /// Get display name for the dashboard
    pub fn display_name(&self) -> &'static str {
        match self {
            DiscoveryDashboard::TwoPeer => "Two Peer",
            DiscoveryDashboard::PeerGroup => "Peer Group",
            DiscoveryDashboard::LateJoiner => "Late Joiner",
            DiscoveryDashboard::RateLimit => "Rate Limit",
            DiscoveryDashboard::Reconnect => "Reconnect",
            DiscoveryDashboard::PQKeys => "PQ Keys",
            DiscoveryDashboard::Stress => "Stress Test",
        }
    }

    /// Get description for the dashboard
    pub fn description(&self) -> &'static str {
        match self {
            DiscoveryDashboard::TwoPeer => {
                "Basic mutual peer discovery and realm formation"
            }
            DiscoveryDashboard::PeerGroup => {
                "Multi-peer discovery with overlapping realm formation"
            }
            DiscoveryDashboard::LateJoiner => {
                "Late joiner discovery via IntroductionRequest messages"
            }
            DiscoveryDashboard::RateLimit => {
                "Rate limiting verification with 30-tick window"
            }
            DiscoveryDashboard::Reconnect => {
                "Peer disconnect and reconnect handling"
            }
            DiscoveryDashboard::PQKeys => {
                "Post-quantum key exchange (ML-KEM-768, ML-DSA-65)"
            }
            DiscoveryDashboard::Stress => {
                "High-churn stress test with convergence tracking"
            }
        }
    }

    /// Get icon for the dashboard
    pub fn icon(&self) -> &'static str {
        match self {
            DiscoveryDashboard::TwoPeer => "👥",
            DiscoveryDashboard::PeerGroup => "🏘️",
            DiscoveryDashboard::LateJoiner => "🚪",
            DiscoveryDashboard::RateLimit => "🚦",
            DiscoveryDashboard::Reconnect => "🔄",
            DiscoveryDashboard::PQKeys => "🔐",
            DiscoveryDashboard::Stress => "🔥",
        }
    }
}

/// State for the Discovery tab
#[derive(Clone, Debug, Default)]
pub struct DiscoveryState {
    /// Currently selected dashboard
    pub current_dashboard: DiscoveryDashboard,
    /// Whether a test is currently running
    pub running: bool,
    /// Current stress level
    pub stress_level: String,
    /// Events from the current test run
    pub events: Vec<SimEvent>,
    /// Current metrics
    pub metrics: SimMetrics,
    /// Discovery matrix - who knows whom (peer_id -> set of known peer_ids)
    pub discovery_matrix: HashMap<String, HashMap<String, bool>>,
    /// List of formed realm IDs
    pub realms_formed: Vec<String>,
    /// Whether full convergence has been achieved
    pub convergence_achieved: bool,
    /// Tick at which convergence was achieved
    pub convergence_tick: Option<u64>,
    /// Current phase name (if applicable)
    pub current_phase: Option<String>,
    /// Total phases in the current test
    pub total_phases: usize,
    /// Current phase number (1-indexed)
    pub current_phase_number: usize,
}

impl DiscoveryState {
    pub fn new() -> Self {
        Self {
            stress_level: "medium".to_string(),
            ..Default::default()
        }
    }

    /// Reset state for a new test run
    pub fn reset(&mut self) {
        self.running = false;
        self.events.clear();
        self.metrics = SimMetrics::default();
        self.discovery_matrix.clear();
        self.realms_formed.clear();
        self.convergence_achieved = false;
        self.convergence_tick = None;
        self.current_phase = None;
        self.total_phases = 0;
        self.current_phase_number = 0;
    }

    /// Add an event to the event log
    pub fn add_event(&mut self, event: SimEvent) {
        self.events.push(event);
        // Keep only the most recent 200 events
        if self.events.len() > 200 {
            self.events.remove(0);
        }
    }

    /// Set the current phase
    #[allow(dead_code)]
    pub fn set_phase(&mut self, phase_name: &str, phase_number: usize, total: usize) {
        self.current_phase = Some(phase_name.to_string());
        self.current_phase_number = phase_number;
        self.total_phases = total;
    }

    /// Update discovery matrix from a peer discovery event
    #[allow(dead_code)]
    pub fn record_discovery(&mut self, from_peer: &str, discovered_peer: &str) {
        let entry = self
            .discovery_matrix
            .entry(from_peer.to_string())
            .or_insert_with(HashMap::new);
        entry.insert(discovered_peer.to_string(), true);
    }

    /// Calculate discovery completeness (0.0 - 1.0)
    pub fn discovery_completeness(&self) -> f64 {
        if self.discovery_matrix.is_empty() {
            return 0.0;
        }

        let peer_count = self.discovery_matrix.len();
        if peer_count <= 1 {
            return 1.0;
        }

        let expected_total = peer_count * (peer_count - 1); // Each peer should know all others
        let actual_total: usize = self
            .discovery_matrix
            .values()
            .map(|known| known.len())
            .sum();

        if expected_total == 0 {
            1.0
        } else {
            (actual_total as f64 / expected_total as f64).min(1.0)
        }
    }

    /// Get list of peers from the discovery matrix
    pub fn peer_list(&self) -> Vec<String> {
        self.discovery_matrix.keys().cloned().collect()
    }
}

/// Discovery-specific metrics parsed from simulation output
#[derive(Clone, Debug, Default)]
pub struct DiscoveryMetrics {
    /// Number of peers that have been discovered
    pub peers_discovered: u64,
    /// Number of discovery failures
    pub discovery_failures: u64,
    /// Discovery completeness (0.0 - 1.0)
    pub discovery_completeness: f64,
    /// PQ key exchange completeness (0.0 - 1.0)
    pub pq_key_completeness: f64,
    /// IntroductionRequest messages sent
    pub introduction_requests_sent: u64,
    /// IntroductionResponse messages received
    pub introduction_responses_received: u64,
    /// Number of requests that were rate-limited
    pub rate_limited_count: u64,
    /// Rate limit violations (exceeded window)
    pub rate_limit_violations: u64,
    /// Number of realms available
    pub realms_available: u64,
    /// Number of peer churn events (joins + leaves)
    pub churn_events: u64,
    /// Number of successful reconnects
    pub reconnects: u64,
    /// Discovery latency p99 in ticks
    pub discovery_latency_p99_ticks: u64,
}

/// Status of a peer in the discovery visualization
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PeerDiscoveryStatus {
    #[default]
    Offline,
    Online,
    Discovering,
    Discovered,
}

impl PeerDiscoveryStatus {
    pub fn css_class(&self) -> &'static str {
        match self {
            PeerDiscoveryStatus::Offline => "offline",
            PeerDiscoveryStatus::Online => "online",
            PeerDiscoveryStatus::Discovering => "discovering",
            PeerDiscoveryStatus::Discovered => "discovered",
        }
    }
}

/// PQ Key exchange status for a peer pair
#[derive(Clone, Debug, Default)]
pub struct PQKeyStatus {
    /// ML-KEM-768 encapsulation complete
    pub kem_complete: bool,
    /// ML-DSA-65 signature verified
    pub signature_verified: bool,
    /// Encapsulated key size (bytes)
    pub kem_ciphertext_size: usize,
    /// Public key size (bytes)
    pub public_key_size: usize,
}

impl PQKeyStatus {
    /// Standard ML-KEM-768 ciphertext size
    pub const ML_KEM_768_CIPHERTEXT_SIZE: usize = 1088;
    /// Standard ML-DSA-65 public key size
    pub const ML_DSA_65_PUBLIC_KEY_SIZE: usize = 1952;
}
