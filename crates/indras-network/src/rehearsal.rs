//! Spaced repetition for story drift mitigation.
//!
//! Tracks rehearsal schedule and enforces periodic story retelling
//! to prevent drift in the user's memory of their pass story.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Spaced repetition state for a user's pass story.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RehearsalState {
    /// When the story was first created.
    pub created_at: DateTime<Utc>,
    /// When the last rehearsal occurred.
    pub last_rehearsal: Option<DateTime<Utc>>,
    /// Total number of rehearsals completed.
    pub rehearsal_count: u32,
    /// When the next rehearsal is due.
    pub next_rehearsal: DateTime<Utc>,
    /// Consecutive successful rehearsals.
    pub consecutive_successes: u32,
}

impl RehearsalState {
    /// Create a new rehearsal state. First rehearsal is due in 1 day.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            last_rehearsal: None,
            rehearsal_count: 0,
            next_rehearsal: now + Duration::days(1),
            consecutive_successes: 0,
        }
    }

    /// Record a successful rehearsal and schedule the next one.
    ///
    /// Schedule: day 1, 3, 7, then monthly.
    pub fn record_success(&mut self) {
        let now = Utc::now();
        self.last_rehearsal = Some(now);
        self.rehearsal_count += 1;
        self.consecutive_successes += 1;

        // Schedule next rehearsal based on count
        self.next_rehearsal = match self.rehearsal_count {
            1 => now + Duration::days(2),     // Next at day 3
            2 => now + Duration::days(4),     // Next at day 7
            _ => now + Duration::days(30),    // Monthly thereafter
        };
    }

    /// Record a failed rehearsal. Resets to more frequent schedule.
    pub fn record_failure(&mut self) {
        let now = Utc::now();
        self.last_rehearsal = Some(now);
        self.consecutive_successes = 0;

        // After failure, retry in 1 day
        self.next_rehearsal = now + Duration::days(1);
    }

    /// Check if a rehearsal is currently due.
    pub fn is_due(&self) -> bool {
        Utc::now() >= self.next_rehearsal
    }

    /// Get the next rehearsal due date.
    pub fn next_due(&self) -> DateTime<Utc> {
        self.next_rehearsal
    }

    /// Get the number of days since story creation.
    pub fn days_since_creation(&self) -> i64 {
        (Utc::now() - self.created_at).num_days()
    }

    /// Serialize to JSON bytes for storage.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize from JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl Default for RehearsalState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rehearsal_state() {
        let state = RehearsalState::new();
        assert_eq!(state.rehearsal_count, 0);
        assert_eq!(state.consecutive_successes, 0);
        assert!(state.last_rehearsal.is_none());
    }

    #[test]
    fn test_is_due_initially() {
        // New state: rehearsal is due in 1 day, not immediately
        let state = RehearsalState::new();
        assert!(!state.is_due());
    }

    #[test]
    fn test_record_success_increments() {
        let mut state = RehearsalState::new();
        state.record_success();
        assert_eq!(state.rehearsal_count, 1);
        assert_eq!(state.consecutive_successes, 1);
        assert!(state.last_rehearsal.is_some());
    }

    #[test]
    fn test_record_failure_resets_streak() {
        let mut state = RehearsalState::new();
        state.record_success();
        state.record_success();
        assert_eq!(state.consecutive_successes, 2);

        state.record_failure();
        assert_eq!(state.consecutive_successes, 0);
        assert_eq!(state.rehearsal_count, 2); // Count doesn't reset
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut state = RehearsalState::new();
        state.record_success();

        let bytes = state.to_bytes().unwrap();
        let restored = RehearsalState::from_bytes(&bytes).unwrap();

        assert_eq!(restored.rehearsal_count, state.rehearsal_count);
        assert_eq!(restored.consecutive_successes, state.consecutive_successes);
    }

    #[test]
    fn test_schedule_progression() {
        let mut state = RehearsalState::new();

        // After 1st success: next in 2 days (day 3)
        state.record_success();
        let gap1 = (state.next_rehearsal - state.last_rehearsal.unwrap()).num_days();
        assert_eq!(gap1, 2);

        // After 2nd success: next in 4 days (day 7)
        state.record_success();
        let gap2 = (state.next_rehearsal - state.last_rehearsal.unwrap()).num_days();
        assert_eq!(gap2, 4);

        // After 3rd success: next in 30 days (monthly)
        state.record_success();
        let gap3 = (state.next_rehearsal - state.last_rehearsal.unwrap()).num_days();
        assert_eq!(gap3, 30);
    }
}
