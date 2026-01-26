//! Playback control for event stream processing
//!
//! Provides global atomic state for controlling playback speed and pause.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Global playback paused state (starts paused by default)
static PLAYBACK_PAUSED: AtomicBool = AtomicBool::new(true);

/// Playback speed as fixed-point (multiply by 10, so 10 = 1.0x, 20 = 2.0x, etc.)
static PLAYBACK_SPEED_X10: AtomicU32 = AtomicU32::new(10);

/// Reset requested flag - stream processor should replay from buffer
static RESET_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Step requested flag - advance one event while paused
static STEP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Shutdown requested flag - signals async tasks to exit gracefully
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Check if playback is paused
pub fn is_paused() -> bool {
    PLAYBACK_PAUSED.load(Ordering::Relaxed)
}

/// Set playback paused state
pub fn set_paused(paused: bool) {
    PLAYBACK_PAUSED.store(paused, Ordering::Relaxed);
}

/// Get playback speed multiplier
pub fn get_speed() -> f32 {
    PLAYBACK_SPEED_X10.load(Ordering::Relaxed) as f32 / 10.0
}

/// Set playback speed multiplier
pub fn set_speed(speed: f32) {
    let speed_x10 = (speed * 10.0) as u32;
    PLAYBACK_SPEED_X10.store(speed_x10, Ordering::Relaxed);
}

/// Get the delay in milliseconds for the current speed setting
/// Base delay is 500ms at 1.0x speed
pub fn get_delay_ms() -> u64 {
    let speed_x10 = PLAYBACK_SPEED_X10.load(Ordering::Relaxed).max(5); // min 0.5x
    (500 * 10) / speed_x10 as u64
}

/// Reset playback state to defaults (paused at 1.0x speed)
pub fn reset() {
    PLAYBACK_PAUSED.store(true, Ordering::Relaxed);
    PLAYBACK_SPEED_X10.store(10, Ordering::Relaxed);
}

/// Request a reset - signals stream processor to replay from buffer
pub fn request_reset() {
    RESET_REQUESTED.store(true, Ordering::Relaxed);
}

/// Check if reset was requested (and clear the flag)
pub fn take_reset_request() -> bool {
    RESET_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Request a single step - advance one event while paused
pub fn request_step() {
    STEP_REQUESTED.store(true, Ordering::Relaxed);
}

/// Check if step was requested (and clear the flag)
pub fn take_step_request() -> bool {
    STEP_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Request shutdown - signals async tasks to exit gracefully
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
}

/// Check if shutdown was requested
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Relaxed)
}
