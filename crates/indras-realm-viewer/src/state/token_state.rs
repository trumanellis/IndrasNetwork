//! Token of Gratitude state management
//!
//! Tracks Tokens of Gratitude - recognized tokens derived from submitted proof folders
//! where the steward is the proof submitter and value equals cumulative blessed attention.

use std::collections::HashMap;

use crate::events::StreamEvent;
use crate::state::format_duration_millis;

/// A Token of Gratitude - derived from a submitted proof folder with accumulated blessings
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenOfGratitude {
    /// Unique folder ID for this proof submission
    pub folder_id: String,
    /// Quest this token relates to
    pub quest_id: String,
    /// Quest title for display
    pub quest_title: String,
    /// The steward (proof submitter) who holds this token
    pub steward: String,
    /// Cumulative blessed attention in milliseconds
    pub value_millis: u64,
    /// Number of unique blessers
    pub blesser_count: usize,
    /// Tick when the proof was submitted
    pub submitted_at_tick: u32,
    /// Preview of the narrative/description
    pub narrative_preview: String,
    /// Number of artifacts in the proof folder
    pub artifact_count: usize,
}

impl TokenOfGratitude {
    /// Format the value as human-readable duration (e.g., "2h 30m")
    pub fn formatted_value(&self) -> String {
        format_duration_millis(self.value_millis)
    }
}

/// State for tracking Tokens of Gratitude
#[derive(Clone, Debug, Default)]
pub struct TokenState {
    /// Tokens indexed by (quest_id, steward)
    tokens: HashMap<(String, String), TokenOfGratitude>,
    /// Track unique blessers per token (quest_id, steward) -> set of blesser IDs
    blessers: HashMap<(String, String), Vec<String>>,
}

impl TokenState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream event and update token state
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ProofFolderSubmitted {
                tick,
                quest_id,
                claimant,
                folder_id,
                artifact_count,
                narrative_preview,
                ..
            } => {
                self.record_submission(
                    quest_id.clone(),
                    claimant.clone(),
                    folder_id.clone(),
                    String::new(), // Quest title will be set later if available
                    *artifact_count,
                    narrative_preview.clone(),
                    *tick,
                );
            }
            // Also track single proof submissions (creates token with 1 artifact)
            StreamEvent::ProofSubmitted {
                tick,
                quest_id,
                claimant,
                quest_title,
                artifact_id,
                ..
            } => {
                self.record_submission(
                    quest_id.clone(),
                    claimant.clone(),
                    artifact_id.clone(), // Use artifact_id as folder_id
                    quest_title.clone(),
                    1, // Single artifact
                    String::new(),
                    *tick,
                );
            }
            StreamEvent::BlessingGiven {
                quest_id,
                claimant,
                blesser,
                attention_millis,
                ..
            } => {
                self.add_blessing(quest_id, claimant, blesser, *attention_millis);
            }
            _ => {}
        }
    }

    /// Record a proof submission, creating a new token with 0 value
    pub fn record_submission(
        &mut self,
        quest_id: String,
        steward: String,
        folder_id: String,
        quest_title: String,
        artifact_count: usize,
        narrative_preview: String,
        tick: u32,
    ) {
        let key = (quest_id.clone(), steward.clone());
        self.tokens.insert(
            key.clone(),
            TokenOfGratitude {
                folder_id,
                quest_id,
                quest_title,
                steward,
                value_millis: 0,
                blesser_count: 0,
                submitted_at_tick: tick,
                narrative_preview,
                artifact_count,
            },
        );
        self.blessers.insert(key, Vec::new());
    }

    /// Add blessed attention to an existing token
    pub fn add_blessing(
        &mut self,
        quest_id: &str,
        claimant: &str,
        blesser: &str,
        attention_millis: u64,
    ) {
        let key = (quest_id.to_string(), claimant.to_string());

        // Only add blessing if token exists (proof was submitted)
        if let Some(token) = self.tokens.get_mut(&key) {
            token.value_millis += attention_millis;

            // Track unique blessers
            let blessers = self.blessers.entry(key).or_default();
            if !blessers.contains(&blesser.to_string()) {
                blessers.push(blesser.to_string());
                token.blesser_count = blessers.len();
            }
        }
    }

    /// Get all tokens where the member is the steward
    pub fn tokens_for_member(&self, member: &str) -> Vec<&TokenOfGratitude> {
        let mut tokens: Vec<_> = self
            .tokens
            .values()
            .filter(|t| t.steward == member)
            .collect();
        // Sort by value descending
        tokens.sort_by(|a, b| b.value_millis.cmp(&a.value_millis));
        tokens
    }

    /// Get total value of all tokens for a member
    pub fn total_value_for_member(&self, member: &str) -> u64 {
        self.tokens
            .values()
            .filter(|t| t.steward == member)
            .map(|t| t.value_millis)
            .sum()
    }

    /// Get count of tokens for a member
    pub fn token_count_for_member(&self, member: &str) -> usize {
        self.tokens.values().filter(|t| t.steward == member).count()
    }

    /// Get all tokens
    pub fn all_tokens(&self) -> Vec<&TokenOfGratitude> {
        self.tokens.values().collect()
    }

    /// Update quest title for a token (called when quest info becomes available)
    pub fn set_quest_title(&mut self, quest_id: &str, steward: &str, title: String) {
        let key = (quest_id.to_string(), steward.to_string());
        if let Some(token) = self.tokens.get_mut(&key) {
            token.quest_title = title;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation_and_blessing() {
        let mut state = TokenState::new();

        // Submit proof
        state.record_submission(
            "quest1".to_string(),
            "member1".to_string(),
            "folder1".to_string(),
            "Test Quest".to_string(),
            3,
            "Did the thing".to_string(),
            100,
        );

        // Check token exists with 0 value
        let tokens = state.tokens_for_member("member1");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].value_millis, 0);

        // Add blessing
        state.add_blessing("quest1", "member1", "blesser1", 5000);

        let tokens = state.tokens_for_member("member1");
        assert_eq!(tokens[0].value_millis, 5000);
        assert_eq!(tokens[0].blesser_count, 1);

        // Add another blessing from same blesser (shouldn't increase count)
        state.add_blessing("quest1", "member1", "blesser1", 3000);
        let tokens = state.tokens_for_member("member1");
        assert_eq!(tokens[0].value_millis, 8000);
        assert_eq!(tokens[0].blesser_count, 1);

        // Add blessing from different blesser
        state.add_blessing("quest1", "member1", "blesser2", 2000);
        let tokens = state.tokens_for_member("member1");
        assert_eq!(tokens[0].value_millis, 10000);
        assert_eq!(tokens[0].blesser_count, 2);
    }
}
