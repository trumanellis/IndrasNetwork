//! Attention tracking for quests within realms.
//!
//! Members can focus their attention on one quest at a time to "charge it up".
//! The system tracks attention via an append-only log of switch events.
//! Quest rankings emerge from accumulated attention time.
//!
//! # Attention Model
//!
//! ```text
//! Member A: Quest1 -------|------ Quest2 --------|------ Quest1 --|
//!           [0ms]       [1000ms]              [3000ms]          [now]
//!
//! Quest1 attention = (1000-0) + (now-3000) = 1000 + elapsed
//! Quest2 attention = (3000-1000) = 2000
//! ```
//!
//! # CRDT Semantics
//!
//! - Events are append-only and immutable
//! - Merge strategy: union + sort by (timestamp, event_id)
//! - Concurrent events from same member: last-writer-wins by timestamp
//! - Derived state (current_focus) is rebuilt after merge

use indras_network::member::MemberId;
use crate::intention::IntentionId;

use indras_artifacts::attention::AttentionSwitchEvent as ChainedSwitchEvent;
use indras_artifacts::attention::fraud::{check_equivocation, EquivocationProof};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for an attention switch event.
pub type AttentionEventId = [u8; 16];

/// Generate a new unique attention event ID.
///
/// Uses timestamp + atomic counter for uniqueness, similar to IntentionId generation.
pub fn generate_attention_event_id() -> AttentionEventId {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut id = [0u8; 16];

    // Use timestamp for first 8 bytes
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    id[..8].copy_from_slice(&timestamp.to_le_bytes());

    // Use counter for remaining bytes (uniqueness within same nanosecond)
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    id[8..].copy_from_slice(&counter.to_le_bytes());

    id
}

/// An attention switch event in the append-only log.
///
/// Represents a member changing their focus to a specific quest (or clearing focus).
/// Events are immutable once created.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionSwitchEvent {
    /// Unique identifier for this event.
    pub event_id: AttentionEventId,
    /// The member who switched attention.
    pub member: MemberId,
    /// The quest they focused on (None = cleared attention).
    pub intention_id: Option<IntentionId>,
    /// When the switch occurred (Unix timestamp in milliseconds).
    pub timestamp_millis: i64,
    /// Logical clock for causal ordering. Monotonically increasing across
    /// all events a peer has seen. Updated on create and on merge.
    #[serde(default)]
    pub logical_clock: u64,
}

impl AttentionSwitchEvent {
    /// Create a new attention switch event.
    pub fn new(member: MemberId, intention_id: Option<IntentionId>) -> Self {
        Self {
            event_id: generate_attention_event_id(),
            member,
            intention_id,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            logical_clock: 0,
        }
    }

    /// Create an event to focus on a quest.
    pub fn focus(member: MemberId, intention_id: IntentionId) -> Self {
        Self::new(member, Some(intention_id))
    }

    /// Create an event to clear attention.
    pub fn clear(member: MemberId) -> Self {
        Self::new(member, None)
    }
}

impl PartialOrd for AttentionSwitchEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AttentionSwitchEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Primary: logical clock for causal ordering
        // Secondary: wall clock as tiebreaker
        // Tertiary: event_id for determinism
        self.logical_clock
            .cmp(&other.logical_clock)
            .then_with(|| self.timestamp_millis.cmp(&other.timestamp_millis))
            .then_with(|| self.event_id.cmp(&other.event_id))
    }
}

/// Computed attention value for a quest.
#[derive(Debug, Clone, Default)]
pub struct IntentionAttention {
    /// The quest this attention is for.
    pub intention_id: IntentionId,
    /// Total attention time in milliseconds.
    pub total_attention_millis: u64,
    /// Attention breakdown by member.
    pub attention_by_member: HashMap<MemberId, u64>,
    /// Members currently focusing on this quest.
    pub currently_focused_members: Vec<MemberId>,
}

