use bytes::Bytes;
use std::collections::HashMap;

use crate::access::{AccessGrant, ArtifactStatus};
use crate::artifact::{Artifact, ArtifactId, ArtifactRef, PlayerId, StewardshipRecord, TreeType};
use crate::attention::AttentionSwitchEvent;
use crate::error::VaultError;

type Result<T> = std::result::Result<T, VaultError>;

/// Result of checking a peer's attention log against our replica.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegrityResult {
    /// Logs are consistent — theirs matches ours exactly.
    Consistent,
    /// New events appended (normal sync).
    Extended { new_events: usize },
    /// Events were modified or removed — divergence detected.
    Diverged { first_mismatch_index: usize },
    /// We have no prior replica to compare against.
    NoPriorReplica,
}

// ---------------------------------------------------------------------------
// ArtifactStore
// ---------------------------------------------------------------------------

/// Store and retrieve artifact metadata (steward, grants, type, refs).
pub trait ArtifactStore {
    fn put_artifact(&mut self, artifact: &Artifact) -> Result<()>;
    fn get_artifact(&self, id: &ArtifactId) -> Result<Option<Artifact>>;
    fn list_by_type(&self, tree_type: &TreeType) -> Result<Vec<ArtifactId>>;
    fn list_by_steward(&self, steward: &PlayerId) -> Result<Vec<ArtifactId>>;
    fn update_steward(&mut self, id: &ArtifactId, new_steward: PlayerId) -> Result<()>;
    fn update_status(&mut self, id: &ArtifactId, status: ArtifactStatus) -> Result<()>;
    fn add_grant(&mut self, id: &ArtifactId, grant: AccessGrant) -> Result<()>;
    fn remove_grant(&mut self, id: &ArtifactId, grantee: &PlayerId) -> Result<()>;
    fn set_parent(&mut self, id: &ArtifactId, parent: Option<ArtifactId>) -> Result<()>;
    fn list_by_status(&self, status: &ArtifactStatus) -> Result<Vec<ArtifactId>>;
    fn accessible_by(&self, player: &PlayerId, now: i64) -> Result<Vec<ArtifactId>>;
    fn delete_artifact(&mut self, id: &ArtifactId) -> Result<()>;
    fn add_ref(&mut self, tree_id: &ArtifactId, child_ref: ArtifactRef) -> Result<()>;
    fn remove_ref(&mut self, tree_id: &ArtifactId, child_id: &ArtifactId) -> Result<()>;
    fn record_stewardship_transfer(&mut self, id: &ArtifactId, record: StewardshipRecord) -> Result<()>;
    fn steward_history(&self, id: &ArtifactId) -> Result<Vec<StewardshipRecord>>;
}

/// In-memory artifact store backed by HashMap.
#[derive(Default)]
pub struct InMemoryArtifactStore {
    artifacts: HashMap<ArtifactId, Artifact>,
    stewardship_history: HashMap<ArtifactId, Vec<StewardshipRecord>>,
}

impl InMemoryArtifactStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ArtifactStore for InMemoryArtifactStore {
    fn put_artifact(&mut self, artifact: &Artifact) -> Result<()> {
        self.artifacts.insert(artifact.id().clone(), artifact.clone());
        Ok(())
    }

    fn get_artifact(&self, id: &ArtifactId) -> Result<Option<Artifact>> {
        Ok(self.artifacts.get(id).cloned())
    }

    fn list_by_type(&self, tree_type: &TreeType) -> Result<Vec<ArtifactId>> {
        Ok(self
            .artifacts
            .values()
            .filter_map(|a| {
                if let Artifact::Tree(t) = a {
                    if &t.artifact_type == tree_type {
                        return Some(a.id().clone());
                    }
                }
                None
            })
            .collect())
    }

    fn list_by_steward(&self, steward: &PlayerId) -> Result<Vec<ArtifactId>> {
        Ok(self
            .artifacts
            .values()
            .filter(|a| a.steward() == steward)
            .map(|a| a.id().clone())
            .collect())
    }

    fn update_steward(&mut self, id: &ArtifactId, new_steward: PlayerId) -> Result<()> {
        let artifact = self
            .artifacts
            .get_mut(id)
            .ok_or(VaultError::ArtifactNotFound)?;
        match artifact {
            Artifact::Leaf(leaf) => leaf.steward = new_steward,
            Artifact::Tree(tree) => tree.steward = new_steward,
        }
        Ok(())
    }

