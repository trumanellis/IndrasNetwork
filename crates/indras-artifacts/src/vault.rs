use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::access::{AccessGrant, AccessMode, ArtifactStatus};
use crate::artifact::*;
use crate::attention::{compute_heat, AttentionLog, AttentionSwitchEvent, AttentionValue};
use crate::error::VaultError;
use crate::peering::{PeerEntry, PeerRegistry};
use crate::store::{
    ArtifactStore, AttentionStore, InMemoryArtifactStore, InMemoryAttentionStore,
    InMemoryPayloadStore, IntegrityResult, PayloadStore,
};

type Result<T> = std::result::Result<T, VaultError>;

/// The player's root artifact and local operation hub.
///
/// Generic over storage backends. Use `Vault::in_memory()` for testing.
pub struct Vault<A: ArtifactStore, P: PayloadStore, T: AttentionStore> {
    /// The Vault's own Tree Artifact (root of the player's fractal tree).
    pub root: TreeArtifact,
    /// Artifact metadata storage.
    artifact_store: A,
    /// Blob payload storage (content-addressed, lazily loaded).
    payload_store: P,
    /// Player's own attention log (append-only).
    attention_log: AttentionLog<T>,
    /// Mutual peer registry.
    peer_registry: PeerRegistry,
    /// Peer attention log replicas (read-only, ingested from peers).
    peer_attention: HashMap<PlayerId, Vec<AttentionSwitchEvent>>,
}

impl Vault<InMemoryArtifactStore, InMemoryPayloadStore, InMemoryAttentionStore> {
    /// Create a Vault with all in-memory storage for testing and standalone use.
    pub fn in_memory(player: PlayerId, now: i64) -> Result<Self> {
        let root_id = generate_tree_id();
        let root = TreeArtifact {
            id: root_id,
            steward: player,
            grants: vec![AccessGrant {
                grantee: player,
                mode: AccessMode::Permanent,
                granted_at: now,
                granted_by: player,
            }],
            status: ArtifactStatus::Active,
            parent: None,
            provenance: None,
            references: Vec::new(),
            metadata: BTreeMap::new(),
            artifact_type: TreeType::Vault,
            created_at: now,
        };

        let mut artifact_store = InMemoryArtifactStore::new();
        artifact_store.put_artifact(&Artifact::Tree(root.clone()))?;

        let attention_store = InMemoryAttentionStore::new();
        let attention_log = AttentionLog::new(player, attention_store);

        Ok(Self {
            root,
            artifact_store,
            payload_store: InMemoryPayloadStore::new(),
            attention_log,
            peer_registry: PeerRegistry::new(player),
            peer_attention: HashMap::new(),
        })
    }
}

impl<A: ArtifactStore, P: PayloadStore, T: AttentionStore> Vault<A, P, T> {
    /// The player who owns this Vault.
    pub fn player(&self) -> &PlayerId {
        &self.root.steward
    }

    // -----------------------------------------------------------------------
    // Artifact operations
    // -----------------------------------------------------------------------

    /// Create a Leaf artifact from payload. Hashes content, stores the blob.
    /// Does NOT auto-add to root tree — caller decides where to compose it.
    pub fn place_leaf(
        &mut self,
        payload: &[u8],
        name: String,
        mime_type: Option<String>,
        leaf_type: LeafType,
        now: i64,
    ) -> Result<LeafArtifact> {
        let id = self
            .payload_store
            .store_payload(payload)
            .map_err(|e| VaultError::StoreError(e.to_string()))?;
        let player = *self.player();
        let leaf = LeafArtifact {
            id,
            name,
            size: payload.len() as u64,
            mime_type,
            steward: player,
            grants: vec![AccessGrant {
                grantee: player,
                mode: AccessMode::Permanent,
                granted_at: now,
                granted_by: player,
            }],
            status: ArtifactStatus::Active,
            parent: None,
            provenance: None,
            artifact_type: leaf_type,
            created_at: now,
            blessing_history: Vec::new(),
        };
        self.artifact_store
            .put_artifact(&Artifact::Leaf(leaf.clone()))?;
        Ok(leaf)
    }

