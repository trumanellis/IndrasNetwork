//! Shared playback control for Indras viewer event stream processing.
//!
//! Provides global atomic state for controlling playback speed, pause, step,
//! reset, seek, and shutdown — shared across all viewer crates that use the
//! two-phase ingest → replay streaming model.
//!
//! # Usage
//!
//! ```rust,ignore
//! use indras_viewer_common::playback;
//!
//! playback::set_speed(1.0);
//! playback::set_paused(false);
//! ```

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// Global playback paused state (starts paused by default).
static PLAYBACK_PAUSED: AtomicBool = AtomicBool::new(true);

/// Playback speed as fixed-point (multiplied by 10, so 10 = 1.0x, 20 = 2.0x).
/// Default is 3 (~0.33x) for calm observation.
static PLAYBACK_SPEED_X10: AtomicU32 = AtomicU32::new(3);

/// Reset requested flag — stream processor should replay from buffer.
static RESET_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Step requested flag — advance one event while paused.
static STEP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Shutdown requested flag — signals async tasks to exit gracefully.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Current position in the event buffer (written by async loop, read by UI).
static CURRENT_POS: AtomicUsize = AtomicUsize::new(0);

/// Total events in the buffer (written by async loop, read by UI).
static BUFFER_LEN: AtomicUsize = AtomicUsize::new(0);

/// Seek target requested by UI (`usize::MAX` = no seek pending).
static SEEK_TARGET: AtomicUsize = AtomicUsize::new(usize::MAX);

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

/// Returns the current playback speed multiplier.
pub fn get_speed() -> f32 {
    PLAYBACK_SPEED_X10.load(Ordering::Relaxed) as f32 / 10.0
}

/// Sets the playback speed multiplier.
pub fn set_speed(speed: f32) {
    let speed_x10 = (speed * 10.0) as u32;
    PLAYBACK_SPEED_X10.store(speed_x10, Ordering::Relaxed);
}

/// Returns the delay in milliseconds between events at the current speed.
///
/// Base delay is 500ms at 1.0x speed; minimum effective speed is 0.5x.
pub fn get_delay_ms() -> u64 {
    let speed_x10 = PLAYBACK_SPEED_X10.load(Ordering::Relaxed).max(5);
    (500 * 10) / speed_x10 as u64
}

/// Resets playback state to defaults (paused at ~0.33x speed, position 0).
pub fn reset() {
    PLAYBACK_PAUSED.store(true, Ordering::Relaxed);
    PLAYBACK_SPEED_X10.store(3, Ordering::Relaxed);
    CURRENT_POS.store(0, Ordering::Relaxed);
    SEEK_TARGET.store(usize::MAX, Ordering::Relaxed);
}

/// Returns the current playback position in the event buffer.
pub fn get_current_pos() -> usize {
    CURRENT_POS.load(Ordering::Relaxed)
}

/// Sets the current playback position.
pub fn set_current_pos(pos: usize) {
    CURRENT_POS.store(pos, Ordering::Relaxed);
}

/// Returns the total number of events in the buffer.
pub fn get_buffer_len() -> usize {
    BUFFER_LEN.load(Ordering::Relaxed)
}

/// Sets the total number of events in the buffer.
pub fn set_buffer_len(len: usize) {
    BUFFER_LEN.store(len, Ordering::Relaxed);
}

/// Requests a seek to the given buffer position (written by UI).
pub fn request_seek(target: usize) {
    SEEK_TARGET.store(target, Ordering::Relaxed);
}

/// Takes a pending seek request, returning `Some(target)` if one was set.
pub fn take_seek_request() -> Option<usize> {
    let val = SEEK_TARGET.swap(usize::MAX, Ordering::Relaxed);
    if val == usize::MAX {
        None
    } else {
        Some(val)
    }
}

/// Requests a reset — signals the stream processor to replay from the buffer.
pub fn request_reset() {
    RESET_REQUESTED.store(true, Ordering::Relaxed);
}

/// Takes and clears the reset request flag.
pub fn take_reset_request() -> bool {
    RESET_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Requests a single step — advances one event while paused.
pub fn request_step() {
    STEP_REQUESTED.store(true, Ordering::Relaxed);
}

/// Takes and clears the step request flag.
pub fn take_step_request() -> bool {
    STEP_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Requests shutdown — signals async tasks to exit gracefully.
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
}

/// Returns whether shutdown has been requested.
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Relaxed)
}