    fn update_status(&mut self, id: &ArtifactId, status: ArtifactStatus) -> Result<()> {
        let artifact = self
            .artifacts
            .get_mut(id)
            .ok_or(VaultError::ArtifactNotFound)?;
        match artifact {
            Artifact::Leaf(leaf) => leaf.status = status,
            Artifact::Tree(tree) => tree.status = status,
        }
        Ok(())
    }

    fn add_grant(&mut self, id: &ArtifactId, grant: AccessGrant) -> Result<()> {
        let artifact = self
            .artifacts
            .get_mut(id)
            .ok_or(VaultError::ArtifactNotFound)?;
        match artifact {
            Artifact::Leaf(leaf) => leaf.grants.push(grant),
            Artifact::Tree(tree) => tree.grants.push(grant),
        }
        Ok(())
    }

    fn remove_grant(&mut self, id: &ArtifactId, grantee: &PlayerId) -> Result<()> {
        let artifact = self
            .artifacts
            .get_mut(id)
            .ok_or(VaultError::ArtifactNotFound)?;
        match artifact {
            Artifact::Leaf(leaf) => leaf.grants.retain(|g| &g.grantee != grantee),
            Artifact::Tree(tree) => tree.grants.retain(|g| &g.grantee != grantee),
        }
        Ok(())
    }

    fn set_parent(&mut self, id: &ArtifactId, parent: Option<ArtifactId>) -> Result<()> {
        let artifact = self
            .artifacts
            .get_mut(id)
            .ok_or(VaultError::ArtifactNotFound)?;
        match artifact {
            Artifact::Leaf(leaf) => leaf.parent = parent,
            Artifact::Tree(tree) => tree.parent = parent,
        }
        Ok(())
    }

    fn list_by_status(&self, status: &ArtifactStatus) -> Result<Vec<ArtifactId>> {
        Ok(self
            .artifacts
            .values()
            .filter(|a| a.status() == status)
            .map(|a| *a.id())
            .collect())
    }

    fn accessible_by(&self, player: &PlayerId, now: i64) -> Result<Vec<ArtifactId>> {
        Ok(self
            .artifacts
            .values()
            .filter(|a| {
                a.status().is_active()
                    && a.grants().iter().any(|g| &g.grantee == player && !g.mode.is_expired(now))
            })
            .map(|a| *a.id())
            .collect())
    }

    fn delete_artifact(&mut self, id: &ArtifactId) -> Result<()> {
        self.artifacts
            .remove(id)
            .ok_or(VaultError::ArtifactNotFound)?;
        Ok(())
    }

    fn add_ref(&mut self, tree_id: &ArtifactId, child_ref: ArtifactRef) -> Result<()> {
        let artifact = self
            .artifacts
            .get_mut(tree_id)
            .ok_or(VaultError::ArtifactNotFound)?;
        match artifact {
            Artifact::Tree(tree) => {
                tree.references.push(child_ref);
                tree.references.sort_by_key(|r| r.position);
                Ok(())
            }
            _ => Err(VaultError::NotATree),
        }
    }

    fn remove_ref(&mut self, tree_id: &ArtifactId, child_id: &ArtifactId) -> Result<()> {
        let artifact = self
            .artifacts
            .get_mut(tree_id)
            .ok_or(VaultError::ArtifactNotFound)?;
        match artifact {
            Artifact::Tree(tree) => {
                tree.references.retain(|r| &r.artifact_id != child_id);
                Ok(())
            }
            _ => Err(VaultError::NotATree),
        }
    }

    fn record_stewardship_transfer(&mut self, id: &ArtifactId, record: StewardshipRecord) -> Result<()> {
        self.stewardship_history
            .entry(id.clone())
            .or_default()
            .push(record);
        Ok(())
    }

    fn steward_history(&self, id: &ArtifactId) -> Result<Vec<StewardshipRecord>> {
        Ok(self.stewardship_history.get(id).cloned().unwrap_or_default())
    }
}

// ---------------------------------------------------------------------------
// PayloadStore
// ---------------------------------------------------------------------------

