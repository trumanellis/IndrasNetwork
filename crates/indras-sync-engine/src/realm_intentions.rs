//! Extension trait adding quest methods to Realm.

use crate::content::SyncContent;
use crate::intention::{Intention, IntentionDocument, IntentionId, IntentionPriority};
use indras_network::artifact::ArtifactId;
use indras_network::document::Document;
use indras_network::error::{IndraError, Result};
use indras_network::member::MemberId;
use indras_network::message::ContentReference;
use indras_network::Realm;

/// Quest management extension trait for Realm.
pub trait RealmIntentions {
    /// Get the quests document for this realm.
    async fn intentions(&self) -> Result<Document<IntentionDocument>>;

    /// Create a new quest.
    async fn create_intention(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
        creator: MemberId,
    ) -> Result<IntentionId>;

    /// Submit a claim/proof of service for a quest.
    async fn submit_service_claim(
        &self,
        intention_id: IntentionId,
        claimant: MemberId,
        proof: Option<ArtifactId>,
    ) -> Result<usize>;

    /// Submit a quest claim with proof artifact and post to realm chat.
    async fn submit_intention_proof(
        &self,
        intention_id: IntentionId,
        claimant: MemberId,
        proof_artifact: ContentReference,
    ) -> Result<usize>;

    /// Verify a claim on a quest.
    ///
    /// Only the intention's creator is authorized to verify claims.
    async fn verify_service_claim(
        &self,
        intention_id: IntentionId,
        claim_index: usize,
        caller: MemberId,
    ) -> Result<()>;

    /// Mark a quest as complete.
    ///
    /// Only the intention's creator is authorized to complete it.
    async fn complete_intention(
        &self,
        intention_id: IntentionId,
        caller: MemberId,
    ) -> Result<()>;

    /// Set a deadline on a quest.
    async fn set_intention_deadline(
        &self,
        intention_id: IntentionId,
        deadline_millis: i64,
    ) -> Result<()>;

    /// Set the priority on a quest.
    async fn set_intention_priority(
        &self,
        intention_id: IntentionId,
        priority: IntentionPriority,
    ) -> Result<()>;

    /// Update a quest's title and description.
    async fn update_intention(
        &self,
        intention_id: IntentionId,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
    ) -> Result<()>;
}

impl RealmIntentions for Realm {
    async fn intentions(&self) -> Result<Document<IntentionDocument>> {
        self.document::<IntentionDocument>("intentions").await
    }

    async fn create_intention(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
        creator: MemberId,
    ) -> Result<IntentionId> {
        let intention = Intention::new(title, description, image, creator);
        let intention_id = intention.id;

        let doc = self.intentions().await?;
        doc.update(|d| {
            d.add(intention);
        })
        .await?;

        Ok(intention_id)
    }

    async fn submit_service_claim(
        &self,
        intention_id: IntentionId,
        claimant: MemberId,
        proof: Option<ArtifactId>,
    ) -> Result<usize> {
        let doc = self.intentions().await?;
        let claim_index = doc
            .try_update(|d| {
                let intention = d
                    .find_mut(&intention_id)
                    .ok_or_else(|| IndraError::InvalidOperation("Intention not found".into()))?;
                intention
                    .submit_claim(claimant, proof)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;

        Ok(claim_index)
    }

    async fn submit_intention_proof(
        &self,
        intention_id: IntentionId,
        claimant: MemberId,
        proof_artifact: ContentReference,
    ) -> Result<usize> {
        let claim_index = self
            .submit_service_claim(intention_id, claimant, Some(ArtifactId::Blob(proof_artifact.hash)))
            .await?;

        self.send(SyncContent::ProofSubmitted {
            intention_id,
            claimant,
            artifact: proof_artifact,
        }.to_content())
        .await?;

        Ok(claim_index)
    }

    async fn verify_service_claim(&self, intention_id: IntentionId, claim_index: usize, caller: MemberId) -> Result<()> {
        let doc = self.intentions().await?;

        {
            let guard = doc.read().await;
            match guard.find(&intention_id) {
                Some(intention) => {
                    if intention.creator != caller {
                        return Err(IndraError::InvalidOperation(
                            "Not authorized: only the intention creator can verify claims".into(),
                        ));
                    }
                }
                None => {
                    return Err(IndraError::InvalidOperation("Intention not found".into()));
                }
            }
        }

        doc.try_update(|d| {
            let intention = d
                .find_mut(&intention_id)
                .ok_or_else(|| IndraError::InvalidOperation("Intention not found".into()))?;
            intention
                .verify_claim(claim_index)
                .map_err(|e| IndraError::InvalidOperation(e.to_string()))
        })
        .await?;

        Ok(())
    }

    async fn complete_intention(&self, intention_id: IntentionId, caller: MemberId) -> Result<()> {
        let doc = self.intentions().await?;

        {
            let guard = doc.read().await;
            match guard.find(&intention_id) {
                Some(intention) => {
                    if intention.creator != caller {
                        return Err(IndraError::InvalidOperation(
                            "Not authorized: only the intention creator can complete it".into(),
                        ));
                    }
                }
                None => {
                    return Err(IndraError::InvalidOperation("Intention not found".into()));
                }
            }
        }

        doc.try_update(|d| {
            let intention = d
                .find_mut(&intention_id)
                .ok_or_else(|| IndraError::InvalidOperation("Intention not found".into()))?;
            intention
                .complete()
                .map_err(|e| IndraError::InvalidOperation(e.to_string()))
        })
        .await?;

        Ok(())
    }

    async fn set_intention_deadline(&self, intention_id: IntentionId, deadline_millis: i64) -> Result<()> {
        let doc = self.intentions().await?;
        doc.update(|d| {
            if let Some(intention) = d.find_mut(&intention_id) {
                intention.set_deadline(deadline_millis);
            }
        })
        .await?;

        Ok(())
    }

    async fn set_intention_priority(&self, intention_id: IntentionId, priority: IntentionPriority) -> Result<()> {
        let doc = self.intentions().await?;
        doc.update(|d| {
            if let Some(intention) = d.find_mut(&intention_id) {
                intention.set_priority(priority);
            }
        })
        .await?;

        Ok(())
    }

    async fn update_intention(
        &self,
        intention_id: IntentionId,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
    ) -> Result<()> {
        let title = title.into();
        let description = description.into();
        let doc = self.intentions().await?;
        doc.update(|d| {
            if let Some(intention) = d.find_mut(&intention_id) {
                intention.set_title(title);
                intention.set_description(description);
            }
        })
        .await?;

        Ok(())
    }
}
