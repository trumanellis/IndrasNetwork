//! Token of Gratitude system for quest bounty incentives.
//!
//! Each blessing mints a discrete Token of Gratitude -- a persistent, transferable
//! object with immutable provenance and a mutable steward (owner). Stewards can
//! pledge tokens to quests as bounty incentives, then release them to proof
//! submitters (updating the steward field). Tokens flow through the network as
//! gratitude changes hands.
//!
//! # Key Properties
//!
//! - **1:1 blessing-to-token**: Each `bless_claim()` call mints exactly one token
//! - **Value is derived**: Stored as `event_indices` into the AttentionDocument;
//!   value is computed on demand from duration spans
//! - **Steward is mutable**: The only mutable field; changes on release
//! - **Append-only event log**: CRDT document with `Minted`, `Pledged`, `Released`,
//!   `Withdrawn` entries; current state derived from log replay
//! - **Realm-scoped**: Tokens live within the realm where they were minted

use crate::blessing::BlessingId;
use crate::member::MemberId;
use crate::quest::QuestId;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a Token of Gratitude (16 bytes).
pub type TokenOfGratitudeId = [u8; 16];

/// Generate a new unique token ID.
pub fn generate_token_id() -> TokenOfGratitudeId {
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

/// A Token of Gratitude -- minted when a blessing is given.
///
/// Value is not stored directly -- it is computed from `event_indices`
/// by looking up durations in the AttentionDocument.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenOfGratitude {
    /// Unique identifier for this token.
    pub id: TokenOfGratitudeId,
    /// Current owner/controller of this token.
    pub steward: MemberId,

    // Immutable provenance (set at mint, never changes)
    /// The blessing that produced this token.
    pub blessing_id: BlessingId,
    /// Who gave the blessing that minted this token.
    pub blesser: MemberId,
    /// Quest where the original proof was submitted.
    pub source_quest_id: QuestId,
    /// Who submitted the proof (original recipient).
    pub original_steward: MemberId,
    /// Indices into AttentionDocument.events -- the "backing asset".
    /// Value is computed from these by calculating duration spans.
    pub event_indices: Vec<usize>,
    /// When the token was minted (Unix timestamp in milliseconds).
    pub created_at_millis: i64,

    // Mutable pledge state
    /// If pledged, which quest is it pledged to.
    pub pledged_to: Option<QuestId>,
    /// When it was pledged (None if not pledged).
    pub pledged_at_millis: Option<i64>,
}

/// Events in the token lifecycle (append-only log entries).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TokenEvent {
    /// A new token was minted from a blessing.
    Minted {
        token_id: TokenOfGratitudeId,
        steward: MemberId,
        blessing_id: BlessingId,
        blesser: MemberId,
        source_quest_id: QuestId,
        original_steward: MemberId,
        event_indices: Vec<usize>,
        created_at_millis: i64,
    },
    /// A token was pledged to a quest as bounty.
    Pledged {
        token_id: TokenOfGratitudeId,
        target_quest_id: QuestId,
        pledged_at_millis: i64,
    },
    /// A pledged token was released to a new steward.
    Released {
        token_id: TokenOfGratitudeId,
        new_steward: MemberId,
    },
    /// A pledge was withdrawn (token returned to steward's wallet).
    Withdrawn {
        token_id: TokenOfGratitudeId,
    },
}

/// Errors that can occur during token operations.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenError {
    /// Token not found.
    TokenNotFound,
    /// Caller doesn't own the token.
    NotSteward,
    /// Token is already pledged to a quest.
    AlreadyPledged,
    /// Token is not pledged (can't release/withdraw).
    NotPledged,
    /// Can't mint a token with zero event indices.
    ZeroValue,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::TokenNotFound => write!(f, "Token not found"),
            TokenError::NotSteward => write!(f, "Caller does not own this token"),
            TokenError::AlreadyPledged => write!(f, "Token is already pledged to a quest"),
            TokenError::NotPledged => write!(f, "Token is not pledged"),
            TokenError::ZeroValue => write!(f, "Cannot mint a token with zero event indices"),
        }
    }
}

impl std::error::Error for TokenError {}

