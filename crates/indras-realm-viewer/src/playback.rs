//! Playback control for event stream processing
//!
//! Provides global atomic state for controlling playback speed and pause.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// Global playback paused state (starts paused by default)
static PLAYBACK_PAUSED: AtomicBool = AtomicBool::new(true);

/// Playback speed as fixed-point (multiply by 10, so 10 = 1.0x, 20 = 2.0x, etc.)
/// Default ~0.33x (3x slower than 1.0x) for calm observation
static PLAYBACK_SPEED_X10: AtomicU32 = AtomicU32::new(3);

/// Reset requested flag - stream processor should replay from buffer
static RESET_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Step requested flag - advance one event while paused
static STEP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Shutdown requested flag - signals async tasks to exit gracefully
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Current position in the event buffer (written by async loop, read by UI)
static CURRENT_POS: AtomicUsize = AtomicUsize::new(0);

/// Total events in the buffer (written by async loop, read by UI)
static BUFFER_LEN: AtomicUsize = AtomicUsize::new(0);

/// Seek target requested by UI (usize::MAX = no seek pending)
static SEEK_TARGET: AtomicUsize = AtomicUsize::new(usize::MAX);

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

/// Reset playback state to defaults (paused at ~0.33x speed)
pub fn reset() {
    PLAYBACK_PAUSED.store(true, Ordering::Relaxed);
    PLAYBACK_SPEED_X10.store(3, Ordering::Relaxed);
    CURRENT_POS.store(0, Ordering::Relaxed);
    SEEK_TARGET.store(usize::MAX, Ordering::Relaxed);
}

/// Get the current playback position in the event buffer
pub fn get_current_pos() -> usize {
    CURRENT_POS.load(Ordering::Relaxed)
}

/// Set the current playback position
pub fn set_current_pos(pos: usize) {
    CURRENT_POS.store(pos, Ordering::Relaxed);
}

/// Get the total number of events in the buffer
pub fn get_buffer_len() -> usize {
    BUFFER_LEN.load(Ordering::Relaxed)
}

/// Set the total number of events in the buffer
pub fn set_buffer_len(len: usize) {
    BUFFER_LEN.store(len, Ordering::Relaxed);
}

/// Request a seek to the given position (UI writes this)
pub fn request_seek(target: usize) {
    SEEK_TARGET.store(target, Ordering::Relaxed);
}

/// Take a pending seek request, returning Some(target) if one was set
pub fn take_seek_request() -> Option<usize> {
    let val = SEEK_TARGET.swap(usize::MAX, Ordering::Relaxed);
    if val == usize::MAX { None } else { Some(val) }
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
