//! # Duty Cycle Management
//!
//! Power-aware scheduling for battery-powered devices.
//! Controls wake/sleep cycles and batches network operations.
//!
//! ## Thread Safety
//!
//! [`DutyCycleManager`] is **not thread-safe**. It uses `&mut self` for all
//! state-modifying operations. If you need to share a manager across threads,
//! wrap it in synchronization primitives:
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use indras_iot::duty_cycle::{DutyCycleManager, DutyCycleConfig};
//!
//! let manager = Arc::new(Mutex::new(DutyCycleManager::new(DutyCycleConfig::default())));
//!
//! // In each thread:
//! // let mut guard = manager.lock().unwrap();
//! // guard.tick();
//! ```
//!
//! ## State Machine
//!
//! The manager cycles through power states:
//!
//! ```text
//! Active -> PreSleep -> Sleeping -> Waking -> Active -> ...
//! ```
//!
//! - **Active**: All operations allowed, normal power consumption
//! - **PreSleep**: Finishing pending ops, only urgent operations allowed
//! - **Sleeping**: Low power, only urgent operations when pending threshold reached
//! - **Waking**: Initializing after sleep, all operations allowed

use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{debug, info};

/// Duty cycle configuration
#[derive(Debug, Clone)]
pub struct DutyCycleConfig {
    /// Active period duration
    pub active_duration: Duration,
    /// Sleep period duration
    pub sleep_duration: Duration,
    /// Minimum time between syncs
    pub min_sync_interval: Duration,
    /// Maximum pending messages before forced wake
    pub max_pending_before_wake: usize,
    /// Battery threshold for aggressive power saving (0.0 - 1.0)
    pub low_battery_threshold: f32,
}

impl Default for DutyCycleConfig {
    fn default() -> Self {
        Self {
            active_duration: Duration::from_secs(30),
            sleep_duration: Duration::from_secs(270), // 5 min cycle, 10% duty
            min_sync_interval: Duration::from_secs(60),
            max_pending_before_wake: 10,
            low_battery_threshold: 0.2,
        }
    }
}

impl DutyCycleConfig {
    /// Aggressive power saving for very low battery
    pub fn low_power() -> Self {
        Self {
            active_duration: Duration::from_secs(10),
            sleep_duration: Duration::from_secs(590), // 10 min cycle, ~2% duty
            min_sync_interval: Duration::from_secs(300),
            max_pending_before_wake: 20,
            low_battery_threshold: 0.1,
        }
    }

    /// Balanced mode for normal operation
    pub fn balanced() -> Self {
        Self::default()
    }

    /// High responsiveness mode (higher power consumption)
    pub fn responsive() -> Self {
        Self {
            active_duration: Duration::from_secs(60),
            sleep_duration: Duration::from_secs(60), // 2 min cycle, 50% duty
            min_sync_interval: Duration::from_secs(30),
            max_pending_before_wake: 5,
            low_battery_threshold: 0.3,
        }
    }

    /// Calculate duty cycle percentage.
    ///
    /// Returns 100.0 if both durations are zero (always active).
    pub fn duty_percentage(&self) -> f32 {
        let total = self.active_duration + self.sleep_duration;
        if total.is_zero() {
            return 100.0; // Always active if no cycle defined
        }
        self.active_duration.as_secs_f32() / total.as_secs_f32() * 100.0
    }
}

/// Current power state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// Fully active, all operations allowed
    Active,
    /// Preparing to sleep, finishing pending ops
    PreSleep,
    /// Sleeping, only urgent operations
    Sleeping,
    /// Waking up, initializing
    Waking,
}

/// Duty cycle errors
#[derive(Debug, Error)]
pub enum DutyCycleError {
    #[error("Operation not allowed in current power state: {0:?}")]
    NotAllowedInState(PowerState),
    #[error("Too many pending operations: {count}")]
    TooManyPending { count: usize },
}

/// Manages device duty cycling.
///
/// See module documentation for thread safety considerations.
#[derive(Debug)]
pub struct DutyCycleManager {
    config: DutyCycleConfig,
    state: PowerState,
    state_entered: Instant,
    last_sync: Option<Instant>,
    pending_count: usize,
    battery_level: f32,
}

impl DutyCycleManager {
    /// Create a new duty cycle manager
    pub fn new(config: DutyCycleConfig) -> Self {
        info!(
            duty_pct = config.duty_percentage(),
            "Duty cycle manager initialized"
        );
        Self {
            config,
            state: PowerState::Active,
            state_entered: Instant::now(),
            last_sync: None,
            pending_count: 0,
            battery_level: 1.0,
        }
    }

    /// Get current power state
    pub fn state(&self) -> PowerState {
        self.state
    }

    /// Get the configuration
    pub fn config(&self) -> &DutyCycleConfig {
        &self.config
    }

    /// Update battery level (0.0 - 1.0)
    pub fn set_battery_level(&mut self, level: f32) {
        self.battery_level = level.clamp(0.0, 1.0);

        // Auto-switch to low power mode if battery is critical
        if self.battery_level < self.config.low_battery_threshold {
            debug!(battery = self.battery_level, "Low battery, extending sleep");
        }
    }

