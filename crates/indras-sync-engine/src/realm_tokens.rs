//! Extension trait adding token of gratitude methods to Realm.

use crate::content::SyncContent;
use crate::intention::IntentionId;
use crate::token_of_gratitude::{TokenOfGratitude, TokenOfGratitudeDocument, TokenOfGratitudeId};
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
}
