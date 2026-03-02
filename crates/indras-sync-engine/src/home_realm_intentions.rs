//! Extension trait adding quest methods to HomeRealm.
//!
//! This mirrors the pattern used by `RealmIntentions` for shared realms,
//! but tailored for the personal home realm where `self.member_id()`
//! is always the creator.

use crate::intention::{Intention, IntentionDocument, IntentionId};
use indras_network::artifact::ArtifactId;
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::home_realm::HomeRealm;
use tracing::debug;

/// Quest management extension trait for HomeRealm.
///
/// Provides convenience methods for personal quest management.
/// All quests are created with `self.member_id()` as the creator,
/// since the home realm is personal.
///
/// # Example
///
/// ```ignore
/// use indras_sync_engine::prelude::*;
///
/// let home = network.home_realm().await?;
/// let intention_id = home.create_intention("Read a book", "Finish chapter 3", None).await?;
/// home.complete_intention(intention_id).await?;
/// ```
pub trait HomeRealmIntentions {
    /// Get the quests document for this home realm.
    async fn intentions(&self) -> Result<Document<IntentionDocument>>;

    /// Seed a welcome quest if the quests document is empty.
    ///
    /// This is idempotent -- if the document already has quests
    /// (e.g., from a CRDT merge on a second device), the welcome
    /// quest is not re-seeded.
    async fn seed_welcome_intention_if_empty(
        &self,
    ) -> Result<()>;

    /// Create a new personal quest.
    ///
    /// The creator is automatically set to `self.member_id()`.
    async fn create_intention(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
    ) -> Result<IntentionId>;

    /// Complete a personal quest.
    async fn complete_intention(
        &self,
        intention_id: IntentionId,
    ) -> Result<()>;

    /// Submit a claim/proof of service for a quest.
    ///
    /// In the home realm, claims are typically self-claims for personal tracking.
    async fn submit_service_claim(
        &self,
        intention_id: IntentionId,
        proof: Option<ArtifactId>,
    ) -> Result<usize>;

    /// Verify a claim on a quest.
    ///
    /// In the home realm, the owner verifies their own claims.
    async fn verify_service_claim(
        &self,
        intention_id: IntentionId,
        claim_index: usize,
    ) -> Result<()>;

    /// Update a quest's title and description.
    async fn update_intention(
        &self,
        intention_id: IntentionId,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
    ) -> Result<()>;
}

impl HomeRealmIntentions for HomeRealm {
    async fn intentions(&self) -> Result<Document<IntentionDocument>> {
        self.document::<IntentionDocument>("intentions").await
    }

    async fn seed_welcome_intention_if_empty(&self) -> Result<()> {
        let doc = self.intentions().await?;
        let data = doc.read().await;

        // Only seed if the quests document is completely empty
        if data.intention_count() > 0 {
            debug!(
                "Home realm already has quests, skipping welcome quest seed"
            );
            return Ok(());
        }
        drop(data);

        let welcome_intention = Intention::new(
            "Explore your home realm",
            "Your home realm is your private space on the network.\n\
             \n\
             Try these:\n\
             - [ ] Create a personal note\n\
             - [ ] Set your display name\n\
             - [ ] Write your pass story (Settings > Identity)\n\
             - [ ] Create a shared realm and generate an invite code\n\
             - [ ] Back up your identity (Settings > Export Identity)\n\
             \n\
             Complete this quest when you feel at home.",
            None,
            self.member_id(),
        );

        doc.update(|d| {
            d.add(welcome_intention);
        })
        .await?;

        debug!("Seeded welcome quest in home realm");

        Ok(())
    }

    async fn create_intention(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
    ) -> Result<IntentionId> {
        let intention = Intention::new(title, description, image, self.member_id());
        let intention_id = intention.id;

        let doc = self.intentions().await?;
        doc.update(|d| {
            d.add(intention);
        })
        .await?;

        Ok(intention_id)
    }

    async fn complete_intention(&self, intention_id: IntentionId) -> Result<()> {
        let doc = self.intentions().await?;
        doc.update(|d| {
            if let Some(intention) = d.find_mut(&intention_id) {
                let _ = intention.complete();
            }
        })
        .await?;

        Ok(())
    }

    async fn submit_service_claim(
        &self,
        intention_id: IntentionId,
        proof: Option<ArtifactId>,
    ) -> Result<usize> {
        let mut claim_index = 0;
        let claimant = self.member_id();
        let doc = self.intentions().await?;
        doc.update(|d| {
            if let Some(intention) = d.find_mut(&intention_id) {
                if let Ok(idx) = intention.submit_claim(claimant, proof) {
                    claim_index = idx;
                }
            }
        })
        .await?;

        Ok(claim_index)
    }

    async fn verify_service_claim(
        &self,
        intention_id: IntentionId,
        claim_index: usize,
    ) -> Result<()> {
        let doc = self.intentions().await?;
        doc.update(|d| {
            if let Some(intention) = d.find_mut(&intention_id) {
                let _ = intention.verify_claim(claim_index);
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