/// CRDT document for Token of Gratitude tracking.
///
/// Maintains an append-only event log with derived state for quick lookups.
/// The derived state is recomputable from events and must be rebuilt after merges.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenOfGratitudeDocument {
    /// Append-only event log.
    events: Vec<TokenEvent>,
    /// Derived state: token_id -> TokenOfGratitude.
    #[serde(default)]
    tokens: HashMap<TokenOfGratitudeId, TokenOfGratitude>,
}

impl TokenOfGratitudeDocument {
    /// Create a new empty document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a new Token of Gratitude.
    ///
    /// Called when a blessing is given. Creates a token with the claimant as steward.
    ///
    /// # Arguments
    ///
    /// * `steward` - The initial owner (proof submitter / claimant)
    /// * `blessing_id` - The blessing that produced this token
    /// * `blesser` - Who gave the blessing
    /// * `source_quest_id` - Quest where the proof was submitted
    /// * `event_indices` - Indices into AttentionDocument.events (the backing asset)
    ///
    /// # Returns
    ///
    /// The token ID of the newly minted token.
    pub fn mint(
        &mut self,
        steward: MemberId,
        blessing_id: BlessingId,
        blesser: MemberId,
        source_quest_id: QuestId,
        event_indices: Vec<usize>,
    ) -> Result<TokenOfGratitudeId, TokenError> {
        if event_indices.is_empty() {
            return Err(TokenError::ZeroValue);
        }

        let token_id = generate_token_id();
        let created_at_millis = chrono::Utc::now().timestamp_millis();

        let event = TokenEvent::Minted {
            token_id,
            steward,
            blessing_id,
            blesser,
            source_quest_id,
            original_steward: steward,
            event_indices: event_indices.clone(),
            created_at_millis,
        };
        self.events.push(event);

        // Update derived state
        self.tokens.insert(
            token_id,
            TokenOfGratitude {
                id: token_id,
                steward,
                blessing_id,
                blesser,
                source_quest_id,
                original_steward: steward,
                event_indices,
                created_at_millis,
                pledged_to: None,
                pledged_at_millis: None,
            },
        );

        Ok(token_id)
    }

    /// Pledge a token to a quest as a bounty incentive.
    ///
    /// The token must be owned by the caller and not already pledged.
    pub fn pledge(
        &mut self,
        token_id: TokenOfGratitudeId,
        target_quest_id: QuestId,
    ) -> Result<(), TokenError> {
        let token = self.tokens.get(&token_id).ok_or(TokenError::TokenNotFound)?;

        if token.pledged_to.is_some() {
            return Err(TokenError::AlreadyPledged);
        }

        let pledged_at_millis = chrono::Utc::now().timestamp_millis();

        let event = TokenEvent::Pledged {
            token_id,
            target_quest_id,
            pledged_at_millis,
        };
        self.events.push(event);

        // Update derived state
        if let Some(token) = self.tokens.get_mut(&token_id) {
            token.pledged_to = Some(target_quest_id);
            token.pledged_at_millis = Some(pledged_at_millis);
        }

        Ok(())
    }

    /// Release a pledged token to a new steward (transfer ownership).
    ///
    /// The token must be currently pledged.
    pub fn release(
        &mut self,
        token_id: TokenOfGratitudeId,
        new_steward: MemberId,
    ) -> Result<(), TokenError> {
        let token = self.tokens.get(&token_id).ok_or(TokenError::TokenNotFound)?;

        if token.pledged_to.is_none() {
            return Err(TokenError::NotPledged);
        }

        let event = TokenEvent::Released {
            token_id,
            new_steward,
        };
        self.events.push(event);

        // Update derived state
        if let Some(token) = self.tokens.get_mut(&token_id) {
            token.steward = new_steward;
            token.pledged_to = None;
            token.pledged_at_millis = None;
        }

        Ok(())
    }

    /// Withdraw a pledge (return token to steward's wallet).
    ///
    /// The token must be currently pledged.
    pub fn withdraw(
        &mut self,
        token_id: TokenOfGratitudeId,
    ) -> Result<(), TokenError> {
        let token = self.tokens.get(&token_id).ok_or(TokenError::TokenNotFound)?;

        if token.pledged_to.is_none() {
            return Err(TokenError::NotPledged);
        }

        let event = TokenEvent::Withdrawn { token_id };
        self.events.push(event);

        // Update derived state
        if let Some(token) = self.tokens.get_mut(&token_id) {
            token.pledged_to = None;
            token.pledged_at_millis = None;
        }

        Ok(())
    }

