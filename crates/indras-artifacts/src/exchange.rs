use crate::artifact::*;
use crate::error::VaultError;
use crate::store::{ArtifactStore, AttentionStore, PayloadStore};
use crate::story::Story;
use crate::vault::Vault;

type Result<T> = std::result::Result<T, VaultError>;

const ACCEPT_KEY_PREFIX: &str = "accept:";
const OFFERED_LABEL: &str = "offered";
const REQUESTED_LABEL: &str = "requested";
const CONVERSATION_LABEL: &str = "conversation";

/// An Exchange is a Tree Artifact (TreeType::Exchange) representing a
/// negotiation space between two stewards. Contains refs to the two artifacts
/// being discussed + a Story for the negotiation conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub id: ArtifactId,
}

impl Exchange {
    /// Wrap an existing ArtifactId as an Exchange.
    pub fn from_id(id: ArtifactId) -> Self {
        Self { id }
    }

    /// Propose an exchange: create an Exchange tree with refs labeled
    /// "offered" and "requested", plus an empty Story for negotiation.
    pub fn propose<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        vault: &mut Vault<A, P, T>,
        my_artifact_id: ArtifactId,
        their_artifact_id: ArtifactId,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<Self> {
        let tree = vault.place_tree(TreeType::Exchange, audience.clone(), now)?;
        let exchange_id = tree.id.clone();

        // Add offered artifact (position 0)
        vault.compose(
            &exchange_id,
            my_artifact_id,
            0,
            Some(OFFERED_LABEL.to_string()),
        )?;

        // Add requested artifact (position 1)
        vault.compose(
            &exchange_id,
            their_artifact_id,
            1,
            Some(REQUESTED_LABEL.to_string()),
        )?;

        // Create negotiation Story (position 2)
        let story = Story::create(vault, audience, now)?;
        vault.compose(
            &exchange_id,
            story.id,
            2,
            Some(CONVERSATION_LABEL.to_string()),
        )?;

        Ok(Self { id: exchange_id })
    }

    /// Get the negotiation Story.
    pub fn conversation<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Story> {
        let refs = self.get_refs(vault)?;
        let conv_ref = refs
            .iter()
            .find(|r| r.label.as_deref() == Some(CONVERSATION_LABEL))
            .ok_or(VaultError::ArtifactNotFound)?;
        Ok(Story::from_id(conv_ref.artifact_id.clone()))
    }

    /// Get the artifact offered by the initiator.
    pub fn offered<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Option<Artifact>> {
        let refs = self.get_refs(vault)?;
        let offered_ref = refs
            .iter()
            .find(|r| r.label.as_deref() == Some(OFFERED_LABEL));
        match offered_ref {
            Some(r) => vault.get_artifact(&r.artifact_id),
            None => Ok(None),
        }
    }

    /// Get the artifact requested by the initiator.
    pub fn requested<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Option<Artifact>> {
        let refs = self.get_refs(vault)?;
        let req_ref = refs
            .iter()
            .find(|r| r.label.as_deref() == Some(REQUESTED_LABEL));
        match req_ref {
            Some(r) => vault.get_artifact(&r.artifact_id),
            None => Ok(None),
        }
    }

    /// Record this party's acceptance in the Exchange tree metadata.
    pub fn accept<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
    ) -> Result<()> {
        let player = *vault.player();
        let mut artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = artifact.as_tree_mut().ok_or(VaultError::NotATree)?;

        let key = format!("{ACCEPT_KEY_PREFIX}{}", player.iter().map(|b| format!("{b:02x}")).collect::<String>());
        tree.metadata.insert(key, b"true".to_vec());

        // Write updated artifact back to store
        vault.artifact_store_mut().put_artifact(&artifact)?;
        Ok(())
    }

    /// Check if a specific player has accepted.
    pub fn is_accepted_by<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
        player: &PlayerId,
    ) -> Result<bool> {
        let artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = artifact.as_tree().ok_or(VaultError::NotATree)?;

        let key = format!("{ACCEPT_KEY_PREFIX}{}", player.iter().map(|b| format!("{b:02x}")).collect::<String>());
        Ok(tree.metadata.contains_key(&key))
    }

    /// Execute mutual stewardship transfer. Both parties must have accepted.
    pub fn complete<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
    ) -> Result<()> {
        let refs = self.get_refs(vault)?;

        let offered_ref = refs
            .iter()
            .find(|r| r.label.as_deref() == Some(OFFERED_LABEL))
            .ok_or(VaultError::ArtifactNotFound)?;
        let requested_ref = refs
            .iter()
            .find(|r| r.label.as_deref() == Some(REQUESTED_LABEL))
            .ok_or(VaultError::ArtifactNotFound)?;

        let offered = vault
            .get_artifact(&offered_ref.artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let requested = vault
            .get_artifact(&requested_ref.artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?;

        let offerer = *offered.steward();
        let requester = *requested.steward();

        // Both must have accepted
        if !self.is_accepted_by(vault, &offerer)? || !self.is_accepted_by(vault, &requester)? {
            return Err(VaultError::ExchangeNotFullyAccepted);
        }

        // Transfer stewardship both ways
        vault
            .artifact_store_mut()
            .update_steward(&offered_ref.artifact_id, requester)?;
        vault
            .artifact_store_mut()
            .update_steward(&requested_ref.artifact_id, offerer)?;

        Ok(())
    }

    fn get_refs<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Vec<ArtifactRef>> {
        let artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = artifact.as_tree().ok_or(VaultError::NotATree)?;
        Ok(tree.references.clone())
    }
}
