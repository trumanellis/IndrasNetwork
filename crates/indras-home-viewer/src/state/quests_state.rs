//! State tracking for personal quests in the home realm.

use std::collections::HashMap;

use crate::events::HomeRealmEvent;

/// Status of a quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestStatus {
    Active,
    Completed,
}

/// A personal quest in the home realm.
#[derive(Debug, Clone, PartialEq)]
pub struct HomeQuest {
    pub id: String,
    pub title: String,
    pub status: QuestStatus,
    pub created_tick: u32,
    pub completed_tick: Option<u32>,
}

impl HomeQuest {
    /// Creates a new active quest.
    pub fn new(id: String, title: String, created_tick: u32) -> Self {
        Self {
            id,
            title,
            status: QuestStatus::Active,
            created_tick,
            completed_tick: None,
        }
    }
}

/// State for tracking all quests.
#[derive(Debug, Clone, Default)]
pub struct QuestsState {
    /// Map of quest_id -> HomeQuest
    pub quests: HashMap<String, HomeQuest>,
}

impl QuestsState {
    /// Creates a new empty quests state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a home realm event that may affect quests.
    pub fn process_event(&mut self, event: &HomeRealmEvent) {
        match event {
            HomeRealmEvent::HomeQuestCreated {
                quest_id,
                title,
                tick,
                ..
            } => {
                let quest = HomeQuest::new(quest_id.clone(), title.clone(), *tick);
                self.quests.insert(quest_id.clone(), quest);
            }
            HomeRealmEvent::HomeQuestCompleted { quest_id, tick, .. } => {
                if let Some(quest) = self.quests.get_mut(quest_id) {
                    quest.status = QuestStatus::Completed;
                    quest.completed_tick = Some(*tick);
                }
            }
            _ => {}
        }
    }

    /// Returns the count of active quests.
    pub fn active_count(&self) -> usize {
        self.quests
            .values()
            .filter(|q| q.status == QuestStatus::Active)
            .count()
    }

    /// Returns the count of completed quests.
    pub fn completed_count(&self) -> usize {
        self.quests
            .values()
            .filter(|q| q.status == QuestStatus::Completed)
            .count()
    }

    /// Returns quests sorted by creation tick (newest first).
    pub fn quests_by_recency(&self) -> Vec<&HomeQuest> {
        let mut quests: Vec<_> = self.quests.values().collect();
        quests.sort_by(|a, b| b.created_tick.cmp(&a.created_tick));
        quests
    }

    /// Returns only active quests sorted by recency.
    pub fn active_quests(&self) -> Vec<&HomeQuest> {
        let mut quests: Vec<_> = self
            .quests
            .values()
            .filter(|q| q.status == QuestStatus::Active)
            .collect();
        quests.sort_by(|a, b| b.created_tick.cmp(&a.created_tick));
        quests
    }

    /// Resets the state.
    pub fn reset(&mut self) {
        self.quests.clear();
    }
}
