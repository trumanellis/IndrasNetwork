//! Extension trait adding proof folder methods to Realm.

use crate::content::SyncContent;
use crate::proof_folder::{
    ProofFolder, ProofFolderArtifact, ProofFolderDocument, ProofFolderError, ProofFolderId,
};
use crate::quest::QuestId;
use crate::realm_quests::RealmQuests;
use indras_network::artifact::ArtifactId;
use indras_network::document::Document;
use indras_network::error::{IndraError, Result};
use indras_network::member::MemberId;
use indras_network::Realm;

/// Proof folder management extension trait for Realm.
pub trait RealmProofFolders {
    /// Get the proof folders document for this realm.
    async fn proof_folders(&self) -> Result<Document<ProofFolderDocument>>;

    /// Create a new proof folder in draft status.
    async fn create_proof_folder(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<ProofFolderId>;

    /// Update the narrative in a proof folder.
    async fn update_proof_folder_narrative(
        &self,
        folder_id: ProofFolderId,
        narrative: impl Into<String> + Send,
    ) -> Result<()>;

    /// Add an artifact to a proof folder.
    async fn add_artifact_to_proof_folder(
        &self,
        folder_id: ProofFolderId,
        artifact: ProofFolderArtifact,
    ) -> Result<()>;

    /// Remove an artifact from a proof folder.
    async fn remove_artifact_from_proof_folder(
        &self,
        folder_id: ProofFolderId,
        artifact_id: ArtifactId,
    ) -> Result<()>;

    /// Submit a proof folder for review.
    async fn submit_proof_folder(
        &self,
        folder_id: ProofFolderId,
    ) -> Result<usize>;
}

impl RealmProofFolders for Realm {
    async fn proof_folders(&self) -> Result<Document<ProofFolderDocument>> {
        self.document("proof_folders").await
    }

    async fn create_proof_folder(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<ProofFolderId> {
        let folder = ProofFolder::new(quest_id, claimant);
        let folder_id = folder.id;

        let doc = self.proof_folders().await?;
        doc.update(|d| {
            d.add(folder);
        })
        .await?;

        Ok(folder_id)
    }

    async fn update_proof_folder_narrative(
        &self,
        folder_id: ProofFolderId,
        narrative: impl Into<String> + Send,
    ) -> Result<()> {
        let narrative = narrative.into();
        let doc = self.proof_folders().await?;

        let mut result = Ok(());
        doc.update(|d| {
            if let Some(folder) = d.find_mut(&folder_id) {
                result = folder.set_narrative(&narrative).map_err(|e| match e {
                    ProofFolderError::NotDraft => IndraError::InvalidOperation(
                        "Cannot update narrative: folder is not in draft status".into(),
                    ),
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            } else {
                result = Err(IndraError::InvalidOperation("Proof folder not found".into()));
            }
        })
        .await?;

        result
    }

    async fn add_artifact_to_proof_folder(
        &self,
        folder_id: ProofFolderId,
        artifact: ProofFolderArtifact,
    ) -> Result<()> {
        let doc = self.proof_folders().await?;

        let mut result = Ok(());
        doc.update(|d| {
            if let Some(folder) = d.find_mut(&folder_id) {
                result = folder.add_artifact(artifact.clone()).map_err(|e| match e {
                    ProofFolderError::NotDraft => IndraError::InvalidOperation(
                        "Cannot add artifact: folder is not in draft status".into(),
                    ),
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            } else {
                result = Err(IndraError::InvalidOperation("Proof folder not found".into()));
            }
        })
        .await?;

        result
    }

    async fn remove_artifact_from_proof_folder(
        &self,
        folder_id: ProofFolderId,
        artifact_id: ArtifactId,
    ) -> Result<()> {
        let doc = self.proof_folders().await?;

        let mut result = Ok(());
        doc.update(|d| {
            if let Some(folder) = d.find_mut(&folder_id) {
                result = folder.remove_artifact(&artifact_id).map_err(|e| match e {
                    ProofFolderError::NotDraft => IndraError::InvalidOperation(
                        "Cannot remove artifact: folder is not in draft status".into(),
                    ),
                    ProofFolderError::ArtifactNotFound => {
                        IndraError::InvalidOperation("Artifact not found in folder".into())
                    }
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            } else {
                result = Err(IndraError::InvalidOperation("Proof folder not found".into()));
            }
        })
        .await?;

        result
    }

    async fn submit_proof_folder(&self, folder_id: ProofFolderId) -> Result<usize> {
        // First, get folder info and submit it
        let doc = self.proof_folders().await?;
        let guard = doc.read().await;
        let folder = guard.find(&folder_id).ok_or_else(|| {
            IndraError::InvalidOperation("Proof folder not found".into())
        })?;

        if folder.is_submitted() {
            return Err(IndraError::InvalidOperation(
                "Proof folder has already been submitted".into(),
            ));
        }

        let quest_id = folder.quest_id;
        let claimant = folder.claimant;
        let narrative_preview = folder.narrative_preview();
        let artifact_count = folder.artifact_count();

        drop(guard);

        // Submit the folder
        let mut submit_result = Ok(());
        doc.update(|d| {
            if let Some(f) = d.find_mut(&folder_id) {
                submit_result = f.submit().map_err(|e| match e {
                    ProofFolderError::AlreadySubmitted => IndraError::InvalidOperation(
                        "Proof folder has already been submitted".into(),
                    ),
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            }
        })
        .await?;

        submit_result?;

        // Create or update quest claim with proof folder
        let mut claim_index = 0usize;
        let quests = self.quests().await?;
        quests
            .update(|d| {
                if let Some(quest) = d.find_mut(&quest_id) {
                    if let Some((idx, claim)) = quest
                        .claims
                        .iter_mut()
                        .enumerate()
                        .find(|(_, c)| c.claimant == claimant)
                    {
                        claim.set_proof_folder(folder_id);
                        claim_index = idx;
                    } else {
                        let claim = crate::quest::QuestClaim::with_proof_folder(claimant, folder_id);
                        quest.claims.push(claim);
                        claim_index = quest.claims.len() - 1;
                    }
                }
            })
            .await?;

        // Post chat notification
        self.send(SyncContent::ProofFolderSubmitted {
            quest_id,
            claimant,
            folder_id,
            narrative_preview,
            artifact_count,
        }.to_content())
        .await?;

        Ok(claim_index)
    }
}
