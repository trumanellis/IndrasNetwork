//! Extension trait adding token of gratitude methods to Realm.
//!
//! Includes the attention→gratitude bridge: `token_attention_millis()` computes
//! the raw attention duration backing a token, and `token_subjective_value()`
//! combines it with trust and humanness for observer-specific valuation.

use crate::attention::AttentionDocument;
use crate::content::SyncContent;
use crate::humanness::HumannessDocument;
use crate::intention::IntentionId;
use crate::token_of_gratitude::{TokenOfGratitude, TokenOfGratitudeDocument, TokenOfGratitudeId};
use crate::token_valuation::{SubjectiveTokenValue, subjective_value};
use indras_network::document::Document;
use indras_network::error::{IndraError, Result};
use indras_network::member::MemberId;
use indras_network::Realm;

/// Token of Gratitude management extension trait for Realm.
pub trait RealmTokens {
    /// Get the token of gratitude document for this realm.
    async fn tokens(&self) -> Result<Document<TokenOfGratitudeDocument>>;

    /// Pledge a token to a quest as a bounty incentive.
    ///
    /// Only the token's current steward is authorized to pledge it.
    async fn pledge_token(
        &self,
        token_id: TokenOfGratitudeId,
        target_intention_id: IntentionId,
        caller: MemberId,
    ) -> Result<()>;

    /// Release a pledged token to a new steward (transfer ownership).
    ///
    /// Only the token's current steward is authorized to release it.
    async fn release_token(
        &self,
        token_id: TokenOfGratitudeId,
        new_steward: MemberId,
        caller: MemberId,
    ) -> Result<()>;

    /// Withdraw a pledge (return token to steward's wallet).
    ///
    /// Only the token's current steward is authorized to withdraw it.
    async fn withdraw_token(
        &self,
        token_id: TokenOfGratitudeId,
        caller: MemberId,
    ) -> Result<()>;

    /// Get all tokens pledged to a quest.
    async fn intention_pledged_tokens(
        &self,
        intention_id: &IntentionId,
    ) -> Result<Vec<TokenOfGratitude>>;

    /// Get all tokens owned by a member.
    async fn member_tokens(
        &self,
        member: &MemberId,
    ) -> Result<Vec<TokenOfGratitude>>;

    /// Compute raw attention milliseconds backing a token.
    ///
    /// Reads the attention document and calculates the total duration
    /// of focus sessions referenced by the token's `event_indices`.
    /// This is the objective component of token value — same for all observers.
    async fn token_attention_millis(
        &self,
        token_id: &TokenOfGratitudeId,
    ) -> Result<u64>;

    /// Compute the subjective value of a token from the local node's perspective.
    ///
    /// Combines:
    /// - Raw attention millis (from the attention document)
    /// - Trust chain weight (from the provided sentiment function)
    /// - Humanness freshness (from the humanness document)
    ///
    /// The `sentiment_fn` is observer-specific: it returns the local node's
    /// sentiment toward a given member, or `None` if unknown.
    async fn token_subjective_value(
        &self,
        token_id: &TokenOfGratitudeId,
        sentiment_fn: &dyn Fn(&MemberId) -> Option<f64>,
    ) -> Result<SubjectiveTokenValue>;
}

impl RealmTokens for Realm {
    async fn tokens(&self) -> Result<Document<TokenOfGratitudeDocument>> {
        self.document("_tokens").await
    }

