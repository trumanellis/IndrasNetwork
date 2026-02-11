//! Extension trait adding quest methods to Realm.

use crate::content::SyncContent;
use crate::quest::{Quest, QuestDocument, QuestId, QuestPriority};
use indras_network::artifact::ArtifactId;
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::message::ArtifactRef;
use indras_network::Realm;

/// Quest management extension trait for Realm.
pub trait RealmQuests {
    /// Get the quests document for this realm.
    async fn quests(&self) -> Result<Document<QuestDocument>>;

    /// Create a new quest.
    async fn create_quest(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
        creator: MemberId,
    ) -> Result<QuestId>;

    /// Submit a claim/proof of service for a quest.
    async fn submit_quest_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        proof: Option<ArtifactId>,
    ) -> Result<usize>;

    /// Submit a quest claim with proof artifact and post to realm chat.
    async fn submit_quest_proof(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        proof_artifact: ArtifactRef,
    ) -> Result<usize>;

    /// Verify a claim on a quest.
    async fn verify_quest_claim(
        &self,
        quest_id: QuestId,
        claim_index: usize,
    ) -> Result<()>;

    /// Mark a quest as complete.
    async fn complete_quest(
        &self,
        quest_id: QuestId,
    ) -> Result<()>;

    /// Set a deadline on a quest.
    async fn set_quest_deadline(
        &self,
        quest_id: QuestId,
        deadline_millis: i64,
    ) -> Result<()>;

    /// Set the priority on a quest.
    async fn set_quest_priority(
        &self,
        quest_id: QuestId,
        priority: QuestPriority,
    ) -> Result<()>;

    /// Update a quest's title and description.
    async fn update_quest(
        &self,
        quest_id: QuestId,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
    ) -> Result<()>;
}

impl RealmQuests for Realm {
    async fn quests(&self) -> Result<Document<QuestDocument>> {
        self.document::<QuestDocument>("quests").await
    }

    async fn create_quest(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
        creator: MemberId,
    ) -> Result<QuestId> {
        let quest = Quest::new(title, description, image, creator);
        let quest_id = quest.id;

        let doc = self.quests().await?;
        doc.update(|d| {
            d.add(quest);
        })
        .await?;

        Ok(quest_id)
    }

    async fn submit_quest_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        proof: Option<ArtifactId>,
    ) -> Result<usize> {
        let mut claim_index = 0;
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                if let Ok(idx) = quest.submit_claim(claimant, proof) {
                    claim_index = idx;
                }
            }
        })
        .await?;

        Ok(claim_index)
    }

    async fn submit_quest_proof(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        proof_artifact: ArtifactRef,
    ) -> Result<usize> {
        let claim_index = self
            .submit_quest_claim(quest_id, claimant, Some(proof_artifact.hash))
            .await?;

        self.send(SyncContent::ProofSubmitted {
            quest_id,
            claimant,
            artifact: proof_artifact,
        }.to_content())
        .await?;

        Ok(claim_index)
    }

    async fn verify_quest_claim(&self, quest_id: QuestId, claim_index: usize) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                let _ = quest.verify_claim(claim_index);
            }
        })
        .await?;

        Ok(())
    }

    async fn complete_quest(&self, quest_id: QuestId) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                let _ = quest.complete();
            }
        })
        .await?;

        Ok(())
    }

    async fn set_quest_deadline(&self, quest_id: QuestId, deadline_millis: i64) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                quest.set_deadline(deadline_millis);
            }
        })
        .await?;

        Ok(())
    }

    async fn set_quest_priority(&self, quest_id: QuestId, priority: QuestPriority) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                quest.set_priority(priority);
            }
        })
        .await?;

        Ok(())
    }

    async fn update_quest(
        &self,
        quest_id: QuestId,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
    ) -> Result<()> {
        let title = title.into();
        let description = description.into();
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                quest.set_title(title);
                quest.set_description(description);
            }
        })
        .await?;

        Ok(())
    }
}
