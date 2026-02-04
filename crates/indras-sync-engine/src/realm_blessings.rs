//! Extension trait adding blessing methods to Realm.

use std::time::Duration;

use crate::blessing::{Blessing, BlessingDocument, BlessingId, ClaimId};
use crate::content::SyncContent;
use crate::quest::QuestId;
use crate::realm_attention::RealmAttention;
use crate::realm_tokens::RealmTokens;
use indras_network::document::Document;
use indras_network::error::{IndraError, Result};
use indras_network::member::MemberId;
use indras_network::Realm;

/// Blessing management extension trait for Realm.
pub trait RealmBlessings {
    /// Get the blessings document for this realm.
    async fn blessings(&self) -> Result<Document<BlessingDocument>>;

    /// Bless a quest claim by releasing accumulated attention.
    async fn bless_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        blesser: MemberId,
        event_indices: Vec<usize>,
    ) -> Result<BlessingId>;

    /// Get all blessings for a specific quest claim.
    async fn blessings_for_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<Vec<Blessing>>;

    /// Get the total blessed attention duration for a quest claim.
    async fn blessed_attention_duration(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<Duration>;

    /// Get attention event indices that haven't been blessed yet.
    async fn unblessed_event_indices(
        &self,
        member: MemberId,
        quest_id: QuestId,
    ) -> Result<Vec<usize>>;

    /// Get the total unblessed attention duration available for blessing.
    async fn unblessed_attention_duration(
        &self,
        member: MemberId,
        quest_id: QuestId,
    ) -> Result<Duration>;
}

impl RealmBlessings for Realm {
    async fn blessings(&self) -> Result<Document<BlessingDocument>> {
        self.document("blessings").await
    }

    async fn bless_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        blesser: MemberId,
        event_indices: Vec<usize>,
    ) -> Result<BlessingId> {
        // Validate that blesser owns the attention events
        let attention_doc = self.attention().await?;
        let attention = attention_doc.read().await;
        let events = attention.events();

        for &idx in &event_indices {
            if idx >= events.len() {
                return Err(IndraError::InvalidOperation(format!(
                    "Invalid event index: {} (only {} events exist)",
                    idx,
                    events.len()
                )));
            }
            let event = &events[idx];
            if event.member != blesser {
                return Err(IndraError::InvalidOperation(format!(
                    "Event {} belongs to different member, not blesser",
                    idx
                )));
            }
            if event.quest_id != Some(quest_id) {
                return Err(IndraError::InvalidOperation(format!(
                    "Event {} is for different quest",
                    idx
                )));
            }
        }
        drop(attention);

        // Record the blessing
        let claim_id = ClaimId::new(quest_id, claimant);
        let mut blessing_id = [0u8; 16];
        let blessing_doc = self.blessings().await?;

        let event_indices_clone = event_indices.clone();
        blessing_doc
            .update(|d| {
                match d.bless_claim(claim_id, blesser, event_indices_clone) {
                    Ok(id) => blessing_id = id,
                    Err(e) => {
                        tracing::warn!("Blessing failed: {}", e);
                    }
                }
            })
            .await?;

        // Mint a Token of Gratitude for the claimant
        let token_doc = self.tokens().await?;
        let event_indices_for_token = event_indices.clone();
        let mut _token_id = [0u8; 16];
        token_doc
            .update(|d| {
                match d.mint(claimant, blessing_id, blesser, quest_id, event_indices_for_token) {
                    Ok(id) => _token_id = id,
                    Err(e) => {
                        tracing::warn!("Token minting failed: {}", e);
                    }
                }
            })
            .await?;

        // Post BlessingGiven message to chat
        self.send(SyncContent::BlessingGiven {
            quest_id,
            claimant,
            blesser,
            event_indices,
        }.to_content())
        .await?;

        Ok(blessing_id)
    }

    async fn blessings_for_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<Vec<Blessing>> {
        let claim_id = ClaimId::new(quest_id, claimant);
        let doc = self.blessings().await?;
        Ok(doc
            .read()
            .await
            .blessings_for_claim(&claim_id)
            .into_iter()
            .cloned()
            .collect())
    }

    async fn blessed_attention_duration(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<Duration> {
        let claim_id = ClaimId::new(quest_id, claimant);
        let blessing_doc = self.blessings().await?;
        let attention_doc = self.attention().await?;

        let blessings_data = blessing_doc.read().await;
        let attention_data = attention_doc.read().await;
        let events = attention_data.events();

        let mut total_millis: u64 = 0;

        for blessing in blessings_data.blessings_for_claim(&claim_id) {
            for &idx in &blessing.event_indices {
                if idx < events.len() {
                    let event = &events[idx];
                    let end_time = events
                        .iter()
                        .skip(idx + 1)
                        .find(|e| e.member == event.member)
                        .map(|e| e.timestamp_millis)
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

                    let duration = (end_time - event.timestamp_millis).max(0) as u64;
                    total_millis += duration;
                }
            }
        }

        Ok(Duration::from_millis(total_millis))
    }

    async fn unblessed_event_indices(
        &self,
        member: MemberId,
        quest_id: QuestId,
    ) -> Result<Vec<usize>> {
        let attention_doc = self.attention().await?;
        let blessing_doc = self.blessings().await?;

        let attention_data = attention_doc.read().await;
        let blessing_data = blessing_doc.read().await;
        let events = attention_data.events();

        let candidate_indices: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.member == member && e.quest_id == Some(quest_id))
            .map(|(idx, _)| idx)
            .collect();

        Ok(blessing_data.unblessed_event_indices(&member, &quest_id, &candidate_indices))
    }

    async fn unblessed_attention_duration(
        &self,
        member: MemberId,
        quest_id: QuestId,
    ) -> Result<Duration> {
        let unblessed = self.unblessed_event_indices(member, quest_id).await?;
        let attention_doc = self.attention().await?;
        let attention_data = attention_doc.read().await;
        let events = attention_data.events();

        let mut total_millis: u64 = 0;

        for idx in unblessed {
            if idx < events.len() {
                let event = &events[idx];
                let end_time = events
                    .iter()
                    .skip(idx + 1)
                    .find(|e| e.member == event.member)
                    .map(|e| e.timestamp_millis)
                    .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

                let duration = (end_time - event.timestamp_millis).max(0) as u64;
                total_millis += duration;
            }
        }

        Ok(Duration::from_millis(total_millis))
    }
}
