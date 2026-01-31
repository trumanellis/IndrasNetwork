//! Token of Gratitude state management (discrete token model)
//!
//! Tracks individual Tokens of Gratitude as discrete, transferable objects.
//! Each token has a single steward (owner), immutable provenance, and
//! optional pledge state. Tokens are minted via TokenMinted events and
//! flow through the network via pledge/release/withdraw operations.

use std::collections::HashMap;

use crate::events::StreamEvent;
use crate::state::format_duration_millis;

/// A discrete Token of Gratitude -- minted when a blessing is given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenOfGratitude {
    /// Unique token identifier.
    pub token_id: String,
    /// Current owner/controller of this token.
    pub steward: String,
    /// Pre-computed value from attention event indices (millis).
    /// Computed at mint time and carried in the event for display.
    pub value_millis: u64,
    /// Who gave the blessing that minted this token.
    pub blesser: String,
    /// Quest where the original proof was submitted.
    pub source_quest_id: String,
    /// Who submitted the proof (original recipient).
    pub original_steward: String,
    /// Tick when the token was minted.
    pub created_at_tick: u32,
    /// If pledged, which quest it is pledged to (None if free).
    pub pledged_to: Option<String>,
}

impl TokenOfGratitude {
    /// Format the value as human-readable duration (e.g., "2h 30m").
    pub fn formatted_value(&self) -> String {
        format_duration_millis(self.value_millis)
    }

    /// Whether this token is currently pledged to a quest.
    pub fn is_pledged(&self) -> bool {
        self.pledged_to.is_some()
    }
}

/// State for tracking discrete Tokens of Gratitude.
#[derive(Clone, Debug, Default)]
pub struct TokenState {
    /// Tokens indexed by token_id.
    tokens: HashMap<String, TokenOfGratitude>,
    /// Quest bounties: quest_id -> list of pledged token_ids.
    quest_bounties: HashMap<String, Vec<String>>,
}

