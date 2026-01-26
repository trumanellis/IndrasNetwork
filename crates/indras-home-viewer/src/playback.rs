//! Playback control for home realm event streaming.
//!
//! Uses atomic primitives for lock-free communication between
//! the UI thread and the async stream reader.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Whether playback is currently paused.
static PLAYBACK_PAUSED: AtomicBool = AtomicBool::new(true);

/// Playback speed multiplied by 10 (e.g., 10 = 1.0x, 20 = 2.0x).
static PLAYBACK_SPEED_X10: AtomicU32 = AtomicU32::new(10);

/// Whether a reset has been requested.
static RESET_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether a single step has been requested.
static STEP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether shutdown has been requested.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Returns whether playback is currently paused.
pub fn is_paused() -> bool {
    PLAYBACK_PAUSED.load(Ordering::Relaxed)
}

/// Sets the paused state.
pub fn set_paused(paused: bool) {
    PLAYBACK_PAUSED.store(paused, Ordering::Relaxed);
}

/// Toggles the paused state and returns the new value.
pub fn toggle_paused() -> bool {
    let new_value = !is_paused();
    set_paused(new_value);
    new_value
}

/// Returns the current playback speed as a float.
pub fn get_speed() -> f32 {
    PLAYBACK_SPEED_X10.load(Ordering::Relaxed) as f32 / 10.0
}

/// Sets the playback speed.
pub fn set_speed(speed: f32) {
    let speed_x10 = (speed * 10.0) as u32;
    PLAYBACK_SPEED_X10.store(speed_x10, Ordering::Relaxed);
}

/// Returns the delay between events in milliseconds based on current speed.
pub fn get_delay_ms() -> u64 {
    let speed = get_speed();
    if speed <= 0.0 {
        100
    } else {
        (100.0 / speed) as u64
    }
}

/// Requests a reset to the beginning.
pub fn request_reset() {
    RESET_REQUESTED.store(true, Ordering::Relaxed);
}

/// Takes and clears the reset request.
pub fn take_reset_request() -> bool {
    RESET_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Requests a single step forward.
pub fn request_step() {
    STEP_REQUESTED.store(true, Ordering::Relaxed);
}

/// Takes and clears the step request.
pub fn take_step_request() -> bool {
    STEP_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Requests shutdown.
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
}

/// Returns whether shutdown has been requested.
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Relaxed)
}