/// CRDT document for attention tracking.
///
/// Maintains an append-only log of attention switch events and derived state
/// for quick lookups. The derived state is recomputable from events and must
/// be rebuilt after CRDT merges.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AttentionDocument {
    /// Append-only log of attention switch events.
    events: Vec<AttentionSwitchEvent>,
    /// Derived state: current focus per member (cached for efficiency).
    /// Key = member, Value = Some(intention_id) if focused, None if cleared.
    #[serde(default)]
    current_focus: HashMap<MemberId, Option<IntentionId>>,
    /// Hash-chained attention switch events (from indras-artifacts layer).
    /// Keyed by author (MemberId/PlayerId) for per-chain storage.
    #[serde(default)]
    chain_events: HashMap<MemberId, Vec<ChainedSwitchEvent>>,
    /// Local Lamport clock — tracks the maximum logical_clock seen.
    #[serde(default)]
    local_clock: u64,
}

impl AttentionDocument {
    /// Create a new empty attention document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an attention switch event.
    ///
    /// Returns the event ID of the recorded event.
    pub fn switch_attention(
        &mut self,
        member: MemberId,
        intention_id: Option<IntentionId>,
    ) -> AttentionEventId {
        self.local_clock += 1;
        let mut event = AttentionSwitchEvent::new(member, intention_id);
        event.logical_clock = self.local_clock;
        let event_id = event.event_id;

        // Update derived state
        self.current_focus.insert(member, intention_id);

        // Append event (maintains sorted order since new events have later timestamps)
        self.events.push(event);

        event_id
    }

    /// Focus a member on a specific quest.
    ///
    /// Returns the event ID of the recorded event.
    pub fn focus_on_intention(&mut self, member: MemberId, intention_id: IntentionId) -> AttentionEventId {
        self.switch_attention(member, Some(intention_id))
    }

    /// Clear a member's attention (stop focusing).
    ///
    /// Returns the event ID of the recorded event.
    pub fn clear_attention(&mut self, member: MemberId) -> AttentionEventId {
        self.switch_attention(member, None)
    }

    /// Get current focus for a member.
    ///
    /// Returns `Some(intention_id)` if focused, `None` if not focused or never focused.
    pub fn current_focus(&self, member: &MemberId) -> Option<IntentionId> {
        self.current_focus.get(member).copied().flatten()
    }

