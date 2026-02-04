//! Extension trait adding quest methods to HomeRealm.
//!
//! This mirrors the pattern used by `RealmQuests` for shared realms,
//! but tailored for the personal home realm where `self.member_id()`
//! is always the creator.

use crate::quest::{Quest, QuestDocument, QuestId};
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
/// let quest_id = home.create_quest("Read a book", "Finish chapter 3", None).await?;
/// home.complete_quest(quest_id).await?;
/// ```
pub trait HomeRealmQuests {
    /// Get the quests document for this home realm.
    async fn quests(&self) -> Result<Document<QuestDocument>>;

    /// Seed a welcome quest if the quests document is empty.
    ///
    /// This is idempotent -- if the document already has quests
    /// (e.g., from a CRDT merge on a second device), the welcome
    /// quest is not re-seeded.
    async fn seed_welcome_quest_if_empty(
        &self,
    ) -> Result<()>;

    /// Create a new personal quest.
    ///
    /// The creator is automatically set to `self.member_id()`.
    async fn create_quest(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
    ) -> Result<QuestId>;

    /// Complete a personal quest.
    async fn complete_quest(
        &self,
        quest_id: QuestId,
    ) -> Result<()>;
}

impl HomeRealmQuests for HomeRealm {
    async fn quests(&self) -> Result<Document<QuestDocument>> {
        self.document::<QuestDocument>("quests").await
    }

    async fn seed_welcome_quest_if_empty(&self) -> Result<()> {
        let doc = self.quests().await?;
        let data = doc.read().await;

        // Only seed if the quests document is completely empty
        if data.quest_count() > 0 {
            debug!(
                "Home realm already has quests, skipping welcome quest seed"
            );
            return Ok(());
        }
        drop(data);

        let welcome_quest = Quest::new(
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
            d.add(welcome_quest);
        })
        .await?;

        debug!("Seeded welcome quest in home realm");

        Ok(())
    }

    async fn create_quest(
        &self,
        title: impl Into<String> + Send,
        description: impl Into<String> + Send,
        image: Option<ArtifactId>,
    ) -> Result<QuestId> {
        let quest = Quest::new(title, description, image, self.member_id());
        let quest_id = quest.id;

        let doc = self.quests().await?;
        doc.update(|d| {
            d.add(quest);
        })
        .await?;

        Ok(quest_id)
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
}
