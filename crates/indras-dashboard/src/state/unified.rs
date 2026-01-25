//! Unified playback state for the control bar
//!
//! Provides a common interface across all tab types for the unified control bar.

use super::{DiscoveryState, DocumentState, InstanceState, SDKState, SimMetrics, Tab};

/// Which context is currently active
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActiveContext {
    #[default]
    None,
    Simulations,
    Documents,
    SDK,
    Metrics,
    Discovery,
}

impl ActiveContext {
    /// Get display name for the context
    pub fn display_name(&self) -> &'static str {
        match self {
            ActiveContext::None => "No Context",
            ActiveContext::Simulations => "Network Simulation",
            ActiveContext::Documents => "Document Sync",
            ActiveContext::SDK => "SDK Stress Test",
            ActiveContext::Metrics => "Stress Test",
            ActiveContext::Discovery => "Discovery Test",
        }
    }

    /// Get icon for the context
    pub fn icon(&self) -> &'static str {
        match self {
            ActiveContext::None => "○",
            ActiveContext::Simulations => "◉",
            ActiveContext::Documents => "📄",
            ActiveContext::SDK => "⚡",
            ActiveContext::Metrics => "📊",
            ActiveContext::Discovery => "🔍",
        }
    }
}

/// Unified playback state that works across all tabs
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnifiedPlaybackState {
    /// Which context is active
    pub context: ActiveContext,
    /// Human-readable name of the current scenario/test
    pub context_name: String,
    /// Something is loaded and ready
    pub is_active: bool,
    /// Currently running/playing
    pub is_running: bool,
    /// Loaded but paused (for Simulations/Documents)
    pub is_paused: bool,
    /// Current tick/step
    pub current_tick: u64,
    /// Maximum ticks/steps
    pub max_ticks: u64,
    /// Playback speed multiplier
    pub playback_speed: f64,
    /// Step button should be enabled
    pub can_step: bool,
    /// Play/Pause button should be enabled
    pub can_play: bool,
    /// Reset button should be enabled
    pub can_reset: bool,
    /// Show speed control slider
    pub has_speed_control: bool,
    /// Show stress level selector
    pub has_stress_control: bool,
    /// Current stress level (quick/medium/full)
    pub stress_level: String,
}

impl UnifiedPlaybackState {
    /// Create state from the Simulations tab
    pub fn from_simulations(state: &InstanceState) -> Self {
        let has_sim = state.simulation.is_some();
        let scenario_name = state
            .scenario_name
            .clone()
            .unwrap_or_else(|| "No simulation".to_string());

        Self {
            context: ActiveContext::Simulations,
            context_name: scenario_name,
            is_active: has_sim,
            is_running: has_sim && !state.paused,
            is_paused: has_sim && state.paused,
            current_tick: state.current_tick(),
            max_ticks: state.max_ticks(),
            playback_speed: state.playback_speed,
            can_step: has_sim && state.paused,
            can_play: has_sim,
            can_reset: has_sim,
            has_speed_control: true,
            has_stress_control: false,
            stress_level: String::new(),
        }
    }

    /// Create state from the Documents tab
    pub fn from_documents(state: &DocumentState) -> Self {
        let has_scenario = state.scenario_name.is_some();
        let scenario_name = state
            .scenario_name
            .clone()
            .unwrap_or_else(|| "No scenario".to_string());

        Self {
            context: ActiveContext::Documents,
            context_name: scenario_name,
            is_active: has_scenario,
            is_running: state.running,
            is_paused: has_scenario && !state.running,
            current_tick: state.current_step as u64,
            max_ticks: state.total_steps as u64,
            playback_speed: 1.0,
            can_step: has_scenario && !state.running,
            can_play: has_scenario,
            can_reset: has_scenario,
            has_speed_control: false,
            has_stress_control: false,
            stress_level: String::new(),
        }
    }

    /// Create state from the SDK tab
    pub fn from_sdk(state: &SDKState) -> Self {
        let dashboard_name = state.current_dashboard.display_name().to_string();

        Self {
            context: ActiveContext::SDK,
            context_name: dashboard_name,
            is_active: true,
            is_running: state.running,
            is_paused: false,
            current_tick: state.metrics.current_tick,
            max_ticks: state.metrics.max_ticks,
            playback_speed: 1.0,
            can_step: false,
            can_play: true,
            can_reset: state.running,
            has_speed_control: false,
            has_stress_control: true,
            stress_level: state.stress_level.clone(),
        }
    }

    /// Create state from the Metrics tab
    pub fn from_metrics(metrics: &SimMetrics, running: bool, scenario_name: Option<&str>, stress_level: &str) -> Self {
        let has_scenario = scenario_name.is_some();
        let name = scenario_name
            .unwrap_or("No scenario")
            .to_string();

        Self {
            context: ActiveContext::Metrics,
            context_name: name,
            is_active: has_scenario,
            is_running: running,
            is_paused: false,
            current_tick: metrics.current_tick,
            max_ticks: metrics.max_ticks,
            playback_speed: 1.0,
            can_step: false,
            can_play: has_scenario,
            can_reset: running,
            has_speed_control: false,
            has_stress_control: true,
            stress_level: stress_level.to_string(),
        }
    }

    /// Create state from the Discovery tab
    pub fn from_discovery(state: &DiscoveryState) -> Self {
        let dashboard_name = state.current_dashboard.display_name().to_string();

        Self {
            context: ActiveContext::Discovery,
            context_name: dashboard_name,
            is_active: true,
            is_running: state.running,
            is_paused: false,
            current_tick: state.metrics.current_tick,
            max_ticks: state.metrics.max_ticks,
            playback_speed: 1.0,
            can_step: false,
            can_play: true,
            can_reset: state.running,
            has_speed_control: false,
            has_stress_control: true,
            stress_level: state.stress_level.clone(),
        }
    }

    /// Get the current state based on active tab
    pub fn from_tab(
        tab: Tab,
        instance_state: &InstanceState,
        document_state: &DocumentState,
        sdk_state: &SDKState,
        discovery_state: &DiscoveryState,
        metrics: &SimMetrics,
        metrics_running: bool,
        metrics_scenario: Option<&str>,
        metrics_stress_level: &str,
    ) -> Self {
        match tab {
            Tab::Simulations => Self::from_simulations(instance_state),
            Tab::Documents => Self::from_documents(document_state),
            Tab::SDK => Self::from_sdk(sdk_state),
            Tab::Discovery => Self::from_discovery(discovery_state),
            Tab::Metrics => Self::from_metrics(metrics, metrics_running, metrics_scenario, metrics_stress_level),
        }
    }

    /// Calculate progress percentage (0.0 to 100.0)
    pub fn progress_percent(&self) -> f64 {
        if self.max_ticks == 0 {
            0.0
        } else {
            (self.current_tick as f64 / self.max_ticks as f64) * 100.0
        }
    }

    /// Get status text
    pub fn status_text(&self) -> &'static str {
        if !self.is_active {
            "Idle"
        } else if self.is_running {
            "Running"
        } else if self.is_paused {
            "Paused"
        } else {
            "Ready"
        }
    }
}
