use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod document;
pub mod instance;

pub use document::DocumentState;
pub use instance::{format_network_event, InstanceState, PacketAnimation};

/// A data point for charts
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
}

/// Test category detection for adaptive panels
#[allow(dead_code)] // Some variants reserved for future categorization
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TestCategory {
    #[default]
    Unknown,
    /// PQ crypto tests (pq_*, crypto_stress)
    PQCrypto,
    /// Routing/messaging tests
    Routing,
    /// Transport tests
    Transport,
    /// Sync tests
    Sync,
    /// Integration tests (full stack, partition, scalability)
    Integration,
}

#[allow(dead_code)] // Reserved for future test categorization feature
impl TestCategory {
    /// Detect test category from scenario name
    pub fn from_scenario_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();

        if name_lower.starts_with("pq_") || name_lower.contains("crypto") {
            TestCategory::PQCrypto
        } else if name_lower.contains("routing") || name_lower.contains("messaging") {
            TestCategory::Routing
        } else if name_lower.contains("transport") {
            TestCategory::Transport
        } else if name_lower.contains("sync") {
            TestCategory::Sync
        } else if name_lower.contains("integration")
            || name_lower.contains("partition")
            || name_lower.contains("scalability")
        {
            TestCategory::Integration
        } else {
            TestCategory::Unknown
        }
    }

    /// Get display name for the category
    pub fn display_name(&self) -> &'static str {
        match self {
            TestCategory::Unknown => "General",
            TestCategory::PQCrypto => "Post-Quantum Crypto",
            TestCategory::Routing => "Routing & Messaging",
            TestCategory::Transport => "Transport",
            TestCategory::Sync => "Synchronization",
            TestCategory::Integration => "Integration",
        }
    }
}

/// Metrics history for charts (ring buffer with max capacity)
#[allow(dead_code)] // Reserved for future metrics visualization
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricsHistory {
    /// Maximum number of data points to keep
    pub max_points: usize,
    /// Operations per second over time
    pub ops_per_second: Vec<DataPoint>,
    /// Signature latency over time
    pub signature_latency: Vec<DataPoint>,
    /// Delivery rate over time
    pub delivery_rate: Vec<DataPoint>,
    /// Average latency over time
    pub avg_latency: Vec<DataPoint>,
}

#[allow(dead_code)] // Reserved for future metrics visualization
impl MetricsHistory {
    /// Create a new metrics history with specified capacity
    pub fn new(max_points: usize) -> Self {
        Self {
            max_points,
            ops_per_second: Vec::with_capacity(max_points),
            signature_latency: Vec::with_capacity(max_points),
            delivery_rate: Vec::with_capacity(max_points),
            avg_latency: Vec::with_capacity(max_points),
        }
    }

    /// Record metrics at a given tick
    pub fn record(&mut self, tick: u64, metrics: &SimMetrics) {
        let x = tick as f64;

        // Record ops per second
        if metrics.ops_per_second > 0.0 {
            self.push_point(
                &mut self.ops_per_second.clone(),
                DataPoint {
                    x,
                    y: metrics.ops_per_second,
                },
            );
            self.ops_per_second = self.ops_per_second.clone();
        }

        // Record signature latency
        if metrics.avg_sign_latency_us > 0.0 {
            let mut history = self.signature_latency.clone();
            self.push_point(
                &mut history,
                DataPoint {
                    x,
                    y: metrics.avg_sign_latency_us,
                },
            );
            self.signature_latency = history;
        }

        // Record delivery rate
        if metrics.messages_sent > 0 {
            let rate = metrics.messages_delivered as f64 / metrics.messages_sent as f64;
            let mut history = self.delivery_rate.clone();
            self.push_point(&mut history, DataPoint { x, y: rate });
            self.delivery_rate = history;
        }

        // Record average latency
        if metrics.avg_latency > 0.0 {
            let mut history = self.avg_latency.clone();
            self.push_point(
                &mut history,
                DataPoint {
                    x,
                    y: metrics.avg_latency,
                },
            );
            self.avg_latency = history;
        }
    }

