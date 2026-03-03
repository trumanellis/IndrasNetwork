//! Extension trait adding attention tracking methods to Realm.

use crate::attention::{AttentionDocument, AttentionEventId, IntentionAttention};
use crate::intention::IntentionId;
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::Realm;

/// Attention tracking extension trait for Realm.
pub trait RealmAttention {
    /// Get the attention tracking document for this realm.
    async fn attention(&self) -> Result<Document<AttentionDocument>>;

    /// Focus on a specific quest.
    async fn focus_on_intention(
        &self,
        intention_id: IntentionId,
        member: MemberId,
    ) -> Result<AttentionEventId>;

    /// Clear attention (stop focusing on any quest).
    async fn clear_attention(
        &self,
        member: MemberId,
    ) -> Result<AttentionEventId>;

    /// Get current focus for a member.
    async fn get_member_focus(
        &self,
        member: &MemberId,
    ) -> Result<Option<IntentionId>>;

    /// Get all members currently focusing on an intention.
    async fn get_intention_focusers(
        &self,
        intention_id: &IntentionId,
    ) -> Result<Vec<MemberId>>;

    /// Get quests ranked by total attention time.
    async fn intentions_by_attention(
        &self,
    ) -> Result<Vec<IntentionAttention>>;

    /// Get attention details for a specific quest.
    async fn intention_attention(
        &self,
        intention_id: &IntentionId,
    ) -> Result<IntentionAttention>;
}

impl RealmAttention for Realm {
    async fn attention(&self) -> Result<Document<AttentionDocument>> {
        self.document("attention").await
    }

    async fn focus_on_intention(&self, intention_id: IntentionId, member: MemberId) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.focus_on_intention(member, intention_id);
        })
        .await?;

        Ok(event_id)
    }

    async fn clear_attention(&self, member: MemberId) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.clear_attention(member);
        })
        .await?;

        Ok(event_id)
    }

    async fn get_member_focus(&self, member: &MemberId) -> Result<Option<IntentionId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.current_focus(member))
    }

    async fn get_intention_focusers(&self, intention_id: &IntentionId) -> Result<Vec<MemberId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.members_focusing_on(intention_id))
    }

    async fn intentions_by_attention(&self) -> Result<Vec<IntentionAttention>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.intentions_by_attention(None))
    }

    async fn intention_attention(&self, intention_id: &IntentionId) -> Result<IntentionAttention> {
        let doc = self.attention().await?;
        Ok(doc.read().await.intention_attention(intention_id, None))
    }
}
