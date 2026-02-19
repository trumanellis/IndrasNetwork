use crate::artifact::*;
use crate::error::VaultError;
use crate::store::{ArtifactStore, AttentionStore, PayloadStore};
use crate::vault::Vault;

type Result<T> = std::result::Result<T, VaultError>;

/// A Story is an artifact (type "story") representing a sequential
/// journey through artifacts. A conversation is a Story where most children
/// are chat messages. Any sequential experience is a Story.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Story {
    pub id: ArtifactId,
}

impl Story {
    /// Wrap an existing ArtifactId as a Story (must point to a "story" artifact).
    pub fn from_id(id: ArtifactId) -> Self {
        Self { id }
    }

    /// Create a new Story artifact.
    pub fn create<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        vault: &mut Vault<A, P, T>,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<Self> {
        let artifact = vault.place_tree("story", audience, now)?;
        Ok(Self { id: artifact.id })
    }

    /// Create a DM story with a deterministic ID derived from both player IDs.
    /// Both players will generate the same ArtifactId for the same pair.
    pub fn create_dm<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        vault: &mut Vault<A, P, T>,
        peer_id: PlayerId,
        now: i64,
    ) -> Result<Self> {
        let self_id = *vault.player();
        let id = crate::artifact::dm_story_id(self_id, peer_id);
        let audience = vec![self_id, peer_id];
        let artifact = vault.place_tree_with_id(id, "story", audience, now)?;
        Ok(Self { id: artifact.id })
    }

    /// Append an artifact at the next position in the Story.
    pub fn append<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        artifact_id: ArtifactId,
        label: Option<String>,
    ) -> Result<()> {
        let next_pos = self.entry_count(vault)?;
        vault.compose(&self.id, artifact_id, next_pos as u64, label)
    }

    /// Convenience: create a Message leaf and append it to the Story.
    pub fn send_message<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        text: &str,
        now: i64,
    ) -> Result<ArtifactId> {
        let leaf = vault.place_leaf(
            text.as_bytes(),
            "message".to_string(),
            None,
            "message",
            now,
        )?;
        self.append(vault, leaf.id, None)?;
        Ok(leaf.id)
    }

    /// Get ordered entries (refs + resolved artifacts).
    pub fn entries<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Vec<(ArtifactRef, Option<Artifact>)>> {
        let story = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;

        let mut entries: Vec<(ArtifactRef, Option<Artifact>)> = Vec::new();
        for r in &story.references {
            let artifact = vault.get_artifact(&r.artifact_id)?;
            entries.push((r.clone(), artifact));
        }
        // References are already sorted by position in the store
        entries.sort_by_key(|(r, _)| r.position);
        Ok(entries)
    }

    /// Count of entries in the Story.
    pub fn entry_count<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<usize> {
        let story = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        Ok(story.references.len())
    }

    /// Branch: create a sub-Story from a specific position.
    pub fn branch<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        from_position: u64,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<Story> {
        let sub = Story::create(vault, audience, now)?;
        vault.compose(
            &self.id,
            sub.id,
            from_position,
            Some("branch".to_string()),
        )?;
        Ok(sub)
    }
}