    fn push_point(&self, vec: &mut Vec<DataPoint>, point: DataPoint) {
        if vec.len() >= self.max_points {
            vec.remove(0);
        }
        vec.push(point);
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.ops_per_second.clear();
        self.signature_latency.clear();
        self.delivery_rate.clear();
        self.avg_latency.clear();
    }
}

/// Phase marker for multi-phase tests
#[allow(dead_code)] // Reserved for future multi-phase visualization
#[derive(Clone, Debug)]
pub struct PhaseMarker {
    pub tick: u64,
    pub name: String,
    pub phase_number: u32,
}

/// Tab selection for the dashboard view
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    /// Metrics view for stress test results
    #[default]
    Metrics,
    /// Live network simulation visualization view
    Simulations,
    /// Documents/CRDT sync visualization view
    Documents,
}

/// Event severity/type for display purposes
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EventType {
    Info,
    Warning,
    Error,
    Success,
}

/// Core simulation metrics matching the stats from simulation runs
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SimMetrics {
    // Message statistics
    pub messages_sent: u64,
    pub messages_delivered: u64,
    pub messages_dropped: u64,

    // Performance metrics
    pub delivery_rate: f64,
    pub avg_latency: f64,
    pub avg_latency_ticks: u64,
    pub avg_hops: f64,

    // Backpropagation statistics
    pub backprops_completed: u64,
    pub backprops_timed_out: u64,

    // Post-quantum cryptography statistics
    pub pq_signatures_created: u64,
    pub pq_signatures_verified: u64,
    pub pq_signature_failures: u64,
    pub signature_verifications: u64,
    pub signature_failures: u64,

    // PQ latencies (microseconds)
    pub avg_sign_latency_us: f64,
    pub avg_verify_latency_us: f64,
    pub avg_encap_latency_us: f64,
    pub avg_decap_latency_us: f64,

    // KEM statistics
    pub kem_encapsulations: u64,
    pub kem_decapsulations: u64,
    pub kem_failures: u64,

    // Failure rates
    pub signature_failure_rate: f64,
    pub kem_failure_rate: f64,

    // Throughput
    pub ops_per_second: f64,

    // Simulation progress
    pub current_tick: u64,
    pub max_ticks: u64,
}

/// Events that occur during simulation execution
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimEvent {
    pub tick: u64,
    pub event_type: EventType,
    pub description: String,
}

/// Result of a test scenario execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestResult {
    /// Scenario name (e.g., "pq_baseline_benchmark")
    pub scenario: String,

    /// Stress level (Quick, Medium, Full)
    pub level: String,

    /// Whether all assertions passed
    pub passed: bool,

    /// Collected metrics from the simulation
    pub metrics: SimMetrics,

    /// Total execution time in seconds
    pub duration_secs: f64,

    /// When the test was executed
    pub timestamp: DateTime<Utc>,

    /// Any errors or assertion failures
    pub errors: Vec<String>,
}

/// Stress level for test scenarios
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StressLevel {
    /// Quick smoke test (minimal load)
    #[default]
    Quick,

    /// Medium load test
    Medium,

    /// Full stress test (maximum load)
    Full,
}

impl StressLevel {
    /// Get all available stress levels
    #[allow(dead_code)] // Reserved for future UI enumeration
    pub fn all() -> Vec<StressLevel> {
        vec![StressLevel::Quick, StressLevel::Medium, StressLevel::Full]
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            StressLevel::Quick => "quick",
            StressLevel::Medium => "medium",
            StressLevel::Full => "full",
        }
    }
}

impl std::fmt::Display for StressLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for StressLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "quick" => Ok(StressLevel::Quick),
            "medium" => Ok(StressLevel::Medium),
            "full" => Ok(StressLevel::Full),
            _ => Err(format!("Invalid stress level: {}", s)),
        }
    }
}
