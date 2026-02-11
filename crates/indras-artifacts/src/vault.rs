use std::collections::BTreeMap;
use std::collections::HashMap;

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
            audience: vec![player],
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
        leaf_type: LeafType,
        now: i64,
    ) -> Result<LeafArtifact> {
        let id = self
            .payload_store
            .store_payload(payload)
            .map_err(|e| VaultError::StoreError(e.to_string()))?;
        let leaf = LeafArtifact {
            id,
            size: payload.len() as u64,
            steward: *self.player(),
            audience: vec![*self.player()],
            artifact_type: leaf_type,
            created_at: now,
        };
        self.artifact_store
            .put_artifact(&Artifact::Leaf(leaf.clone()))?;
        Ok(leaf)
    }

    /// Create a Tree artifact with given type and audience.
    pub fn place_tree(
        &mut self,
        tree_type: TreeType,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<TreeArtifact> {
        let tree = TreeArtifact {
            id: generate_tree_id(),
            steward: *self.player(),
            audience,
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

    /// Steward-only: set audience for an artifact.
    pub fn set_audience(&mut self, artifact_id: &ArtifactId, players: Vec<PlayerId>) -> Result<()> {
        self.require_steward(artifact_id)?;
        self.artifact_store.update_audience(artifact_id, players)
    }

    /// Steward-only: transfer stewardship to another player.
    pub fn transfer_stewardship(
        &mut self,
        artifact_id: &ArtifactId,
        new_steward: PlayerId,
    ) -> Result<()> {
        self.require_steward(artifact_id)?;
        self.artifact_store.update_steward(artifact_id, new_steward)
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
            Some(a) => a.audience().to_vec(),
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
}