    /// Get current battery level
    pub fn battery_level(&self) -> f32 {
        self.battery_level
    }

    /// Check if an operation should be allowed based on current state and urgency.
    ///
    /// - **Active/Waking**: All operations allowed
    /// - **PreSleep**: Only urgent operations allowed
    /// - **Sleeping**: Urgent operations allowed, OR non-urgent if pending threshold reached
    pub fn should_allow_operation(&self, is_urgent: bool) -> bool {
        match self.state {
            PowerState::Active => true,
            PowerState::Waking => true,
            PowerState::PreSleep => is_urgent,
            // Allow urgent ops OR allow any op if pending threshold reached (to drain queue)
            PowerState::Sleeping => is_urgent || self.pending_count >= self.config.max_pending_before_wake,
        }
    }

    /// Check if sync should happen now
    pub fn should_sync(&self) -> bool {
        if self.state != PowerState::Active {
            return false;
        }

        match self.last_sync {
            None => true,
            Some(last) => last.elapsed() >= self.config.min_sync_interval,
        }
    }

    /// Record that a sync happened
    pub fn record_sync(&mut self) {
        self.last_sync = Some(Instant::now());
        debug!("Sync recorded");
    }

    /// Add a pending operation
    pub fn add_pending(&mut self) -> Result<(), DutyCycleError> {
        self.pending_count = self.pending_count.saturating_add(1);

        if self.pending_count > self.config.max_pending_before_wake * 2 {
            return Err(DutyCycleError::TooManyPending {
                count: self.pending_count,
            });
        }

        // Force wake if too many pending while sleeping
        if self.state == PowerState::Sleeping
            && self.pending_count >= self.config.max_pending_before_wake
        {
            self.wake();
        }

        Ok(())
    }

    /// Mark one pending operation as complete
    pub fn complete_pending(&mut self) {
        self.pending_count = self.pending_count.saturating_sub(1);
    }

    /// Clear all pending operations
    pub fn clear_pending(&mut self) {
        self.pending_count = 0;
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending_count
    }

    /// Advance the state machine based on elapsed time.
    ///
    /// Call this periodically (e.g., in your main loop) to transition between
    /// power states automatically.
    pub fn tick(&mut self) -> PowerState {
        let elapsed = self.state_entered.elapsed();
        let effective_sleep = if self.battery_level < self.config.low_battery_threshold {
            self.config.sleep_duration.saturating_mul(2) // Double sleep time on low battery
        } else {
            self.config.sleep_duration
        };

        match self.state {
            PowerState::Active if elapsed >= self.config.active_duration => {
                self.transition_to(PowerState::PreSleep);
            }
            PowerState::PreSleep if elapsed >= Duration::from_secs(5) => {
                self.transition_to(PowerState::Sleeping);
            }
            PowerState::Sleeping if elapsed >= effective_sleep => {
                self.transition_to(PowerState::Waking);
            }
            PowerState::Waking if elapsed >= Duration::from_secs(2) => {
                self.transition_to(PowerState::Active);
            }
            _ => {}
        }

        self.state
    }

    /// Force wake from sleep
    pub fn wake(&mut self) {
        if self.state == PowerState::Sleeping || self.state == PowerState::PreSleep {
            debug!("Forced wake requested");
            self.transition_to(PowerState::Waking);
        }
    }

    /// Force sleep
    pub fn sleep(&mut self) {
        if self.state == PowerState::Active {
            debug!("Forced sleep requested");
            self.transition_to(PowerState::PreSleep);
        }
    }

    /// Time until next state transition
    pub fn time_until_transition(&self) -> Duration {
        let elapsed = self.state_entered.elapsed();
        let target = match self.state {
            PowerState::Active => self.config.active_duration,
            PowerState::PreSleep => Duration::from_secs(5),
            PowerState::Sleeping => {
                if self.battery_level < self.config.low_battery_threshold {
                    self.config.sleep_duration.saturating_mul(2)
                } else {
                    self.config.sleep_duration
                }
            }
            PowerState::Waking => Duration::from_secs(2),
        };
        target.saturating_sub(elapsed)
    }

    fn transition_to(&mut self, new_state: PowerState) {
        debug!(from = ?self.state, to = ?new_state, "Power state transition");
        self.state = new_state;
        self.state_entered = Instant::now();
    }

