//! State tracking for personal intentions in the home realm.

use std::collections::HashMap;

use crate::events::HomeRealmEvent;

/// Status of an intention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentionStatus {
    Active,
    Completed,
}

/// A personal intention in the home realm.
#[derive(Debug, Clone, PartialEq)]
pub struct HomeIntention {
    pub id: String,
    pub title: String,
    pub status: IntentionStatus,
    pub created_tick: u32,
    pub completed_tick: Option<u32>,
}

impl HomeIntention {
    /// Creates a new active intention.
    pub fn new(id: String, title: String, created_tick: u32) -> Self {
        Self {
            id,
            title,
            status: IntentionStatus::Active,
            created_tick,
            completed_tick: None,
        }
    }
}

/// State for tracking all intentions.
#[derive(Debug, Clone, Default)]
pub struct IntentionsState {
    /// Map of intention_id -> HomeIntention
    pub intentions: HashMap<String, HomeIntention>,
}

impl IntentionsState {
    /// Creates a new empty intentions state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a home realm event that may affect intentions.
    pub fn process_event(&mut self, event: &HomeRealmEvent) {
        match event {
            HomeRealmEvent::HomeQuestCreated {
                quest_id,
                title,
                tick,
                ..
            } => {
                let intention = HomeIntention::new(quest_id.clone(), title.clone(), *tick);
                self.intentions.insert(quest_id.clone(), intention);
            }
            HomeRealmEvent::HomeQuestCompleted { quest_id, tick, .. } => {
                if let Some(intention) = self.intentions.get_mut(quest_id) {
                    intention.status = IntentionStatus::Completed;
                    intention.completed_tick = Some(*tick);
                }
            }
            _ => {}
        }
    }

    /// Returns the count of active intentions.
    pub fn active_count(&self) -> usize {
        self.intentions
            .values()
            .filter(|i| i.status == IntentionStatus::Active)
            .count()
    }

    /// Returns the count of completed intentions.
    pub fn completed_count(&self) -> usize {
        self.intentions
            .values()
            .filter(|i| i.status == IntentionStatus::Completed)
            .count()
    }

    /// Returns intentions sorted by creation tick (newest first).
    pub fn intentions_by_recency(&self) -> Vec<&HomeIntention> {
        let mut intentions: Vec<_> = self.intentions.values().collect();
        intentions.sort_by(|a, b| b.created_tick.cmp(&a.created_tick));
        intentions
    }

    /// Returns only active intentions sorted by recency.
    pub fn active_intentions(&self) -> Vec<&HomeIntention> {
        let mut intentions: Vec<_> = self
            .intentions
            .values()
            .filter(|i| i.status == IntentionStatus::Active)
            .collect();
        intentions.sort_by(|a, b| b.created_tick.cmp(&a.created_tick));
        intentions
    }

    /// Resets the state.
    pub fn reset(&mut self) {
        self.intentions.clear();
    }
}