impl TokenState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream event and update token state.
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::TokenMinted {
                tick,
                token_id,
                steward,
                value_millis,
                blesser,
                source_quest_id,
                ..
            } => {
                self.tokens.insert(
                    token_id.clone(),
                    TokenOfGratitude {
                        token_id: token_id.clone(),
                        steward: steward.clone(),
                        value_millis: *value_millis,
                        blesser: blesser.clone(),
                        source_quest_id: source_quest_id.clone(),
                        original_steward: steward.clone(),
                        created_at_tick: *tick,
                        pledged_to: None,
                    },
                );
            }

            StreamEvent::GratitudePledged {
                token_id,
                target_quest_id,
                ..
            } => {
                if let Some(token) = self.tokens.get_mut(token_id) {
                    token.pledged_to = Some(target_quest_id.clone());
                }
                // Track in quest bounties
                self.quest_bounties
                    .entry(target_quest_id.clone())
                    .or_default()
                    .push(token_id.clone());
            }

            StreamEvent::GratitudeReleased {
                token_id,
                to_steward,
                target_quest_id,
                ..
            } => {
                if let Some(token) = self.tokens.get_mut(token_id) {
                    token.steward = to_steward.clone();
                    token.pledged_to = None;
                }
                // Remove from quest bounties
                if let Some(bounties) = self.quest_bounties.get_mut(target_quest_id) {
                    bounties.retain(|id| id != token_id);
                }
            }

            StreamEvent::GratitudeWithdrawn {
                token_id,
                target_quest_id,
                ..
            } => {
                if let Some(token) = self.tokens.get_mut(token_id) {
                    token.pledged_to = None;
                }
                // Remove from quest bounties
                if let Some(bounties) = self.quest_bounties.get_mut(target_quest_id) {
                    bounties.retain(|id| id != token_id);
                }
            }

            _ => {}
        }
    }

    /// Get all tokens where the member is the steward.
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

    /// Get unpledged tokens for a member.
    pub fn available_tokens_for_member(&self, member: &str) -> Vec<&TokenOfGratitude> {
        let mut tokens: Vec<_> = self
            .tokens
            .values()
            .filter(|t| t.steward == member && t.pledged_to.is_none())
            .collect();
        tokens.sort_by(|a, b| b.value_millis.cmp(&a.value_millis));
        tokens
    }

    /// Get total bounty (sum of pledged token values) for a quest.
    pub fn quest_bounty(&self, quest_id: &str) -> u64 {
        self.quest_bounties
            .get(quest_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.tokens.get(id))
                    .map(|t| t.value_millis)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Get all tokens pledged to a quest.
    pub fn pledged_tokens_for_quest(&self, quest_id: &str) -> Vec<&TokenOfGratitude> {
        self.quest_bounties
            .get(quest_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.tokens.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get total value of all tokens for a member.
    pub fn total_value_for_member(&self, member: &str) -> u64 {
        self.tokens
            .values()
            .filter(|t| t.steward == member)
            .map(|t| t.value_millis)
            .sum()
    }

    /// Get total value of unpledged tokens for a member.
    pub fn available_value_for_member(&self, member: &str) -> u64 {
        self.tokens
            .values()
            .filter(|t| t.steward == member && t.pledged_to.is_none())
            .map(|t| t.value_millis)
            .sum()
    }

    /// Get count of tokens for a member.
    pub fn token_count_for_member(&self, member: &str) -> usize {
        self.tokens.values().filter(|t| t.steward == member).count()
    }

    /// Get count of pledged tokens for a member.
    pub fn pledged_count_for_member(&self, member: &str) -> usize {
        self.tokens
            .values()
            .filter(|t| t.steward == member && t.pledged_to.is_some())
            .count()
    }

    /// Get total number of tokens minted.
    pub fn total_tokens(&self) -> usize {
        self.tokens.len()
    }

    /// Get count of tokens currently pledged to any quest.
    pub fn total_pledged(&self) -> usize {
        self.tokens.values().filter(|t| t.pledged_to.is_some()).count()
    }

    /// Get number of quests that have at least one pledged bounty.
    pub fn quests_with_bounties(&self) -> usize {
        self.quest_bounties.values().filter(|ids| !ids.is_empty()).count()
    }

    /// Get count of tokens whose current steward differs from original (recycled).
    pub fn recycled_tokens(&self) -> usize {
        self.tokens.values().filter(|t| t.steward != t.original_steward).count()
    }

    /// Get all tokens.
    pub fn all_tokens(&self) -> Vec<&TokenOfGratitude> {
        self.tokens.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::StreamEvent;

    #[test]
    fn test_token_minting() {
        let mut state = TokenState::new();

        state.process_event(&StreamEvent::TokenMinted {
            tick: 100,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            steward: "member1".to_string(),
            value_millis: 30000,
            blesser: "member2".to_string(),
            source_quest_id: "quest1".to_string(),
        });

        let tokens = state.tokens_for_member("member1");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].value_millis, 30000);
        assert_eq!(tokens[0].blesser, "member2");
        assert!(tokens[0].pledged_to.is_none());
    }

    #[test]
    fn test_pledge_and_release() {
        let mut state = TokenState::new();

        // Mint token
        state.process_event(&StreamEvent::TokenMinted {
            tick: 100,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            steward: "member1".to_string(),
            value_millis: 45000,
            blesser: "member2".to_string(),
            source_quest_id: "quest1".to_string(),
        });

        // Pledge to quest
        state.process_event(&StreamEvent::GratitudePledged {
            tick: 110,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            pledger: "member1".to_string(),
            target_quest_id: "quest2".to_string(),
            amount_millis: 45000,
        });

        assert_eq!(state.quest_bounty("quest2"), 45000);
        assert_eq!(state.available_tokens_for_member("member1").len(), 0);

        // Release to new steward
        state.process_event(&StreamEvent::GratitudeReleased {
            tick: 120,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            from_steward: "member1".to_string(),
            to_steward: "member3".to_string(),
            target_quest_id: "quest2".to_string(),
            amount_millis: 45000,
        });

        assert_eq!(state.tokens_for_member("member1").len(), 0);
        assert_eq!(state.tokens_for_member("member3").len(), 1);
        assert_eq!(state.quest_bounty("quest2"), 0); // No longer pledged
    }

    #[test]
    fn test_withdraw() {
        let mut state = TokenState::new();

        state.process_event(&StreamEvent::TokenMinted {
            tick: 100,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            steward: "member1".to_string(),
            value_millis: 30000,
            blesser: "member2".to_string(),
            source_quest_id: "quest1".to_string(),
        });

        state.process_event(&StreamEvent::GratitudePledged {
            tick: 110,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            pledger: "member1".to_string(),
            target_quest_id: "quest2".to_string(),
            amount_millis: 30000,
        });

        state.process_event(&StreamEvent::GratitudeWithdrawn {
            tick: 115,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            steward: "member1".to_string(),
            target_quest_id: "quest2".to_string(),
            amount_millis: 30000,
        });

        assert_eq!(state.available_tokens_for_member("member1").len(), 1);
        assert_eq!(state.quest_bounty("quest2"), 0);
    }

    #[test]
    fn test_token_chaining() {
        let mut state = TokenState::new();

        // Mint to A
        state.process_event(&StreamEvent::TokenMinted {
            tick: 100,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            steward: "A".to_string(),
            value_millis: 60000,
            blesser: "B".to_string(),
            source_quest_id: "quest1".to_string(),
        });

        // A pledges to quest2, releases to C
        state.process_event(&StreamEvent::GratitudePledged {
            tick: 110,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            pledger: "A".to_string(),
            target_quest_id: "quest2".to_string(),
            amount_millis: 60000,
        });

        state.process_event(&StreamEvent::GratitudeReleased {
            tick: 120,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            from_steward: "A".to_string(),
            to_steward: "C".to_string(),
            target_quest_id: "quest2".to_string(),
            amount_millis: 60000,
        });

        // C pledges to quest3, releases to D
        state.process_event(&StreamEvent::GratitudePledged {
            tick: 130,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            pledger: "C".to_string(),
            target_quest_id: "quest3".to_string(),
            amount_millis: 60000,
        });

        state.process_event(&StreamEvent::GratitudeReleased {
            tick: 140,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            from_steward: "C".to_string(),
            to_steward: "D".to_string(),
            target_quest_id: "quest3".to_string(),
            amount_millis: 60000,
        });

        // D now owns the token
        assert_eq!(state.tokens_for_member("D").len(), 1);
        assert_eq!(state.tokens_for_member("D")[0].value_millis, 60000);
        // Original steward preserved
        assert_eq!(state.tokens_for_member("D")[0].original_steward, "A");
    }

    #[test]
    fn test_multiple_tokens_stats() {
        let mut state = TokenState::new();

        state.process_event(&StreamEvent::TokenMinted {
            tick: 100,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            steward: "member1".to_string(),
            value_millis: 10000,
            blesser: "blesser1".to_string(),
            source_quest_id: "quest1".to_string(),
        });

        state.process_event(&StreamEvent::TokenMinted {
            tick: 110,
            realm_id: "realm1".to_string(),
            token_id: "tok2".to_string(),
            steward: "member1".to_string(),
            value_millis: 20000,
            blesser: "blesser2".to_string(),
            source_quest_id: "quest1".to_string(),
        });

        assert_eq!(state.token_count_for_member("member1"), 2);
        assert_eq!(state.total_value_for_member("member1"), 30000);

        // Pledge one
        state.process_event(&StreamEvent::GratitudePledged {
            tick: 120,
            realm_id: "realm1".to_string(),
            token_id: "tok1".to_string(),
            pledger: "member1".to_string(),
            target_quest_id: "quest2".to_string(),
            amount_millis: 10000,
        });

        assert_eq!(state.available_value_for_member("member1"), 20000);
        assert_eq!(state.pledged_count_for_member("member1"), 1);
    }
}