/// Store and retrieve blob payloads (Leaf content). Content-addressed by BLAKE3.
pub trait PayloadStore {
    fn store_payload(&mut self, payload: &[u8]) -> Result<ArtifactId>;
    fn get_payload(&self, id: &ArtifactId) -> Result<Option<Bytes>>;
    fn has_payload(&self, id: &ArtifactId) -> bool;
    fn delete_payload(&mut self, id: &ArtifactId) -> Result<()>;
}

/// In-memory payload store backed by HashMap.
#[derive(Default)]
pub struct InMemoryPayloadStore {
    payloads: HashMap<ArtifactId, Bytes>,
}

impl InMemoryPayloadStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PayloadStore for InMemoryPayloadStore {
    fn store_payload(&mut self, payload: &[u8]) -> Result<ArtifactId> {
        let id = crate::artifact::leaf_id(payload);
        self.payloads.insert(id.clone(), Bytes::copy_from_slice(payload));
        Ok(id)
    }

    fn get_payload(&self, id: &ArtifactId) -> Result<Option<Bytes>> {
        Ok(self.payloads.get(id).cloned())
    }

    fn has_payload(&self, id: &ArtifactId) -> bool {
        self.payloads.contains_key(id)
    }

    fn delete_payload(&mut self, id: &ArtifactId) -> Result<()> {
        self.payloads
            .remove(id)
            .ok_or(VaultError::ArtifactNotFound)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AttentionStore
// ---------------------------------------------------------------------------

/// Append-only attention log storage with integrity checking.
pub trait AttentionStore {
    fn append_event(&mut self, event: AttentionSwitchEvent) -> Result<()>;
    fn events(&self, player: &PlayerId) -> Result<Vec<AttentionSwitchEvent>>;
    fn events_since(&self, player: &PlayerId, since: i64) -> Result<Vec<AttentionSwitchEvent>>;
    fn ingest_peer_log(
        &mut self,
        peer: PlayerId,
        events: Vec<AttentionSwitchEvent>,
    ) -> Result<()>;
    /// Detect if a peer's log has diverged from our replica (mutual witnessing).
    fn check_integrity(
        &self,
        peer: &PlayerId,
        their_events: &[AttentionSwitchEvent],
    ) -> IntegrityResult;
}

/// In-memory attention store backed by per-player Vec.
#[derive(Default)]
pub struct InMemoryAttentionStore {
    logs: HashMap<PlayerId, Vec<AttentionSwitchEvent>>,
}

impl InMemoryAttentionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AttentionStore for InMemoryAttentionStore {
    fn append_event(&mut self, event: AttentionSwitchEvent) -> Result<()> {
        self.logs
            .entry(event.player)
            .or_default()
            .push(event);
        Ok(())
    }

    fn events(&self, player: &PlayerId) -> Result<Vec<AttentionSwitchEvent>> {
        Ok(self.logs.get(player).cloned().unwrap_or_default())
    }

    fn events_since(&self, player: &PlayerId, since: i64) -> Result<Vec<AttentionSwitchEvent>> {
        Ok(self
            .logs
            .get(player)
            .map(|events| {
                events
                    .iter()
                    .filter(|e| e.timestamp >= since)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    fn ingest_peer_log(
        &mut self,
        peer: PlayerId,
        events: Vec<AttentionSwitchEvent>,
    ) -> Result<()> {
        self.logs.insert(peer, events);
        Ok(())
    }

    fn check_integrity(
        &self,
        peer: &PlayerId,
        their_events: &[AttentionSwitchEvent],
    ) -> IntegrityResult {
        let our_events = match self.logs.get(peer) {
            Some(events) => events,
            None => return IntegrityResult::NoPriorReplica,
        };

        // Compare our replica against their claimed log
        for (i, our_event) in our_events.iter().enumerate() {
            match their_events.get(i) {
                None => {
                    // They have fewer events than we do — divergence (events removed)
                    return IntegrityResult::Diverged {
                        first_mismatch_index: i,
                    };
                }
                Some(their_event) => {
                    if our_event != their_event {
                        return IntegrityResult::Diverged {
                            first_mismatch_index: i,
                        };
                    }
                }
            }
        }

        // All our events matched. Check if they have new ones.
        let new_count = their_events.len() - our_events.len();
        if new_count == 0 {
            IntegrityResult::Consistent
        } else {
            IntegrityResult::Extended {
                new_events: new_count,
            }
        }
    }
}
