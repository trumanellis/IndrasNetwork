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

use crate::member::MemberId;
use crate::quest::QuestId;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for an attention switch event.
pub type AttentionEventId = [u8; 16];

/// Generate a new unique attention event ID.
///
/// Uses timestamp + atomic counter for uniqueness, similar to QuestId generation.
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
    pub quest_id: Option<QuestId>,
    /// When the switch occurred (Unix timestamp in milliseconds).
    pub timestamp_millis: i64,
}

impl AttentionSwitchEvent {
    /// Create a new attention switch event.
    pub fn new(member: MemberId, quest_id: Option<QuestId>) -> Self {
        Self {
            event_id: generate_attention_event_id(),
            member,
            quest_id,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Create an event to focus on a quest.
    pub fn focus(member: MemberId, quest_id: QuestId) -> Self {
        Self::new(member, Some(quest_id))
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
        // Sort by timestamp first, then by event_id for determinism
        self.timestamp_millis
            .cmp(&other.timestamp_millis)
            .then_with(|| self.event_id.cmp(&other.event_id))
    }
}

/// Computed attention value for a quest.
#[derive(Debug, Clone, Default)]
pub struct QuestAttention {
    /// The quest this attention is for.
    pub quest_id: QuestId,
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
    /// Key = member, Value = Some(quest_id) if focused, None if cleared.
    #[serde(default)]
    current_focus: HashMap<MemberId, Option<QuestId>>,
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
        quest_id: Option<QuestId>,
    ) -> AttentionEventId {
        let event = AttentionSwitchEvent::new(member, quest_id);
        let event_id = event.event_id;

        // Update derived state
        self.current_focus.insert(member, quest_id);

        // Append event (maintains sorted order since new events have later timestamps)
        self.events.push(event);

        event_id
    }

    /// Focus a member on a specific quest.
    ///
    /// Returns the event ID of the recorded event.
    pub fn focus_on_quest(&mut self, member: MemberId, quest_id: QuestId) -> AttentionEventId {
        self.switch_attention(member, Some(quest_id))
    }

    /// Clear a member's attention (stop focusing).
    ///
    /// Returns the event ID of the recorded event.
    pub fn clear_attention(&mut self, member: MemberId) -> AttentionEventId {
        self.switch_attention(member, None)
    }

    /// Get current focus for a member.
    ///
    /// Returns `Some(quest_id)` if focused, `None` if not focused or never focused.
    pub fn current_focus(&self, member: &MemberId) -> Option<QuestId> {
        self.current_focus.get(member).copied().flatten()
    }

    /// Get all members currently focusing on a quest.
    pub fn members_focusing_on(&self, quest_id: &QuestId) -> Vec<MemberId> {
        self.current_focus
            .iter()
            .filter_map(|(member, focus)| {
                if focus.as_ref() == Some(quest_id) {
                    Some(*member)
                } else {
                    None
                }
            })
            .collect()
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
            self.current_focus.insert(event.member, event.quest_id);
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
        self.rebuild_derived_state();
    }

    /// Calculate attention for all quests up to a point in time.
    ///
    /// If `as_of` is None, uses current time.
    pub fn calculate_attention(&self, as_of: Option<i64>) -> Vec<QuestAttention> {
        let as_of = as_of.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        // Track attention windows per member
        // member -> (quest_id, start_time)
        let mut member_windows: HashMap<MemberId, (QuestId, i64)> = HashMap::new();

        // Accumulated attention per quest per member
        // quest_id -> member -> total_millis
        let mut quest_attention: HashMap<QuestId, HashMap<MemberId, u64>> = HashMap::new();

        // Process events in order
        let mut sorted_events = self.events.clone();
        sorted_events.sort();

        for event in &sorted_events {
            // Skip events after as_of
            if event.timestamp_millis > as_of {
                continue;
            }

            // Close any open window for this member
            if let Some((prev_quest, start_time)) = member_windows.remove(&event.member) {
                let duration = (event.timestamp_millis - start_time).max(0) as u64;
                *quest_attention
                    .entry(prev_quest)
                    .or_default()
                    .entry(event.member)
                    .or_default() += duration;
            }

            // Open new window if focusing on a quest
            if let Some(quest_id) = event.quest_id {
                member_windows.insert(event.member, (quest_id, event.timestamp_millis));
            }
        }

        // Close open windows at as_of time
        for (member, (quest_id, start_time)) in &member_windows {
            let duration = (as_of - start_time).max(0) as u64;
            *quest_attention
                .entry(*quest_id)
                .or_default()
                .entry(*member)
                .or_default() += duration;
        }

        // Build result
        let mut result: Vec<QuestAttention> = quest_attention
            .into_iter()
            .map(|(quest_id, by_member)| {
                let total_attention_millis: u64 = by_member.values().sum();
                let currently_focused_members: Vec<MemberId> = member_windows
                    .iter()
                    .filter_map(|(m, (q, _))| if q == &quest_id { Some(*m) } else { None })
                    .collect();

                QuestAttention {
                    quest_id,
                    total_attention_millis,
                    attention_by_member: by_member,
                    currently_focused_members,
                }
            })
            .collect();

        // Sort by total attention (highest first), then by quest_id for stable ordering
        result.sort_by(|a, b| {
            b.total_attention_millis
                .cmp(&a.total_attention_millis)
                .then_with(|| a.quest_id.cmp(&b.quest_id))
        });

        result
    }

    /// Calculate attention for a specific quest.
    pub fn quest_attention(&self, quest_id: &QuestId, as_of: Option<i64>) -> QuestAttention {
        self.calculate_attention(as_of)
            .into_iter()
            .find(|qa| &qa.quest_id == quest_id)
            .unwrap_or_else(|| QuestAttention {
                quest_id: *quest_id,
                ..Default::default()
            })
    }

    /// Get quests ranked by total attention (highest first).
    pub fn quests_by_attention(&self, as_of: Option<i64>) -> Vec<QuestAttention> {
        self.calculate_attention(as_of)
    }
}

/// Errors that can occur during attention operations.
#[derive(Debug, Clone, PartialEq)]
pub enum AttentionError {
    /// The quest was not found.
    QuestNotFound,
    /// The member was not found.
    MemberNotFound,
}

impl std::fmt::Display for AttentionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttentionError::QuestNotFound => write!(f, "Quest not found"),
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