    /// Get all members currently focusing on a quest.
    pub fn members_focusing_on(&self, intention_id: &IntentionId) -> Vec<MemberId> {
        self.current_focus
            .iter()
            .filter_map(|(member, focus)| {
                if focus.as_ref() == Some(intention_id) {
                    Some(*member)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Insert a pre-formed event (for cross-realm mirroring).
    ///
    /// Skips if an event with the same ID already exists. This enables inserting
    /// the exact same event into multiple docs without generating new IDs,
    /// so CRDT dedup by `event_id` works correctly.
    pub fn insert_event(&mut self, event: AttentionSwitchEvent) {
        if self.events.iter().any(|e| e.event_id == event.event_id) {
            return;
        }
        // Warn (but don't reject) if this member already has a different focus
        // and the new event doesn't clear it first. This catches caller bugs
        // without breaking CRDT merge semantics.
        if let Some(current) = self.current_focus.get(&event.member) {
            if current.is_some() && event.intention_id.is_some() && *current != event.intention_id {
                tracing::warn!(
                    member = ?event.member,
                    current = ?current,
                    new = ?event.intention_id,
                    "insert_event: member switching focus without clearing first"
                );
            }
        }
        self.local_clock = self.local_clock.max(event.logical_clock) + 1;
        self.current_focus.insert(event.member, event.intention_id);
        self.events.push(event);
    }

    /// Get the most recently appended event, if any.
    pub fn last_event(&self) -> Option<&AttentionSwitchEvent> {
        self.events.last()
    }

    /// Get the number of attention events.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get all events (for inspection/debugging).
    pub fn events(&self) -> &[AttentionSwitchEvent] {
        &self.events
    }

    /// Handle a member leaving the realm.
    ///
    /// Automatically clears their attention if they were focused.
    pub fn handle_member_left(&mut self, member: MemberId) -> Option<AttentionEventId> {
        if self.current_focus(&member).is_some() {
            Some(self.clear_attention(member))
        } else {
            None
        }
    }

    /// Rebuild derived state from the event log.
    ///
    /// This must be called after CRDT merges to ensure consistency.
    pub fn rebuild_derived_state(&mut self) {
        self.current_focus.clear();

        // Sort events by timestamp for deterministic processing
        self.events.sort();

        // Replay events to rebuild current_focus
        for event in &self.events {
            self.current_focus.insert(event.member, event.intention_id);
        }
    }

    /// Merge another document into this one (CRDT merge).
    ///
    /// Uses union of events + sort for deterministic ordering.
    /// Automatically rebuilds derived state after merge.
    pub fn merge(&mut self, other: &AttentionDocument) {
        // Collect all events
        let mut all_events: Vec<AttentionSwitchEvent> = self.events.clone();

        // Add events from other that we don't have
        for event in &other.events {
            if !all_events.iter().any(|e| e.event_id == event.event_id) {
                all_events.push(event.clone());
            }
        }

        // Sort by timestamp then event_id for deterministic ordering
        all_events.sort();

        self.events = all_events;

        // Advance local Lamport clock past all merged events
        for event in &self.events {
            self.local_clock = self.local_clock.max(event.logical_clock);
        }

        self.rebuild_derived_state();

        // Merge chain events (union by event hash for each author)
        for (author, events) in &other.chain_events {
            let our_events = self.chain_events.entry(*author).or_default();
            for event in events {
                let dominated = our_events.iter().any(|e| e.seq == event.seq && e.event_hash() == event.event_hash());
                if !dominated {
                    our_events.push(event.clone());
                }
            }
            our_events.sort_by_key(|e| e.seq);
        }
    }

    /// Calculate attention for all quests up to a point in time.
    ///
    /// If `as_of` is None, uses current time.
    pub fn calculate_attention(&self, as_of: Option<i64>) -> Vec<IntentionAttention> {
        let as_of = as_of.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        // Track attention windows per member
        // member -> (intention_id, start_time)
        let mut member_windows: HashMap<MemberId, (IntentionId, i64)> = HashMap::new();

        // Accumulated attention per quest per member
        // intention_id -> member -> total_millis
        let mut intention_attention: HashMap<IntentionId, HashMap<MemberId, u64>> = HashMap::new();

        // Process events in order
        let mut sorted_events = self.events.clone();
        sorted_events.sort();

        for event in &sorted_events {
            // Skip events after as_of
            if event.timestamp_millis > as_of {
                continue;
            }

            // Close any open window for this member
            if let Some((prev_intention, start_time)) = member_windows.remove(&event.member) {
                let duration = (event.timestamp_millis - start_time).max(0) as u64;
                *intention_attention
                    .entry(prev_intention)
                    .or_default()
                    .entry(event.member)
                    .or_default() += duration;
            }

            // Open new window if focusing on a quest
            if let Some(intention_id) = event.intention_id {
                member_windows.insert(event.member, (intention_id, event.timestamp_millis));
            }
        }

        // Close open windows at as_of time
        for (member, (intention_id, start_time)) in &member_windows {
            let duration = (as_of - start_time).max(0) as u64;
            *intention_attention
                .entry(*intention_id)
                .or_default()
                .entry(*member)
                .or_default() += duration;
        }

        // Build result
        let mut result: Vec<IntentionAttention> = intention_attention
            .into_iter()
            .map(|(intention_id, by_member)| {
                let total_attention_millis: u64 = by_member.values().sum();
                let currently_focused_members: Vec<MemberId> = member_windows
                    .iter()
                    .filter_map(|(m, (q, _))| if q == &intention_id { Some(*m) } else { None })
                    .collect();

                IntentionAttention {
                    intention_id,
                    total_attention_millis,
                    attention_by_member: by_member,
                    currently_focused_members,
                }
            })
            .collect();

        // Sort by total attention (highest first), then by intention_id for stable ordering
        result.sort_by(|a, b| {
            b.total_attention_millis
                .cmp(&a.total_attention_millis)
                .then_with(|| a.intention_id.cmp(&b.intention_id))
        });

        result
    }

    /// Calculate attention for a specific quest.
    pub fn intention_attention(&self, intention_id: &IntentionId, as_of: Option<i64>) -> IntentionAttention {
        self.calculate_attention(as_of)
            .into_iter()
            .find(|qa| &qa.intention_id == intention_id)
            .unwrap_or_else(|| IntentionAttention {
                intention_id: *intention_id,
                ..Default::default()
            })
    }

    /// Get quests ranked by total attention (highest first).
    pub fn intentions_by_attention(&self, as_of: Option<i64>) -> Vec<IntentionAttention> {
        self.calculate_attention(as_of)
    }

    /// Compute raw attention milliseconds from token event indices.
    ///
    /// Each index in `event_indices` refers to a focus-start event in the
    /// sorted event log. The duration of each window is measured from that
    /// event's timestamp to the next event for the same member (or `as_of`
    /// if it's the most recent).
    ///
    /// This is the backing-asset computation for `TokenOfGratitude`:
    /// `event_indices` records which focus sessions the token represents,
    /// and this method calculates their total duration in milliseconds.
    pub fn compute_attention_millis(
        &self,
        event_indices: &[usize],
        as_of: Option<i64>,
    ) -> u64 {
        let as_of = as_of.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        let mut sorted_events = self.events.clone();
        sorted_events.sort();

        let mut total: u64 = 0;

        for &idx in event_indices {
            let Some(event) = sorted_events.get(idx) else {
                continue; // Out-of-bounds index — skip
            };

            // Find the next event for this member after this one
            let end_time = sorted_events[idx + 1..]
                .iter()
                .find(|e| e.member == event.member)
                .map(|e| e.timestamp_millis)
                .unwrap_or(as_of);

            let duration = (end_time - event.timestamp_millis).max(0) as u64;
            total += duration;
        }

        total
    }

    /// Store a chained (PQ-signed, hash-linked) attention event.
    pub fn store_chain_event(&mut self, event: ChainedSwitchEvent) {
        self.chain_events.entry(event.author).or_default().push(event);
    }

    /// Get all chained events for an author.
    pub fn chain_events_for(&self, author: &MemberId) -> &[ChainedSwitchEvent] {
        self.chain_events
            .get(author)
            .map_or(&[], |events| events.as_slice())
    }

    /// Get all chained events across all authors.
    pub fn all_chain_events(&self) -> &HashMap<MemberId, Vec<ChainedSwitchEvent>> {
        &self.chain_events
    }

    /// Check a new chained event for equivocation against existing events for the same author.
    ///
    /// Returns an `EquivocationProof` if the event conflicts with an existing one.
    pub fn check_chain_equivocation(
        &self,
        event: &ChainedSwitchEvent,
    ) -> Option<EquivocationProof> {
        let existing = self.chain_events_for(&event.author);
        check_equivocation(event, existing)
    }
}

/// Errors that can occur during attention operations.
#[derive(Debug, Clone, PartialEq)]
pub enum AttentionError {
    /// The intention was not found.
    IntentionNotFound,
    /// The member was not found.
    MemberNotFound,
}

impl std::fmt::Display for AttentionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttentionError::IntentionNotFound => write!(f, "Intention not found"),
            AttentionError::MemberNotFound => write!(f, "Member not found"),
        }
    }
}

impl std::error::Error for AttentionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member_id(n: u8) -> MemberId {
        [n; 32]
    }

