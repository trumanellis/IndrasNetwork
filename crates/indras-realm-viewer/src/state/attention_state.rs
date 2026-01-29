//! Attention tracking state
//!
//! Tracks member focus on quests and calculates attention rankings.

use std::collections::HashMap;

use crate::events::StreamEvent;

/// An attention switch event
#[derive(Clone, Debug)]
pub struct AttentionEvent {
    pub tick: u32,
    pub member: String,
    pub quest_id: Option<String>,
    pub timestamp_ms: i64,
}

/// Calculated attention for a quest
#[derive(Clone, Debug, PartialEq)]
pub struct QuestAttention {
    pub quest_id: String,
    pub total_attention_ms: u64,
    pub by_member: HashMap<String, u64>,
    pub currently_focusing: Vec<String>,
}

/// Attention tracking state
#[derive(Clone, Debug, Default)]
pub struct AttentionState {
    /// All attention events (append-only)
    pub events: Vec<AttentionEvent>,
    /// Current focus per member
    pub current_focus: HashMap<String, Option<String>>,
    /// Focus start time per member (for calculating duration)
    pub focus_start: HashMap<String, (String, u32)>,
    /// Accumulated attention per quest per member
    pub attention: HashMap<String, HashMap<String, u64>>,
    /// Current tick for calculations
    pub current_tick: u32,
}

impl AttentionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process an attention-related event
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::AttentionSwitched {
                tick,
                member,
                quest_id,
                ..
            } => {
                self.current_tick = *tick;

                // Close previous focus window
                if let Some((prev_quest, start_tick)) = self.focus_start.remove(member) {
                    let duration = tick.saturating_sub(start_tick) as u64 * 100; // Convert ticks to ms
                    *self
                        .attention
                        .entry(prev_quest)
                        .or_default()
                        .entry(member.clone())
                        .or_default() += duration;
                }

                // Open new focus window
                self.focus_start
                    .insert(member.clone(), (quest_id.clone(), *tick));
                self.current_focus
                    .insert(member.clone(), Some(quest_id.clone()));

                // Record event
                self.events.push(AttentionEvent {
                    tick: *tick,
                    member: member.clone(),
                    quest_id: Some(quest_id.clone()),
                    timestamp_ms: (*tick as i64) * 100,
                });
            }

            StreamEvent::AttentionCleared { tick, member, .. } => {
                self.current_tick = *tick;

                // Close previous focus window
                if let Some((prev_quest, start_tick)) = self.focus_start.remove(member) {
                    let duration = tick.saturating_sub(start_tick) as u64 * 100;
                    *self
                        .attention
                        .entry(prev_quest)
                        .or_default()
                        .entry(member.clone())
                        .or_default() += duration;
                }

                self.current_focus.insert(member.clone(), None);

                // Record event
                self.events.push(AttentionEvent {
                    tick: *tick,
                    member: member.clone(),
                    quest_id: None,
                    timestamp_ms: (*tick as i64) * 100,
                });
            }

            _ => {}
        }
    }

    /// Get quests ranked by total attention
    pub fn quests_by_attention(&self) -> Vec<QuestAttention> {
        // Collect all quest IDs from both closed and open focus windows
        let mut all_quest_ids: std::collections::HashSet<&str> =
            self.attention.keys().map(|s| s.as_str()).collect();
        for (_, (quest_id, _)) in &self.focus_start {
            all_quest_ids.insert(quest_id.as_str());
        }

        let mut result: Vec<QuestAttention> = all_quest_ids
            .iter()
            .map(|quest_id| {
                let mut by_member = self
                    .attention
                    .get(*quest_id)
                    .cloned()
                    .unwrap_or_default();
                let closed_total: u64 = by_member.values().sum();

                // Add open window durations to both total and per-member maps
                let mut open_duration: u64 = 0;
                for (member, (q, start)) in &self.focus_start {
                    if q.as_str() == *quest_id {
                        let dur = self.current_tick.saturating_sub(*start) as u64 * 100;
                        open_duration += dur;
                        *by_member.entry(member.clone()).or_default() += dur;
                    }
                }

                let currently_focusing: Vec<String> = self
                    .current_focus
                    .iter()
                    .filter_map(|(m, q)| {
                        if q.as_deref() == Some(*quest_id) {
                            Some(m.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                QuestAttention {
                    quest_id: quest_id.to_string(),
                    total_attention_ms: closed_total + open_duration,
                    by_member,
                    currently_focusing,
                }
            })
            .collect();

        // Sort by attention (descending), then by quest_id for stability
        result.sort_by(|a, b| {
            b.total_attention_ms
                .cmp(&a.total_attention_ms)
                .then_with(|| a.quest_id.cmp(&b.quest_id))
        });

        result
    }

    /// Get members sorted by their current focus
    pub fn members_by_focus(&self) -> Vec<(&String, Option<&String>)> {
        let mut members: Vec<_> = self
            .current_focus
            .iter()
            .map(|(m, q)| (m, q.as_ref()))
            .collect();
        members.sort_by(|a, b| a.0.cmp(b.0));
        members
    }

    /// Get all members who have ever focused on something
    pub fn all_members(&self) -> Vec<&String> {
        let mut members: Vec<_> = self.current_focus.keys().collect();
        members.sort();
        members
    }

    /// Get total events
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get current focus for a specific member
    pub fn focus_for_member(&self, member: &str) -> Option<&String> {
        self.current_focus.get(member).and_then(|f| f.as_ref())
    }

    /// Get attention data filtered to quests the member has interacted with
    pub fn attention_for_member(&self, member: &str) -> Vec<QuestAttention> {
        self.quests_by_attention()
            .into_iter()
            .filter(|qa| qa.by_member.contains_key(member) || qa.currently_focusing.contains(&member.to_string()))
            .collect()
    }
}