    /// Find a token by ID.
    pub fn find(&self, token_id: &TokenOfGratitudeId) -> Option<&TokenOfGratitude> {
        self.tokens.get(token_id)
    }

    /// Find a mutable reference to a token by ID.
    pub fn find_mut(&mut self, token_id: &TokenOfGratitudeId) -> Option<&mut TokenOfGratitude> {
        self.tokens.get_mut(token_id)
    }

    /// Get all tokens where the specified member is the steward.
    pub fn tokens_for_steward(&self, member: &MemberId) -> Vec<&TokenOfGratitude> {
        self.tokens
            .values()
            .filter(|t| &t.steward == member)
            .collect()
    }

    /// Get unpledged tokens owned by the specified member.
    pub fn available_tokens_for_steward(&self, member: &MemberId) -> Vec<&TokenOfGratitude> {
        self.tokens
            .values()
            .filter(|t| &t.steward == member && t.pledged_to.is_none())
            .collect()
    }

    /// Get all tokens pledged to a specific quest.
    pub fn pledged_tokens_for_quest(&self, quest_id: &QuestId) -> Vec<&TokenOfGratitude> {
        self.tokens
            .values()
            .filter(|t| t.pledged_to.as_ref() == Some(quest_id))
            .collect()
    }

    /// Get the count of tokens for a steward.
    pub fn token_count_for_steward(&self, member: &MemberId) -> usize {
        self.tokens
            .values()
            .filter(|t| &t.steward == member)
            .count()
    }

    /// Get the event log (for inspection/debugging).
    pub fn events(&self) -> &[TokenEvent] {
        &self.events
    }

    /// Get all tokens (for inspection/debugging).
    pub fn all_tokens(&self) -> Vec<&TokenOfGratitude> {
        self.tokens.values().collect()
    }

    /// Rebuild derived state from the event log.
    ///
    /// Must be called after CRDT merges to ensure consistency.
    pub fn rebuild_derived_state(&mut self) {
        self.tokens.clear();

        for event in &self.events {
            match event {
                TokenEvent::Minted {
                    token_id,
                    steward,
                    blessing_id,
                    blesser,
                    source_quest_id,
                    original_steward,
                    event_indices,
                    created_at_millis,
                } => {
                    self.tokens.insert(
                        *token_id,
                        TokenOfGratitude {
                            id: *token_id,
                            steward: *steward,
                            blessing_id: *blessing_id,
                            blesser: *blesser,
                            source_quest_id: *source_quest_id,
                            original_steward: *original_steward,
                            event_indices: event_indices.clone(),
                            created_at_millis: *created_at_millis,
                            pledged_to: None,
                            pledged_at_millis: None,
                        },
                    );
                }
                TokenEvent::Pledged {
                    token_id,
                    target_quest_id,
                    pledged_at_millis,
                } => {
                    if let Some(token) = self.tokens.get_mut(token_id) {
                        token.pledged_to = Some(*target_quest_id);
                        token.pledged_at_millis = Some(*pledged_at_millis);
                    }
                }
                TokenEvent::Released {
                    token_id,
                    new_steward,
                } => {
                    if let Some(token) = self.tokens.get_mut(token_id) {
                        token.steward = *new_steward;
                        token.pledged_to = None;
                        token.pledged_at_millis = None;
                    }
                }
                TokenEvent::Withdrawn { token_id } => {
                    if let Some(token) = self.tokens.get_mut(token_id) {
                        token.pledged_to = None;
                        token.pledged_at_millis = None;
                    }
                }
            }
        }
    }

    /// Merge another document into this one (CRDT merge).
    ///
    /// Uses union of events + dedupe by token_id for Minted events.
    /// Non-Minted events are deduped by exact equality.
    /// Automatically rebuilds derived state after merge.
    pub fn merge(&mut self, other: &TokenOfGratitudeDocument) {
        let mut all_events: Vec<TokenEvent> = self.events.clone();

        for event in &other.events {
            let is_duplicate = match event {
                TokenEvent::Minted { token_id, .. } => {
                    all_events.iter().any(|e| matches!(e, TokenEvent::Minted { token_id: id, .. } if id == token_id))
                }
                other_event => all_events.contains(other_event),
            };

            if !is_duplicate {
                all_events.push(event.clone());
            }
        }

        self.events = all_events;
        self.rebuild_derived_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_member_id(n: u8) -> MemberId {
        [n; 32]
    }

    fn test_quest_id(n: u8) -> QuestId {
        [n; 16]
    }

    fn test_blessing_id(n: u8) -> BlessingId {
        [n; 16]
    }

    #[test]
    fn test_mint_token() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);
        let blesser = test_member_id(2);
        let quest_id = test_quest_id(1);
        let blessing_id = test_blessing_id(1);

