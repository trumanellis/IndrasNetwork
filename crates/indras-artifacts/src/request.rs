use crate::artifact::*;
use crate::error::VaultError;
use crate::store::{ArtifactStore, AttentionStore, PayloadStore};
use crate::vault::Vault;

type Result<T> = std::result::Result<T, VaultError>;

const DESCRIPTION_LABEL: &str = "description";
const OFFER_LABEL: &str = "offer";

/// A Request is a newtype wrapper around an ArtifactId pointing to a
/// Tree(Request) artifact. It represents a player asking for something,
/// with a description and zero or more offers from peers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: ArtifactId,
}

impl Request {
    /// Wrap an existing ArtifactId as a Request.
    pub fn from_id(id: ArtifactId) -> Self {
        Self { id }
    }

    /// Create a new Request tree with a description message.
    pub fn create<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        vault: &mut Vault<A, P, T>,
        description_text: &str,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<Self> {
        let tree = vault.place_tree(TreeType::Request, audience, now)?;
        let request_id = tree.id.clone();

        // Create description as a Leaf(Message) child
        let desc_leaf = vault.place_leaf(description_text.as_bytes(), LeafType::Message, now)?;
        vault.compose(
            &request_id,
            desc_leaf.id,
            0,
            Some(DESCRIPTION_LABEL.to_string()),
        )?;

        Ok(Self { id: request_id })
    }

    /// Add an offer (reference to an existing artifact) to this request.
    pub fn add_offer<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        offer_artifact_id: ArtifactId,
    ) -> Result<()> {
        let next_pos = self.ref_count(vault)? as u64;
        vault.compose(
            &self.id,
            offer_artifact_id,
            next_pos,
            Some(OFFER_LABEL.to_string()),
        )
    }

    /// Get all offers (refs labeled "offer" with resolved artifacts).
    pub fn offers<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Vec<(ArtifactRef, Option<Artifact>)>> {
        let tree_artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = tree_artifact.as_tree().ok_or(VaultError::NotATree)?;

        let mut offers = Vec::new();
        for r in &tree.references {
            if r.label.as_deref() == Some(OFFER_LABEL) {
                let artifact = vault.get_artifact(&r.artifact_id)?;
                offers.push((r.clone(), artifact));
            }
        }
        Ok(offers)
    }

    /// Get the description artifact.
    pub fn description<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Option<Artifact>> {
        let tree_artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = tree_artifact.as_tree().ok_or(VaultError::NotATree)?;

        let desc_ref = tree
            .references
            .iter()
            .find(|r| r.label.as_deref() == Some(DESCRIPTION_LABEL));
        match desc_ref {
            Some(r) => vault.get_artifact(&r.artifact_id),
            None => Ok(None),
        }
    }

    /// Count of offers on this request.
    pub fn offer_count<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<usize> {
        let tree_artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = tree_artifact.as_tree().ok_or(VaultError::NotATree)?;

        Ok(tree
            .references
            .iter()
            .filter(|r| r.label.as_deref() == Some(OFFER_LABEL))
            .count())
    }

    fn ref_count<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<usize> {
        let tree_artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = tree_artifact.as_tree().ok_or(VaultError::NotATree)?;
        Ok(tree.references.len())
    }
}