    fn test_quest_id(n: u8) -> QuestId {
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
        let quest = test_quest_id(1);

        let focus_event = AttentionSwitchEvent::focus(member, quest);
        assert_eq!(focus_event.member, member);
        assert_eq!(focus_event.quest_id, Some(quest));

        let clear_event = AttentionSwitchEvent::clear(member);
        assert_eq!(clear_event.member, member);
        assert_eq!(clear_event.quest_id, None);
    }

    #[test]
    fn test_single_member_attention() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let quest1 = test_quest_id(1);
        let quest2 = test_quest_id(2);

        // Initially no focus
        assert_eq!(doc.current_focus(&member), None);

        // Focus on quest1
        doc.focus_on_quest(member, quest1);
        assert_eq!(doc.current_focus(&member), Some(quest1));

        // Switch to quest2
        doc.focus_on_quest(member, quest2);
        assert_eq!(doc.current_focus(&member), Some(quest2));

        // Clear attention
        doc.clear_attention(member);
        assert_eq!(doc.current_focus(&member), None);

        // Should have 3 events
        assert_eq!(doc.event_count(), 3);
    }

    #[test]
    fn test_multiple_members_same_quest() {
        let mut doc = AttentionDocument::new();
        let member1 = test_member_id(1);
        let member2 = test_member_id(2);
        let member3 = test_member_id(3);
        let quest = test_quest_id(1);

        doc.focus_on_quest(member1, quest);
        doc.focus_on_quest(member2, quest);
        doc.focus_on_quest(member3, quest);

        let focusers = doc.members_focusing_on(&quest);
        assert_eq!(focusers.len(), 3);
        assert!(focusers.contains(&member1));
        assert!(focusers.contains(&member2));
        assert!(focusers.contains(&member3));
    }

    #[test]
    fn test_attention_calculation() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let quest1 = test_quest_id(1);
        let quest2 = test_quest_id(2);

        // Manually create events with specific timestamps for testing
        let t0 = 1000i64;
        let t1 = 2000i64;
        let t2 = 5000i64;

        doc.events.push(AttentionSwitchEvent {
            event_id: [1; 16],
            member,
            quest_id: Some(quest1),
            timestamp_millis: t0,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [2; 16],
            member,
            quest_id: Some(quest2),
            timestamp_millis: t1,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [3; 16],
            member,
            quest_id: None,
            timestamp_millis: t2,
        });

        doc.rebuild_derived_state();

        // Calculate attention at t2
        let attention = doc.calculate_attention(Some(t2));

        // Quest1: t0 to t1 = 1000ms
        // Quest2: t1 to t2 = 3000ms
        let quest1_attention = attention.iter().find(|a| a.quest_id == quest1).unwrap();
        let quest2_attention = attention.iter().find(|a| a.quest_id == quest2).unwrap();

        assert_eq!(quest1_attention.total_attention_millis, 1000);
        assert_eq!(quest2_attention.total_attention_millis, 3000);

        // Quest2 should rank higher (more attention)
        assert_eq!(attention[0].quest_id, quest2);
        assert_eq!(attention[1].quest_id, quest1);
    }

    #[test]
    fn test_document_merge() {
        let mut doc1 = AttentionDocument::new();
        let mut doc2 = AttentionDocument::new();
        let member1 = test_member_id(1);
        let member2 = test_member_id(2);
        let quest = test_quest_id(1);

        doc1.focus_on_quest(member1, quest);
        doc2.focus_on_quest(member2, quest);

        // Merge doc2 into doc1
        doc1.merge(&doc2);

        // Should have both events
        assert_eq!(doc1.event_count(), 2);
        assert_eq!(doc1.current_focus(&member1), Some(quest));
        assert_eq!(doc1.current_focus(&member2), Some(quest));
    }

    #[test]
    fn test_handle_member_left() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let quest = test_quest_id(1);

        doc.focus_on_quest(member, quest);
        assert_eq!(doc.current_focus(&member), Some(quest));

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
        let quest = test_quest_id(1);

        // Focus on quest at t=1000
        doc.events.push(AttentionSwitchEvent {
            event_id: [1; 16],
            member,
            quest_id: Some(quest),
            timestamp_millis: 1000,
        });
        doc.rebuild_derived_state();

        // Calculate attention at t=5000 (open window)
        let attention = doc.calculate_attention(Some(5000));
        let quest_attention = attention.iter().find(|a| a.quest_id == quest).unwrap();

        // Should have 4000ms of attention (5000 - 1000)
        assert_eq!(quest_attention.total_attention_millis, 4000);
        assert_eq!(quest_attention.currently_focused_members, vec![member]);
    }

    #[test]
    fn test_quests_by_attention_ranking() {
        let mut doc = AttentionDocument::new();
        let member = test_member_id(1);
        let quest1 = test_quest_id(1);
        let quest2 = test_quest_id(2);
        let quest3 = test_quest_id(3);

        // Quest1: 1000ms, Quest2: 3000ms, Quest3: 2000ms
        doc.events.push(AttentionSwitchEvent {
            event_id: [1; 16],
            member,
            quest_id: Some(quest1),
            timestamp_millis: 0,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [2; 16],
            member,
            quest_id: Some(quest2),
            timestamp_millis: 1000,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [3; 16],
            member,
            quest_id: Some(quest3),
            timestamp_millis: 4000,
        });
        doc.events.push(AttentionSwitchEvent {
            event_id: [4; 16],
            member,
            quest_id: None,
            timestamp_millis: 6000,
        });

        doc.rebuild_derived_state();

        let ranked = doc.quests_by_attention(Some(6000));

        // Should be ordered: Quest2 (3000), Quest3 (2000), Quest1 (1000)
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].quest_id, quest2);
        assert_eq!(ranked[0].total_attention_millis, 3000);
        assert_eq!(ranked[1].quest_id, quest3);
        assert_eq!(ranked[1].total_attention_millis, 2000);
        assert_eq!(ranked[2].quest_id, quest1);
        assert_eq!(ranked[2].total_attention_millis, 1000);
    }
}