        let token_id = doc
            .mint(steward, blessing_id, blesser, quest_id, vec![0, 1, 2])
            .unwrap();

        let token = doc.find(&token_id).unwrap();
        assert_eq!(token.steward, steward);
        assert_eq!(token.blesser, blesser);
        assert_eq!(token.source_quest_id, quest_id);
        assert_eq!(token.original_steward, steward);
        assert_eq!(token.event_indices, vec![0, 1, 2]);
        assert!(token.pledged_to.is_none());
    }

    #[test]
    fn test_mint_zero_value_rejected() {
        let mut doc = TokenOfGratitudeDocument::new();
        let result = doc.mint(
            test_member_id(1),
            test_blessing_id(1),
            test_member_id(2),
            test_quest_id(1),
            vec![],
        );
        assert_eq!(result, Err(TokenError::ZeroValue));
    }

    #[test]
    fn test_pledge_token() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);
        let target_quest = test_quest_id(2);

        let token_id = doc
            .mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0, 1])
            .unwrap();

        doc.pledge(token_id, target_quest).unwrap();

        let token = doc.find(&token_id).unwrap();
        assert_eq!(token.pledged_to, Some(target_quest));
        assert!(token.pledged_at_millis.is_some());
    }

    #[test]
    fn test_double_pledge_rejected() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);
        let quest_a = test_quest_id(2);
        let quest_b = test_quest_id(3);

        let token_id = doc
            .mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();

        doc.pledge(token_id, quest_a).unwrap();
        let result = doc.pledge(token_id, quest_b);
        assert_eq!(result, Err(TokenError::AlreadyPledged));
    }

    #[test]
    fn test_release_token() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);
        let new_steward = test_member_id(3);
        let target_quest = test_quest_id(2);

        let token_id = doc
            .mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0, 1])
            .unwrap();

        doc.pledge(token_id, target_quest).unwrap();
        doc.release(token_id, new_steward).unwrap();

        let token = doc.find(&token_id).unwrap();
        assert_eq!(token.steward, new_steward);
        assert!(token.pledged_to.is_none());
        assert!(token.pledged_at_millis.is_none());
        // Provenance unchanged
        assert_eq!(token.original_steward, steward);
        assert_eq!(token.blesser, test_member_id(2));
    }

    #[test]
    fn test_release_unpledged_rejected() {
        let mut doc = TokenOfGratitudeDocument::new();
        let token_id = doc
            .mint(test_member_id(1), test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();

        let result = doc.release(token_id, test_member_id(3));
        assert_eq!(result, Err(TokenError::NotPledged));
    }

    #[test]
    fn test_withdraw_token() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);
        let target_quest = test_quest_id(2);

        let token_id = doc
            .mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();

        doc.pledge(token_id, target_quest).unwrap();
        doc.withdraw(token_id).unwrap();

        let token = doc.find(&token_id).unwrap();
        assert!(token.pledged_to.is_none());
        assert!(token.pledged_at_millis.is_none());
        assert_eq!(token.steward, steward); // Still owned by original steward
    }

    #[test]
    fn test_withdraw_unpledged_rejected() {
        let mut doc = TokenOfGratitudeDocument::new();
        let token_id = doc
            .mint(test_member_id(1), test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();

        let result = doc.withdraw(token_id);
        assert_eq!(result, Err(TokenError::NotPledged));
    }

    #[test]
    fn test_tokens_for_steward() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward_a = test_member_id(1);
        let steward_b = test_member_id(3);

        doc.mint(steward_a, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();
        doc.mint(steward_a, test_blessing_id(2), test_member_id(2), test_quest_id(1), vec![1])
            .unwrap();
        doc.mint(steward_b, test_blessing_id(3), test_member_id(2), test_quest_id(1), vec![2])
            .unwrap();

        assert_eq!(doc.tokens_for_steward(&steward_a).len(), 2);
        assert_eq!(doc.tokens_for_steward(&steward_b).len(), 1);
    }

    #[test]
    fn test_available_tokens_for_steward() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);

        let t1 = doc
            .mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();
        let _t2 = doc
            .mint(steward, test_blessing_id(2), test_member_id(2), test_quest_id(1), vec![1])
            .unwrap();

        doc.pledge(t1, test_quest_id(2)).unwrap();

        assert_eq!(doc.tokens_for_steward(&steward).len(), 2);
        assert_eq!(doc.available_tokens_for_steward(&steward).len(), 1);
    }

    #[test]
    fn test_pledged_tokens_for_quest() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);
        let target = test_quest_id(2);

        let t1 = doc
            .mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();
        let t2 = doc
            .mint(steward, test_blessing_id(2), test_member_id(2), test_quest_id(1), vec![1])
            .unwrap();

        doc.pledge(t1, target).unwrap();
        doc.pledge(t2, target).unwrap();

        assert_eq!(doc.pledged_tokens_for_quest(&target).len(), 2);
        assert_eq!(doc.pledged_tokens_for_quest(&test_quest_id(3)).len(), 0);
    }

    #[test]
    fn test_token_chaining() {
        // Demonstrate a token flowing through 3 stewards
        let mut doc = TokenOfGratitudeDocument::new();
        let steward_a = test_member_id(1); // Original recipient
        let steward_b = test_member_id(3); // Second recipient
        let steward_c = test_member_id(4); // Third recipient

        let quest_a = test_quest_id(1);
        let quest_b = test_quest_id(2);
        let quest_c = test_quest_id(3);

        // Mint to A
        let token_id = doc
            .mint(steward_a, test_blessing_id(1), test_member_id(2), quest_a, vec![0, 1])
            .unwrap();

        // A pledges to quest B, releases to B
        doc.pledge(token_id, quest_b).unwrap();
        doc.release(token_id, steward_b).unwrap();

        let token = doc.find(&token_id).unwrap();
        assert_eq!(token.steward, steward_b);
        assert_eq!(token.original_steward, steward_a); // Provenance preserved

        // B pledges to quest C, releases to C
        doc.pledge(token_id, quest_c).unwrap();
        doc.release(token_id, steward_c).unwrap();

        let token = doc.find(&token_id).unwrap();
        assert_eq!(token.steward, steward_c);
        assert_eq!(token.original_steward, steward_a); // Still original
        assert_eq!(token.blesser, test_member_id(2)); // Still original blesser
    }

    #[test]
    fn test_merge_documents() {
        let mut doc1 = TokenOfGratitudeDocument::new();
        let mut doc2 = TokenOfGratitudeDocument::new();

        let steward = test_member_id(1);
        let blesser = test_member_id(2);

        doc1.mint(steward, test_blessing_id(1), blesser, test_quest_id(1), vec![0])
            .unwrap();
        doc2.mint(steward, test_blessing_id(2), blesser, test_quest_id(1), vec![1])
            .unwrap();

        doc1.merge(&doc2);

        assert_eq!(doc1.tokens_for_steward(&steward).len(), 2);
    }

    #[test]
    fn test_merge_deduplication() {
        let mut doc1 = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);

        doc1.mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0])
            .unwrap();

        let doc2 = doc1.clone();
        doc1.merge(&doc2);

        // Should not duplicate
        assert_eq!(doc1.tokens_for_steward(&steward).len(), 1);
    }

    #[test]
    fn test_rebuild_derived_state() {
        let mut doc = TokenOfGratitudeDocument::new();
        let steward = test_member_id(1);
        let target = test_quest_id(2);

        let token_id = doc
            .mint(steward, test_blessing_id(1), test_member_id(2), test_quest_id(1), vec![0, 1])
            .unwrap();

        doc.pledge(token_id, target).unwrap();

        // Clear derived state and rebuild
        doc.tokens.clear();
        doc.rebuild_derived_state();

        let token = doc.find(&token_id).unwrap();
        assert_eq!(token.steward, steward);
        assert_eq!(token.pledged_to, Some(target));
    }
}