    fn test_intention_id(n: u8) -> IntentionId {
        [n; 16]
    }

    #[test]
    fn test_attention_event_id_generation() {
        let id1 = generate_attention_event_id();
        let id2 = generate_attention_event_id();
        assert_ne!(id1, id2, "Event IDs should be unique");
    }

    #[test]
    fn test_attention_switch_event_creation() {
        let member = test_member_id(1);
        let intention = test_intention_id(1);

        let focus_event = AttentionSwitchEvent::focus(member, intention);
        assert_eq!(focus_event.member, member);
        assert_eq!(focus_event.intention_id, Some(intention));

        let clear_event = AttentionSwitchEvent::clear(member);
        assert_eq!(clear_event.member, member);
        assert_eq!(clear_event.intention_id, None);
    }

    #[test]
    fn test_single_member_attention() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let intention1 = test_intention_id(1);
        let intention2 = test_intention_id(2);

        // Initially no focus
        assert_eq!(doc.current_focus(&member), None);

        // Focus on intention1
        doc.focus_on_intention(member, intention1);
        assert_eq!(doc.current_focus(&member), Some(intention1));

        // Switch to intention2
        doc.focus_on_intention(member, intention2);
        assert_eq!(doc.current_focus(&member), Some(intention2));

        // Clear attention
        doc.clear_attention(member);
        assert_eq!(doc.current_focus(&member), None);

