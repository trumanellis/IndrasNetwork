//! Extension trait adding attention tracking methods to Realm.

use crate::attention::{AttentionDocument, AttentionEventId, QuestAttention};
use crate::quest::QuestId;
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::Realm;

/// Attention tracking extension trait for Realm.
pub trait RealmAttention {
    /// Get the attention tracking document for this realm.
    async fn attention(&self) -> Result<Document<AttentionDocument>>;

    /// Focus on a specific quest.
    async fn focus_on_quest(
        &self,
        quest_id: QuestId,
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
    ) -> Result<Option<QuestId>>;

    /// Get all members currently focusing on a quest.
    async fn get_quest_focusers(
        &self,
        quest_id: &QuestId,
    ) -> Result<Vec<MemberId>>;

    /// Get quests ranked by total attention time.
    async fn quests_by_attention(
        &self,
    ) -> Result<Vec<QuestAttention>>;

    /// Get attention details for a specific quest.
    async fn quest_attention(
        &self,
        quest_id: &QuestId,
    ) -> Result<QuestAttention>;
}

impl RealmAttention for Realm {
    async fn attention(&self) -> Result<Document<AttentionDocument>> {
        self.document("attention").await
    }

    async fn focus_on_quest(&self, quest_id: QuestId, member: MemberId) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.focus_on_quest(member, quest_id);
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

    async fn get_member_focus(&self, member: &MemberId) -> Result<Option<QuestId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.current_focus(member))
    }

    async fn get_quest_focusers(&self, quest_id: &QuestId) -> Result<Vec<MemberId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.members_focusing_on(quest_id))
    }

    async fn quests_by_attention(&self) -> Result<Vec<QuestAttention>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.quests_by_attention(None))
    }

    async fn quest_attention(&self, quest_id: &QuestId) -> Result<QuestAttention> {
        let doc = self.attention().await?;
        Ok(doc.read().await.quest_attention(quest_id, None))
    }
}