    async fn pledge_token(
        &self,
        token_id: TokenOfGratitudeId,
        target_intention_id: IntentionId,
        caller: MemberId,
    ) -> Result<()> {
        let token_doc = self.tokens().await?;
        let pledger;

        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) => {
                    if token.steward != caller {
                        return Err(IndraError::InvalidOperation(
                            "Not authorized: only the token's steward can pledge it".into(),
                        ));
                    }
                    pledger = token.steward;
                }
                None => {
                    return Err(IndraError::InvalidOperation("Token not found".into()));
                }
            }
        }

        token_doc
            .try_update(|d| {
                d.pledge(token_id, target_intention_id)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;

        self.send(SyncContent::GratitudePledged {
            token_id,
            pledger,
            target_intention_id,
        }.to_content())
        .await?;

        Ok(())
    }

    async fn release_token(
        &self,
        token_id: TokenOfGratitudeId,
        new_steward: MemberId,
        caller: MemberId,
    ) -> Result<()> {
        let token_doc = self.tokens().await?;
        let from_steward;
        let target_intention_id;

        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) => {
                    if token.steward != caller {
                        return Err(IndraError::InvalidOperation(
                            "Not authorized: only the token's steward can release it".into(),
                        ));
                    }
                    from_steward = token.steward;
                    target_intention_id = token.pledged_to.unwrap_or([0u8; 16]);
                }
                None => {
                    return Err(IndraError::InvalidOperation("Token not found".into()));
                }
            }
        }

        token_doc
            .try_update(|d| {
                d.release(token_id, new_steward)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;

        self.send(SyncContent::GratitudeReleased {
            token_id,
            from_steward,
            to_steward: new_steward,
            target_intention_id,
        }.to_content())
        .await?;

        Ok(())
    }

    async fn withdraw_token(&self, token_id: TokenOfGratitudeId, caller: MemberId) -> Result<()> {
        let token_doc = self.tokens().await?;
        let steward;
        let target_intention_id;

        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) => {
                    if token.steward != caller {
                        return Err(IndraError::InvalidOperation(
                            "Not authorized: only the token's steward can withdraw it".into(),
                        ));
                    }
                    steward = token.steward;
                    target_intention_id = token.pledged_to.unwrap_or([0u8; 16]);
                }
                None => {
                    return Err(IndraError::InvalidOperation("Token not found".into()));
                }
            }
        }

        token_doc
            .try_update(|d| {
                d.withdraw(token_id)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;

        self.send(SyncContent::GratitudeWithdrawn {
            token_id,
            steward,
            target_intention_id,
        }.to_content())
        .await?;

        Ok(())
    }

    async fn intention_pledged_tokens(&self, intention_id: &IntentionId) -> Result<Vec<TokenOfGratitude>> {
        let token_doc = self.tokens().await?;
        let guard = token_doc.read().await;
        Ok(guard
            .pledged_tokens_for_intention(intention_id)
            .into_iter()
            .cloned()
            .collect())
    }

    async fn member_tokens(&self, member: &MemberId) -> Result<Vec<TokenOfGratitude>> {
        let token_doc = self.tokens().await?;
        let guard = token_doc.read().await;
        Ok(guard
            .tokens_for_steward(member)
            .into_iter()
            .cloned()
            .collect())
    }

    async fn token_attention_millis(
        &self,
        token_id: &TokenOfGratitudeId,
    ) -> Result<u64> {
        let token_doc = self.tokens().await?;
        let token_guard = token_doc.read().await;
        let token = token_guard.find(token_id).ok_or_else(|| {
            IndraError::InvalidOperation("token not found".to_string())
        })?;
        let event_indices = token.event_indices.clone();
        drop(token_guard);

        let attention_doc: Document<AttentionDocument> = self.document("attention").await?;
        let attention_guard = attention_doc.read().await;
        Ok(attention_guard.compute_attention_millis(&event_indices, None))
    }

    async fn token_subjective_value(
        &self,
        token_id: &TokenOfGratitudeId,
        sentiment_fn: &dyn Fn(&MemberId) -> Option<f64>,
    ) -> Result<SubjectiveTokenValue> {
        // 1. Get the token
        let token_doc = self.tokens().await?;
        let token_guard = token_doc.read().await;
        let token = token_guard.find(token_id).ok_or_else(|| {
            IndraError::InvalidOperation("token not found".to_string())
        })?.clone();
        drop(token_guard);

        // 2. Compute raw attention millis
        let attention_doc: Document<AttentionDocument> = self.document("attention").await?;
        let attention_guard = attention_doc.read().await;
        let raw_millis = attention_guard.compute_attention_millis(&token.event_indices, None);
        drop(attention_guard);

        // 3. Get humanness freshness for the blesser
        let humanness_doc: Document<HumannessDocument> = self.document("_humanness").await?;
        let humanness_guard = humanness_doc.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let humanness_fn = |member: &MemberId| -> f64 {
            humanness_guard.freshness_at(member, now)
        };

        // 4. Compute subjective value
        let value = subjective_value(&token, raw_millis, sentiment_fn, humanness_fn);

        Ok(value)
    }
}