        // Should have 3 events
        assert_eq!(doc.event_count(), 3);
    }

    #[test]
    fn test_multiple_members_same_intention() {
        let mut doc = AttentionDocument::new();
        let member1 = test_member_id(1);
        let member2 = test_member_id(2);
        let member3 = test_member_id(3);
        let intention = test_intention_id(1);

        doc.focus_on_intention(member1, intention);
        doc.focus_on_intention(member2, intention);
        doc.focus_on_intention(member3, intention);

        let focusers = doc.members_focusing_on(&intention);
        assert_eq!(focusers.len(), 3);
        assert!(focusers.contains(&member1));
        assert!(focusers.contains(&member2));
        assert!(focusers.contains(&member3));
    }

    #[test]
    fn test_attention_calculation() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let intention1 = test_intention_id(1);
        let intention2 = test_intention_id(2);

        // Manually create events with specific timestamps for testing
        let t0 = 1000i64;
        let t1 = 2000i64;
        let t2 = 5000i64;

        doc.events.push(AttentionSwitchEvent {
            event_id: [1; 16],
            member,
            intention_id: Some(intention1),
            timestamp_millis: t0,
            logical_clock: 0,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [2; 16],
            member,
            intention_id: Some(intention2),
            timestamp_millis: t1,
            logical_clock: 0,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [3; 16],
            member,
            intention_id: None,
            timestamp_millis: t2,
            logical_clock: 0,
        });

        doc.rebuild_derived_state();

        // Calculate attention at t2
        let attention = doc.calculate_attention(Some(t2));

        // Quest1: t0 to t1 = 1000ms
        // Quest2: t1 to t2 = 3000ms
        let intention1_attention = attention.iter().find(|a| a.intention_id == intention1).unwrap();
        let intention2_attention = attention.iter().find(|a| a.intention_id == intention2).unwrap();

        assert_eq!(intention1_attention.total_attention_millis, 1000);
        assert_eq!(intention2_attention.total_attention_millis, 3000);

        // Quest2 should rank higher (more attention)
        assert_eq!(attention[0].intention_id, intention2);
        assert_eq!(attention[1].intention_id, intention1);
    }

    #[test]
    fn test_document_merge() {
        let mut doc1 = AttentionDocument::new();
        let mut doc2 = AttentionDocument::new();
        let member1 = test_member_id(1);
        let member2 = test_member_id(2);
        let intention = test_intention_id(1);

        doc1.focus_on_intention(member1, intention);
        doc2.focus_on_intention(member2, intention);

        // Merge doc2 into doc1
        doc1.merge(&doc2);

        // Should have both events
        assert_eq!(doc1.event_count(), 2);
        assert_eq!(doc1.current_focus(&member1), Some(intention));
        assert_eq!(doc1.current_focus(&member2), Some(intention));
    }

    #[test]
    fn test_handle_member_left() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let intention = test_intention_id(1);

        doc.focus_on_intention(member, intention);
        assert_eq!(doc.current_focus(&member), Some(intention));

        // Member leaves
        let event_id = doc.handle_member_left(member);
        assert!(event_id.is_some());
        assert_eq!(doc.current_focus(&member), None);

        // Leaving again should do nothing
        let event_id = doc.handle_member_left(member);
        assert!(event_id.is_none());
    }

    #[test]
    fn test_open_attention_window() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let intention = test_intention_id(1);

        // Focus on quest at t=1000
        doc.events.push(AttentionSwitchEvent {
            event_id: [1; 16],
            member,
            intention_id: Some(intention),
            timestamp_millis: 1000,
            logical_clock: 0,
        });
        doc.rebuild_derived_state();

        // Calculate attention at t=5000 (open window)
        let attention = doc.calculate_attention(Some(5000));
        let intention_attention = attention.iter().find(|a| a.intention_id == intention).unwrap();

        // Should have 4000ms of attention (5000 - 1000)
        assert_eq!(intention_attention.total_attention_millis, 4000);
        assert_eq!(intention_attention.currently_focused_members, vec![member]);
    }

    #[test]
    fn test_intentions_by_attention_ranking() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let intention1 = test_intention_id(1);
        let intention2 = test_intention_id(2);
        let quest3 = test_intention_id(3);

        // Quest1: 1000ms, Quest2: 3000ms, Quest3: 2000ms
        doc.events.push(AttentionSwitchEvent {
            event_id: [1; 16],
            member,
            intention_id: Some(intention1),
            timestamp_millis: 0,
            logical_clock: 0,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [2; 16],
            member,
            intention_id: Some(intention2),
            timestamp_millis: 1000,
            logical_clock: 0,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [3; 16],
            member,
            intention_id: Some(quest3),
            timestamp_millis: 4000,
            logical_clock: 0,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [4; 16],
            member,
            intention_id: None,
            timestamp_millis: 6000,
            logical_clock: 0,
        });

        doc.rebuild_derived_state();

        let ranked = doc.intentions_by_attention(Some(6000));

        // Should be ordered: Quest2 (3000), Quest3 (2000), Quest1 (1000)
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].intention_id, intention2);
        assert_eq!(ranked[0].total_attention_millis, 3000);
        assert_eq!(ranked[1].intention_id, quest3);
        assert_eq!(ranked[1].total_attention_millis, 2000);
        assert_eq!(ranked[2].intention_id, intention1);
        assert_eq!(ranked[2].total_attention_millis, 1000);
    }

    #[test]
    fn test_insert_event_dedup() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let intention = test_intention_id(1);

        // Create event once
        let event = AttentionSwitchEvent::focus(member, intention);
        let event_id = event.event_id;

        // Insert same event twice — should be idempotent
        doc.insert_event(event.clone());
        doc.insert_event(event.clone());
        assert_eq!(doc.event_count(), 1);
        assert_eq!(doc.current_focus(&member), Some(intention));

        // Insert into a second doc
        let mut doc2 = AttentionDocument::new();
        doc2.insert_event(event.clone());
        assert_eq!(doc2.event_count(), 1);

        // Merge — same event_id should deduplicate
        doc.merge(&doc2);
        assert_eq!(doc.event_count(), 1);
        assert_eq!(doc.events()[0].event_id, event_id);
    }

    #[test]
    fn test_insert_event_cross_doc_merge_no_double_count() {
        let member = test_member_id(1);
        let intention = test_intention_id(1);

        // Simulate: create event once, insert into home + DM realm
        let focus_event = AttentionSwitchEvent {
            event_id: [10; 16],
            member,
            intention_id: Some(intention),
            timestamp_millis: 1000,
            logical_clock: 0,
        };
        let clear_event = AttentionSwitchEvent {
            event_id: [11; 16],
            member,
            intention_id: None,
            timestamp_millis: 5000,
            logical_clock: 0,
        };

        let mut home_doc = AttentionDocument::new();
        home_doc.insert_event(focus_event.clone());
        home_doc.insert_event(clear_event.clone());

        let mut dm_doc = AttentionDocument::new();
        dm_doc.insert_event(focus_event.clone());
        dm_doc.insert_event(clear_event.clone());

        // Both docs should have identical events
        assert_eq!(home_doc.event_count(), 2);
        assert_eq!(dm_doc.event_count(), 2);

        // Merge — no duplicates
        home_doc.merge(&dm_doc);
        assert_eq!(home_doc.event_count(), 2);

        // Attention calculation should show 4000ms, NOT 8000ms (no double-counting)
        let attention = home_doc.calculate_attention(Some(5000));
        let qa = attention.iter().find(|a| a.intention_id == intention).unwrap();
        assert_eq!(qa.total_attention_millis, 4000);
    }

    #[test]
    fn test_insert_event_warns_on_switch_without_clear() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let intention1 = test_intention_id(1);
        let intention2 = test_intention_id(2);

        // Focus on intention1
        doc.insert_event(AttentionSwitchEvent::focus(member, intention1));
        assert_eq!(doc.current_focus(&member), Some(intention1));

        // Switch directly to intention2 without clearing first
        // (this should warn but still insert)
        doc.insert_event(AttentionSwitchEvent::focus(member, intention2));
        assert_eq!(doc.current_focus(&member), Some(intention2));
        assert_eq!(doc.event_count(), 2);
    }
}