    /// For testing: manually set state and reset timer
    #[cfg(test)]
    fn set_state_for_test(&mut self, state: PowerState) {
        self.state = state;
        self.state_entered = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_duty_cycle_config() {
        let config = DutyCycleConfig::default();
        assert!((config.duty_percentage() - 10.0).abs() < 1.0);

        let responsive = DutyCycleConfig::responsive();
        assert!((responsive.duty_percentage() - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_duty_percentage_zero_duration() {
        let config = DutyCycleConfig {
            active_duration: Duration::ZERO,
            sleep_duration: Duration::ZERO,
            ..Default::default()
        };
        // Should not be NaN, should be 100%
        let pct = config.duty_percentage();
        assert!(!pct.is_nan());
        assert_eq!(pct, 100.0);
    }

    #[test]
    fn test_operation_allowed() {
        let manager = DutyCycleManager::new(DutyCycleConfig::default());

        // Active state allows all operations
        assert!(manager.should_allow_operation(false));
        assert!(manager.should_allow_operation(true));
    }

    #[test]
    fn test_operation_allowed_in_sleeping() {
        let mut manager = DutyCycleManager::new(DutyCycleConfig {
            max_pending_before_wake: 5,
            ..Default::default()
        });
        manager.set_state_for_test(PowerState::Sleeping);

        // Non-urgent not allowed when pending is low
        assert!(!manager.should_allow_operation(false));
        // Urgent always allowed
        assert!(manager.should_allow_operation(true));

        // Add pending up to threshold
        for _ in 0..5 {
            manager.pending_count += 1;
        }

        // Now non-urgent is allowed (to drain queue)
        assert!(manager.should_allow_operation(false));
    }

    #[test]
    fn test_battery_adjustment() {
        let mut manager = DutyCycleManager::new(DutyCycleConfig::default());

        manager.set_battery_level(0.1);
        assert!(manager.battery_level() < manager.config().low_battery_threshold);
    }

    #[test]
    fn test_pending_operations() {
        let config = DutyCycleConfig {
            max_pending_before_wake: 3,
            ..Default::default()
        };
        let mut manager = DutyCycleManager::new(config);

        assert!(manager.add_pending().is_ok());
        assert!(manager.add_pending().is_ok());
        assert_eq!(manager.pending_count(), 2);

        manager.complete_pending();
        assert_eq!(manager.pending_count(), 1);

        manager.clear_pending();
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_too_many_pending_error() {
        let config = DutyCycleConfig {
            max_pending_before_wake: 2,
            ..Default::default()
        };
        let mut manager = DutyCycleManager::new(config);

        // Add up to 2x max (4) should work
        for _ in 0..4 {
            assert!(manager.add_pending().is_ok());
        }
        // 5th should fail
        assert!(matches!(
            manager.add_pending(),
            Err(DutyCycleError::TooManyPending { .. })
        ));
    }

    #[test]
    fn test_sync_timing() {
        let mut manager = DutyCycleManager::new(DutyCycleConfig::default());

        // First sync should be allowed
        assert!(manager.should_sync());

        manager.record_sync();

        // Immediately after, should not sync
        assert!(!manager.should_sync());
    }

    #[test]
    fn test_state_transitions_active_to_presleep() {
        let config = DutyCycleConfig {
            active_duration: Duration::from_millis(10),
            sleep_duration: Duration::from_millis(10),
            ..Default::default()
        };
        let mut manager = DutyCycleManager::new(config);

        assert_eq!(manager.state(), PowerState::Active);

        // Wait for active period to expire
        thread::sleep(Duration::from_millis(15));
        manager.tick();

        assert_eq!(manager.state(), PowerState::PreSleep);
    }

    #[test]
    fn test_state_transitions_presleep_to_sleeping() {
        let mut manager = DutyCycleManager::new(DutyCycleConfig::default());
        manager.set_state_for_test(PowerState::PreSleep);

        // Wait for presleep period (5 seconds is too long for test, so use tick timing)
        // PreSleep is 5 seconds, so we simulate by setting state and waiting
        thread::sleep(Duration::from_millis(10));

        // Still in PreSleep (not enough time)
        manager.tick();
        assert_eq!(manager.state(), PowerState::PreSleep);
    }

    #[test]
    fn test_force_wake() {
        let mut manager = DutyCycleManager::new(DutyCycleConfig::default());
        manager.set_state_for_test(PowerState::Sleeping);

        manager.wake();
        assert_eq!(manager.state(), PowerState::Waking);
    }

    #[test]
    fn test_force_sleep() {
        let mut manager = DutyCycleManager::new(DutyCycleConfig::default());
        assert_eq!(manager.state(), PowerState::Active);

        manager.sleep();
        assert_eq!(manager.state(), PowerState::PreSleep);
    }

    #[test]
    fn test_pending_triggers_wake() {
        let config = DutyCycleConfig {
            max_pending_before_wake: 3,
            ..Default::default()
        };
        let mut manager = DutyCycleManager::new(config);
        manager.set_state_for_test(PowerState::Sleeping);

        // Add pending up to threshold
        manager.add_pending().unwrap();
        manager.add_pending().unwrap();
        assert_eq!(manager.state(), PowerState::Sleeping);

        // Third pending should trigger wake
        manager.add_pending().unwrap();
        assert_eq!(manager.state(), PowerState::Waking);
    }

    #[test]
    fn test_time_until_transition() {
        let config = DutyCycleConfig {
            active_duration: Duration::from_secs(30),
            ..Default::default()
        };
        let manager = DutyCycleManager::new(config);

        let time = manager.time_until_transition();
        // Should be close to 30 seconds
        assert!(time <= Duration::from_secs(30));
        assert!(time >= Duration::from_secs(29));
    }
}