    /// Create a Tree artifact with given type and audience.
    ///
    /// The audience list is converted to Permanent grants for each player.
    pub fn place_tree(
        &mut self,
        tree_type: TreeType,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<TreeArtifact> {
        let player = *self.player();
        let grants = audience_to_grants(&audience, now, player);
        let tree = TreeArtifact {
            id: generate_tree_id(),
            steward: player,
            grants,
            status: ArtifactStatus::Active,
            parent: None,
            provenance: None,
            references: Vec::new(),
            metadata: BTreeMap::new(),
            artifact_type: tree_type,
            created_at: now,
        };
        self.artifact_store
            .put_artifact(&Artifact::Tree(tree.clone()))?;
        Ok(tree)
    }

    /// Create a Tree artifact with a specific ID (for deterministic IDs like DM stories).
    pub fn place_tree_with_id(
        &mut self,
        id: ArtifactId,
        tree_type: TreeType,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<TreeArtifact> {
        let player = *self.player();
        let grants = audience_to_grants(&audience, now, player);
        let tree = TreeArtifact {
            id,
            steward: player,
            grants,
            status: ArtifactStatus::Active,
            parent: None,
            provenance: None,
            references: Vec::new(),
            metadata: BTreeMap::new(),
            artifact_type: tree_type,
            created_at: now,
        };
        self.artifact_store
            .put_artifact(&Artifact::Tree(tree.clone()))?;
        Ok(tree)
    }

    /// Get an artifact by ID.
    pub fn get_artifact(&self, id: &ArtifactId) -> Result<Option<Artifact>> {
        self.artifact_store.get_artifact(id)
    }

    /// Get blob payload (returns None if not yet fetched / lazy loaded).
    pub fn get_payload(&self, id: &ArtifactId) -> Result<Option<bytes::Bytes>> {
        self.payload_store.get_payload(id)
    }

    /// Cache a fetched payload locally.
    pub fn store_payload(&mut self, payload: &[u8]) -> Result<ArtifactId> {
        self.payload_store.store_payload(payload)
    }

    /// Check if payload is locally available.
    pub fn has_payload(&self, id: &ArtifactId) -> bool {
        self.payload_store.has_payload(id)
    }

    /// Steward-only: add a child reference to a Tree artifact.
    pub fn compose(
        &mut self,
        tree_id: &ArtifactId,
        child_id: ArtifactId,
        position: u64,
        label: Option<String>,
    ) -> Result<()> {
        self.require_steward(tree_id)?;
        self.require_tree(tree_id)?;
        let child_ref = ArtifactRef {
            artifact_id: child_id,
            position,
            label,
        };
        self.artifact_store.add_ref(tree_id, child_ref)
    }

    /// Steward-only: remove a child reference from a Tree artifact.
    pub fn remove_ref(&mut self, tree_id: &ArtifactId, child_id: &ArtifactId) -> Result<()> {
        self.require_steward(tree_id)?;
        self.require_tree(tree_id)?;
        self.artifact_store.remove_ref(tree_id, child_id)
    }

    /// Steward-only: grant access to an artifact.
    pub fn grant_access(
        &mut self,
        artifact_id: &ArtifactId,
        grantee: PlayerId,
        mode: AccessMode,
        now: i64,
    ) -> Result<()> {
        self.require_steward(artifact_id)?;
        self.require_active(artifact_id)?;
        // Check for existing grant
        let artifact = self
            .artifact_store
            .get_artifact(artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        if artifact.grants().iter().any(|g| g.grantee == grantee && !g.mode.is_expired(now)) {
            return Err(VaultError::AlreadyGranted);
        }
        let grant = AccessGrant {
            grantee,
            mode,
            granted_at: now,
            granted_by: *self.player(),
        };
        self.artifact_store.add_grant(artifact_id, grant)
    }

    /// Steward-only: revoke a grantee's access to an artifact.
    pub fn revoke_access(
        &mut self,
        artifact_id: &ArtifactId,
        grantee: &PlayerId,
    ) -> Result<()> {
        self.require_steward(artifact_id)?;
        self.require_active(artifact_id)?;
        let artifact = self
            .artifact_store
            .get_artifact(artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let grant = artifact.grants().iter().find(|g| &g.grantee == grantee);
        match grant {
            Some(g) if matches!(g.mode, AccessMode::Permanent) => Err(VaultError::CannotRevoke),
            Some(_) => self.artifact_store.remove_grant(artifact_id, grantee),
            None => Err(VaultError::ArtifactNotFound),
        }
    }

    /// Steward-only: recall an artifact (set status to Recalled).
    pub fn recall(&mut self, artifact_id: &ArtifactId, now: i64) -> Result<()> {
        self.require_steward(artifact_id)?;
        self.require_active(artifact_id)?;
        self.artifact_store
            .update_status(artifact_id, ArtifactStatus::Recalled { recalled_at: now })
    }

    /// Steward-only: transfer stewardship to another player.
    pub fn transfer_stewardship(
        &mut self,
        artifact_id: &ArtifactId,
        new_steward: PlayerId,
        now: i64,
    ) -> Result<()> {
        self.require_steward(artifact_id)?;
        self.require_active(artifact_id)?;
        let old_steward = *self
            .artifact_store
            .get_artifact(artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?
            .steward();
        let record = crate::artifact::StewardshipRecord {
            from: old_steward,
            to: new_steward,
            timestamp: now,
        };
        self.artifact_store
            .record_stewardship_transfer(artifact_id, record)?;
        self.artifact_store.update_steward(artifact_id, new_steward)?;
        self.artifact_store.update_status(
            artifact_id,
            ArtifactStatus::Transferred {
                to: new_steward,
                transferred_at: now,
            },
        )
    }

    /// Create an Inbox tree artifact owned by this player.
    pub fn create_inbox(&mut self, now: i64) -> Result<TreeArtifact> {
        self.place_tree(TreeType::Inbox, vec![*self.player()], now)
    }

    /// Add a connection request to an inbox.
    pub fn add_connection_request(
        &mut self,
        inbox_id: &ArtifactId,
        from_player: PlayerId,
        artifact_id: ArtifactId,
        now: i64,
    ) -> Result<()> {
        self.require_steward(inbox_id)?;
        self.require_tree(inbox_id)?;

        let label = format!(
            "connection-request:{}",
            from_player.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );

        // Get next position
        let tree_artifact = self
            .artifact_store
            .get_artifact(inbox_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        let tree = tree_artifact.as_tree().ok_or(VaultError::NotATree)?;
        let next_pos = tree.references.len() as u64;

        let child_ref = ArtifactRef {
            artifact_id,
            position: next_pos,
            label: Some(label),
        };
        self.artifact_store.add_ref(inbox_id, child_ref)?;
        // Record attention on inbox activity
        let _ = self.navigate_to(inbox_id.clone(), now);
        Ok(())
    }

    /// Get the stewardship transfer history for an artifact.
    pub fn steward_history(
        &self,
        artifact_id: &ArtifactId,
    ) -> Result<Vec<crate::artifact::StewardshipRecord>> {
        self.artifact_store.steward_history(artifact_id)
    }

    // -----------------------------------------------------------------------
    // Navigation / Attention (unified)
    // -----------------------------------------------------------------------

    /// Navigate to an artifact. This IS the attention event.
    pub fn navigate_to(&mut self, artifact_id: ArtifactId, now: i64) -> Result<()> {
        self.attention_log.navigate_to(artifact_id, now)
    }

    /// Navigate back to parent (zoom out).
    pub fn navigate_back(&mut self, parent_id: ArtifactId, now: i64) -> Result<()> {
        self.attention_log.navigate_back(parent_id, now)
    }

    /// What the player is currently attending to.
    pub fn current_focus(&self) -> Option<&ArtifactId> {
        self.attention_log.current_focus()
    }

    /// Compute heat for an artifact (0.0–1.0, perspectival).
    pub fn heat(&self, artifact_id: &ArtifactId, now: i64) -> Result<f32> {
        Ok(self.attention_value(artifact_id, now)?.heat)
    }

    /// Full attention value computation for an artifact.
    pub fn attention_value(&self, artifact_id: &ArtifactId, now: i64) -> Result<AttentionValue> {
        let audience = match self.artifact_store.get_artifact(artifact_id)? {
            Some(a) => a.audience(now),
            None => return Err(VaultError::ArtifactNotFound),
        };

        let peer_logs: Vec<(PlayerId, Vec<AttentionSwitchEvent>)> = self
            .peer_attention
            .iter()
            .map(|(id, events)| (*id, events.clone()))
            .collect();

        let refs: Vec<(PlayerId, &[AttentionSwitchEvent])> = peer_logs
            .iter()
            .map(|(id, events)| (*id, events.as_slice()))
            .collect();

        Ok(compute_heat(artifact_id, &refs, &audience, now))
    }

    /// Get the player's own attention events.
    pub fn attention_events(&self) -> Result<Vec<AttentionSwitchEvent>> {
        self.attention_log.events()
    }

    // -----------------------------------------------------------------------
    // Peering
    // -----------------------------------------------------------------------

    /// Add a mutual peer.
    pub fn peer(
        &mut self,
        peer_id: PlayerId,
        display_name: Option<String>,
        now: i64,
    ) -> Result<()> {
        self.peer_registry.add_peer(peer_id, display_name, now)
    }

    /// Remove a mutual peer.
    pub fn unpeer(&mut self, peer_id: &PlayerId) -> Result<()> {
        self.peer_registry.remove_peer(peer_id)?;
        self.peer_attention.remove(peer_id);
        Ok(())
    }

    /// List peers.
    pub fn peers(&self) -> &[PeerEntry] {
        self.peer_registry.peers()
    }

    /// Ingest a peer's attention log snapshot (read-only replica).
    pub fn ingest_peer_log(
        &mut self,
        peer_id: PlayerId,
        events: Vec<AttentionSwitchEvent>,
    ) -> Result<()> {
        if !self.peer_registry.is_peer(&peer_id) {
            return Err(VaultError::NotPeered);
        }
        self.peer_attention.insert(peer_id, events);
        Ok(())
    }

    /// Check integrity of a peer's log (mutual witnessing).
    pub fn check_peer_integrity(
        &self,
        peer: &PlayerId,
        their_events: &[AttentionSwitchEvent],
    ) -> IntegrityResult {
        self.attention_log.check_peer_integrity(peer, their_events)
    }

    // -----------------------------------------------------------------------
    // Access to stores (for advanced use / Story / Exchange)
    // -----------------------------------------------------------------------

    pub fn artifact_store(&self) -> &A {
        &self.artifact_store
    }

    pub fn artifact_store_mut(&mut self) -> &mut A {
        &mut self.artifact_store
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn require_steward(&self, artifact_id: &ArtifactId) -> Result<()> {
        let artifact = self
            .artifact_store
            .get_artifact(artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        if artifact.steward() != self.player() {
            return Err(VaultError::NotSteward);
        }
        Ok(())
    }

    fn require_tree(&self, artifact_id: &ArtifactId) -> Result<()> {
        let artifact = self
            .artifact_store
            .get_artifact(artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        if !artifact.is_tree() {
            return Err(VaultError::NotATree);
        }
        Ok(())
    }

    fn require_active(&self, artifact_id: &ArtifactId) -> Result<()> {
        let artifact = self
            .artifact_store
            .get_artifact(artifact_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        if !artifact.status().is_active() {
            return Err(VaultError::NotActive);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Holonic composition
    // -----------------------------------------------------------------------

    /// Attach a child artifact under a parent. Single-parent invariant enforced.
    pub fn attach_child(
        &mut self,
        parent_id: &ArtifactId,
        child_id: &ArtifactId,
    ) -> Result<()> {
        if parent_id == child_id {
            return Err(VaultError::CycleDetected);
        }
        self.require_steward(parent_id)?;
        self.require_active(parent_id)?;

        let child = self
            .artifact_store
            .get_artifact(child_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        if !child.status().is_active() {
            return Err(VaultError::NotActive);
        }
        if child.parent().is_some() {
            return Err(VaultError::AlreadyHasParent);
        }

        // Cycle detection: walk parent's ancestor chain
        if self.is_ancestor_of(child_id, parent_id) {
            return Err(VaultError::CycleDetected);
        }

        self.artifact_store.set_parent(child_id, Some(*parent_id))?;

        // Add ref in parent tree if it's a tree
        if let Some(parent_artifact) = self.artifact_store.get_artifact(parent_id)? {
            if parent_artifact.is_tree() {
                let next_pos = parent_artifact
                    .as_tree()
                    .map(|t| t.references.len() as u64)
                    .unwrap_or(0);
                let child_ref = ArtifactRef {
                    artifact_id: *child_id,
                    position: next_pos,
                    label: None,
                };
                self.artifact_store.add_ref(parent_id, child_ref)?;
            }
        }
        Ok(())
    }

    /// Detach a child from its parent, making it top-level.
    pub fn detach_child(
        &mut self,
        parent_id: &ArtifactId,
        child_id: &ArtifactId,
    ) -> Result<()> {
        self.require_steward(parent_id)?;
        self.require_active(parent_id)?;

        let child = self
            .artifact_store
            .get_artifact(child_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        match child.parent() {
            Some(pid) if pid == parent_id => {}
            _ => return Err(VaultError::NotAChild),
        }

        self.artifact_store.set_parent(child_id, None)?;
        self.artifact_store.remove_ref(parent_id, child_id)?;
        Ok(())
    }

    /// Walk up ancestor chain from an artifact.
    pub fn ancestors(&self, artifact_id: &ArtifactId) -> Result<Vec<ArtifactId>> {
        let mut result = Vec::new();
        let mut current = self.artifact_store.get_artifact(artifact_id)?;
        while let Some(ref art) = current {
            match art.parent() {
                Some(pid) => {
                    result.push(*pid);
                    current = self.artifact_store.get_artifact(pid)?;
                }
                None => break,
            }
        }
        Ok(result)
    }

    /// Get all descendants of an artifact (depth-first).
    pub fn descendants(&self, artifact_id: &ArtifactId) -> Result<Vec<ArtifactId>> {
        let mut result = Vec::new();
        self.collect_descendants(artifact_id, &mut result)?;
        Ok(result)
    }

    fn collect_descendants(
        &self,
        id: &ArtifactId,
        result: &mut Vec<ArtifactId>,
    ) -> Result<()> {
        if let Some(artifact) = self.artifact_store.get_artifact(id)? {
            if let Some(tree) = artifact.as_tree() {
                for r in &tree.references {
                    result.push(r.artifact_id);
                    self.collect_descendants(&r.artifact_id, result)?;
                }
            }
        }
        Ok(())
    }

    fn is_ancestor_of(&self, candidate: &ArtifactId, descendant: &ArtifactId) -> bool {
        let mut current = self.artifact_store.get_artifact(descendant).ok().flatten();
        while let Some(ref art) = current {
            match art.parent() {
                Some(pid) => {
                    if pid == candidate {
                        return true;
                    }
                    current = self.artifact_store.get_artifact(pid).ok().flatten();
                }
                None => break,
            }
        }
        false
    }
}

/// Convert an audience list to Permanent grants.
fn audience_to_grants(audience: &[PlayerId], now: i64, granted_by: PlayerId) -> Vec<AccessGrant> {
    audience
        .iter()
        .map(|&player| AccessGrant {
            grantee: player,
            mode: AccessMode::Permanent,
            granted_at: now,
            granted_by,
        })
        .collect()
}
