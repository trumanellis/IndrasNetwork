//! SDK-specific state for the SDK dashboard tab.
//!
//! Provides state management for the three SDK stress test dashboards:
//! - Network Lifecycle (sdk_stress.lua)
//! - Document Operations (sdk_document_stress.lua)
//! - Messaging & Threading (sdk_messaging_stress.lua)

use crate::state::{SimEvent, SimMetrics};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Which SDK dashboard is currently active
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SDKDashboard {
    /// Network/Realm lifecycle stress test
    #[default]
    NetworkLifecycle,
    /// Document CRDT operations stress test
    DocumentOperations,
    /// Messaging and threading stress test
    Messaging,
}

impl SDKDashboard {
    /// Get the scenario file name for this dashboard
    pub fn scenario_name(&self) -> &'static str {
        match self {
            SDKDashboard::NetworkLifecycle => "sdk_stress.lua",
            SDKDashboard::DocumentOperations => "sdk_document_stress.lua",
            SDKDashboard::Messaging => "sdk_messaging_stress.lua",
        }
    }

    /// Get display name for the dashboard
    pub fn display_name(&self) -> &'static str {
        match self {
            SDKDashboard::NetworkLifecycle => "Network Lifecycle",
            SDKDashboard::DocumentOperations => "Document Operations",
            SDKDashboard::Messaging => "Messaging & Threading",
        }
    }

    /// Get description for the dashboard
    pub fn description(&self) -> &'static str {
        match self {
            SDKDashboard::NetworkLifecycle => {
                "Tests network creation, realm formation, peer joins, and interface lifecycle"
            }
            SDKDashboard::DocumentOperations => {
                "Tests CRDT document operations, concurrent edits, sync, and persistence"
            }
            SDKDashboard::Messaging => {
                "Tests message delivery, reply threading, reactions, and member presence"
            }
        }
    }

    /// Get icon for the dashboard
    pub fn icon(&self) -> &'static str {
        match self {
            SDKDashboard::NetworkLifecycle => "🌐",
            SDKDashboard::DocumentOperations => "📄",
            SDKDashboard::Messaging => "💬",
        }
    }
}

/// State for the SDK tab
#[derive(Clone, Debug, Default)]
pub struct SDKState {
    /// Currently selected dashboard
    pub current_dashboard: SDKDashboard,
    /// Whether a test is currently running
    pub running: bool,
    /// Current stress level
    pub stress_level: String,
    /// Events from the current test run
    pub events: Vec<SimEvent>,
    /// Current metrics
    pub metrics: SimMetrics,
    /// Phase-specific metrics for multi-phase tests
    pub phase_metrics: HashMap<String, PhaseMetrics>,
    /// Current phase name (if applicable)
    pub current_phase: Option<String>,
    /// Total phases in the current test
    pub total_phases: usize,
    /// Current phase number (1-indexed)
    pub current_phase_number: usize,
}

impl SDKState {
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
        self.phase_metrics.clear();
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
    #[allow(dead_code)] // Reserved for future phase-aware updates
    pub fn set_phase(&mut self, phase_name: &str, phase_number: usize, total: usize) {
        self.current_phase = Some(phase_name.to_string());
        self.current_phase_number = phase_number;
        self.total_phases = total;
    }
}

/// Metrics for a specific phase of a multi-phase test
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhaseMetrics {
    /// Phase name
    pub name: String,
    /// Phase-specific operation count
    pub operations: u64,
    /// Phase-specific success count
    pub successes: u64,
    /// Phase-specific failure count
    pub failures: u64,
    /// Phase latency p50 (milliseconds)
    pub latency_p50_ms: f64,
    /// Phase latency p95 (milliseconds)
    pub latency_p95_ms: f64,
    /// Phase latency p99 (milliseconds)
    pub latency_p99_ms: f64,
    /// Phase duration (seconds)
    pub duration_secs: f64,
    /// Phase-specific throughput
    pub throughput: f64,
}

// ============================================================================
// Network Lifecycle Dashboard Metrics
// ============================================================================

/// Metrics specific to network lifecycle stress test
#[allow(dead_code)] // Reserved for future enhanced metrics display
#[derive(Clone, Debug, Default)]
pub struct NetworkLifecycleMetrics {
    /// Number of networks created
    pub networks_created: u64,
    /// Number of networks started
    pub networks_started: u64,
    /// Number of networks stopped
    pub networks_stopped: u64,
    /// Number of realms created
    pub realms_created: u64,
    /// Number of realm joins
    pub realm_joins: u64,
    /// Number of active peers
    pub active_peers: u64,
    /// Average network start time (ms)
    pub avg_network_start_ms: f64,
    /// Average realm creation time (ms)
    pub avg_realm_create_ms: f64,
    /// Average join time (ms)
    pub avg_join_ms: f64,
}

// ============================================================================
// Document Operations Dashboard Metrics
// ============================================================================

/// Metrics specific to document operations stress test
#[allow(dead_code)] // Reserved for future enhanced metrics display
#[derive(Clone, Debug, Default)]
pub struct DocumentOperationsMetrics {
    /// Number of documents created
    pub documents_created: u64,
    /// Number of document updates
    pub document_updates: u64,
    /// Number of concurrent edit conflicts resolved
    pub conflicts_resolved: u64,
    /// Number of successful syncs
    pub syncs_completed: u64,
    /// Number of sync failures
    pub sync_failures: u64,
    /// Average sync time (ms)
    pub avg_sync_time_ms: f64,
    /// Average update latency (ms)
    pub avg_update_latency_ms: f64,
    /// Data size (bytes)
    pub total_data_bytes: u64,
    /// Convergence rate (0.0-1.0)
    pub convergence_rate: f64,
}

/// Schema type being tested
#[allow(dead_code)] // Reserved for future schema-specific display
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DocumentSchema {
    #[default]
    Counter,
    TaskList,
    TextEditor,
    JsonState,
    NestedObject,
}

#[allow(dead_code)]
impl DocumentSchema {
    pub fn display_name(&self) -> &'static str {
        match self {
            DocumentSchema::Counter => "Counter",
            DocumentSchema::TaskList => "Task List",
            DocumentSchema::TextEditor => "Text Editor",
            DocumentSchema::JsonState => "JSON State",
            DocumentSchema::NestedObject => "Nested Object",
        }
    }
}

// ============================================================================
// Messaging Dashboard Metrics
// ============================================================================

/// Metrics specific to messaging stress test
#[allow(dead_code)] // Reserved for future enhanced metrics display
#[derive(Clone, Debug, Default)]
pub struct MessagingMetrics {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages delivered
    pub messages_delivered: u64,
    /// Messages pending delivery
    pub messages_pending: u64,
    /// Reply threads created
    pub reply_threads: u64,
    /// Reactions sent
    pub reactions_sent: u64,
    /// Average delivery time (ms)
    pub avg_delivery_ms: f64,
    /// Delivery rate (0.0-1.0)
    pub delivery_rate: f64,
    /// Members online
    pub members_online: u64,
    /// Member join events
    pub member_joins: u64,
    /// Member leave events
    pub member_leaves: u64,
}

/// Content type being tested
#[allow(dead_code)] // Reserved for future content-type-specific display
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ContentType {
    #[default]
    Text,
    Binary,
    Json,
    System,
}

#[allow(dead_code)]
impl ContentType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ContentType::Text => "Text",
            ContentType::Binary => "Binary",
            ContentType::Json => "JSON",
            ContentType::System => "System",
        }
    }
}
