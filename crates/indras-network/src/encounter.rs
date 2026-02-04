//! Encounter codes — 6-digit spoken-aloud codes for in-person connection.
//!
//! When two people are near each other (or on a phone call), they can
//! exchange a short 6-digit code to discover each other's MemberIds
//! without sharing any digital content.
//!
//! # How It Works
//!
//! 1. Zephyr calls `create_encounter()` → gets code "743901"
//! 2. Zephyr tells Nova: "seven four three nine zero one"
//! 3. Nova calls `join_encounter("743901")` → discovers Zephyr's MemberId
//! 4. Both auto-call `connect()` (Layer 1) with discovered MemberId
//! 5. Both leave encounter topic after 60 seconds
//!
//! # Time Windows
//!
//! Codes are scoped to 60-second time windows to prevent replay.
//! When joining, both the current and previous windows are tried
//! to handle clock skew.

use crate::error::{IndraError, Result};
use crate::member::MemberId;
use crate::network::RealmId;

use indras_core::InterfaceId;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Length of encounter codes (6 digits).
const ENCOUNTER_CODE_LENGTH: usize = 6;

/// Duration of a time window in seconds.
const TIME_WINDOW_SECS: u64 = 60;

/// Generate a random 6-digit encounter code.
pub fn generate_encounter_code() -> String {
    let n: u32 = rand::random::<u32>() % 1_000_000;
    format!("{:06}", n)
}

/// Compute the current time window index.
fn current_time_window() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / TIME_WINDOW_SECS
}

/// Derive a gossip topic ID from an encounter code and time window.
///
/// The topic is deterministic given the same code and window,
/// so both parties arrive at the same gossip topic.
pub fn encounter_topic(code: &str, time_window: u64) -> RealmId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"encounter-v1:");
    hasher.update(code.as_bytes());
    hasher.update(b":");
    hasher.update(&time_window.to_le_bytes());
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Get the encounter topics for the current and previous time windows.
///
/// Trying both handles clock skew between the two devices.
pub fn encounter_topics(code: &str) -> Vec<RealmId> {
    let window = current_time_window();
    vec![
        encounter_topic(code, window),
        encounter_topic(code, window.saturating_sub(1)),
    ]
}

/// Validate an encounter code format.
pub fn validate_encounter_code(code: &str) -> Result<()> {
    let code = code.trim();
    if code.len() != ENCOUNTER_CODE_LENGTH {
        return Err(IndraError::InvalidOperation(format!(
            "Encounter code must be {} digits, got {}",
            ENCOUNTER_CODE_LENGTH,
            code.len()
        )));
    }
    if !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(IndraError::InvalidOperation(
            "Encounter code must contain only digits".to_string(),
        ));
    }
    Ok(())
}

/// Message exchanged on the encounter gossip topic.
///
/// Contains just the MemberId and optional display name.
/// After exchange, both sides call `connect()` with the discovered ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterExchangePayload {
    /// The member's ID (32-byte public key).
    pub member_id: MemberId,
    /// Optional display name.
    pub display_name: Option<String>,
    /// Timestamp (millis since epoch) for freshness checking.
    pub timestamp_millis: u64,
}

impl EncounterExchangePayload {
    /// Create a new encounter exchange payload.
    pub fn new(member_id: MemberId, display_name: Option<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            member_id,
            display_name,
            timestamp_millis: now,
        }
    }

    /// Check if this payload is reasonably fresh (within 5 minutes).
    pub fn is_fresh(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(self.timestamp_millis) < 5 * 60 * 1000
    }
}

/// Handle for an active encounter (creator side).
///
/// Holds the code and topic IDs so the creator can clean up
/// after the encounter completes.
#[derive(Debug, Clone)]
pub struct EncounterHandle {
    /// The 6-digit encounter code.
    pub code: String,
    /// The gossip topics we joined (current + previous window).
    pub topics: Vec<RealmId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_format() {
        for _ in 0..100 {
            let code = generate_encounter_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn test_encounter_topic_deterministic() {
        let t1 = encounter_topic("123456", 100);
        let t2 = encounter_topic("123456", 100);
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_encounter_topic_unique_by_code() {
        let t1 = encounter_topic("123456", 100);
        let t2 = encounter_topic("654321", 100);
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_encounter_topic_unique_by_window() {
        let t1 = encounter_topic("123456", 100);
        let t2 = encounter_topic("123456", 101);
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_encounter_topics_returns_two() {
        let topics = encounter_topics("123456");
        assert_eq!(topics.len(), 2);
        // Current and previous window should be different
        assert_ne!(topics[0], topics[1]);
    }

    #[test]
    fn test_validate_code_valid() {
        assert!(validate_encounter_code("123456").is_ok());
        assert!(validate_encounter_code("000000").is_ok());
        assert!(validate_encounter_code("999999").is_ok());
    }

    #[test]
    fn test_validate_code_wrong_length() {
        assert!(validate_encounter_code("12345").is_err());
        assert!(validate_encounter_code("1234567").is_err());
        assert!(validate_encounter_code("").is_err());
    }

    #[test]
    fn test_validate_code_non_digit() {
        assert!(validate_encounter_code("12345a").is_err());
        assert!(validate_encounter_code("abcdef").is_err());
        assert!(validate_encounter_code("12 456").is_err());
    }

    #[test]
    fn test_validate_code_whitespace_trimmed() {
        assert!(validate_encounter_code("  123456  ").is_ok());
    }

    #[test]
    fn test_encounter_exchange_payload() {
        let payload = EncounterExchangePayload::new([42u8; 32], Some("Zephyr".to_string()));
        assert_eq!(payload.member_id, [42u8; 32]);
        assert_eq!(payload.display_name, Some("Zephyr".to_string()));
        assert!(payload.is_fresh());
    }

    #[test]
    fn test_encounter_exchange_serialization() {
        let payload = EncounterExchangePayload::new([7u8; 32], None);
        let bytes = postcard::to_allocvec(&payload).unwrap();
        let deserialized: EncounterExchangePayload = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(deserialized.member_id, [7u8; 32]);
        assert_eq!(deserialized.display_name, None);
    }

    #[test]
    fn test_encounter_exchange_stale() {
        let mut payload = EncounterExchangePayload::new([1u8; 32], None);
        // Set to 10 minutes ago
        let ten_min_ms = 10 * 60 * 1000u64;
        payload.timestamp_millis = payload.timestamp_millis.saturating_sub(ten_min_ms);
        assert!(!payload.is_fresh());
    }
}
