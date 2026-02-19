//! Artifact Sync Registry — automatic P2P sync for shared artifacts.
//!
//! When a Tree artifact's audience grows beyond the steward (i.e., has active
//! grantees), the registry automatically creates an NInterface and joins the
//! gossip topic. When all grantees are removed, the interface is torn down.

use crate::artifact::ArtifactId;
use crate::artifact_index::HomeArtifactEntry;
use crate::error::{IndraError, Result};
use crate::member::MemberId;

use dashmap::DashMap;
use indras_core::InterfaceId;
use indras_node::IndrasNode;
use indras_sync::{ArtifactDocument, HeadTracker, ArtifactSyncPayload, RawSync};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Derive a deterministic InterfaceId from an ArtifactId.
///
/// Both peers independently compute the same interface ID for the same
/// artifact, enabling them to join the same gossip topic without coordination.
pub fn artifact_interface_id(artifact_id: &ArtifactId) -> InterfaceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"artifact-sync-v1:");
    hasher.update(artifact_id.bytes());
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Derive a deterministic key seed for an artifact's sync interface.
///
/// Used to derive the symmetric encryption key for the interface.
pub fn artifact_key_seed(artifact_id: &ArtifactId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"artifact-key-v1:");
    hasher.update(artifact_id.bytes());
    *hasher.finalize().as_bytes()
}

/// Registry tracking which artifacts have active sync interfaces.
///
/// When an artifact's audience changes (grant/revoke/recall/transfer),
/// the registry reconciles the NInterface state:
/// - Any active grantees → ensure interface exists with correct members
/// - No active grantees → tear down interface
pub struct ArtifactSyncRegistry {
    /// Reference to the underlying node for creating/managing interfaces.
    node: Arc<IndrasNode>,
    /// Our own member ID (the steward).
    self_id: MemberId,
    /// Active artifact → interface mappings.
    active: DashMap<ArtifactId, InterfaceId>,
    /// Per-artifact Automerge documents for CRDT sync.
    documents: DashMap<ArtifactId, ArtifactDocument>,
    /// Per-peer head tracking for incremental sync.
    head_tracker: RwLock<HeadTracker>,
}

impl ArtifactSyncRegistry {
    /// Create a new registry.
    pub fn new(node: Arc<IndrasNode>, self_id: MemberId) -> Self {
        Self {
            node,
            self_id,
            active: DashMap::new(),
            documents: DashMap::new(),
            head_tracker: RwLock::new(HeadTracker::new()),
        }
    }

    /// Reconcile the sync interface for an artifact based on its current grants.
    ///
    /// Call this after any grant change (grant_access, revoke_access, recall, transfer).
    /// The entry's grants are used to determine the current audience.
    pub async fn reconcile(&self, artifact_id: &ArtifactId, entry: &HomeArtifactEntry) -> Result<()> {
        let audience: Vec<MemberId> = entry
            .grants
            .iter()
            .filter(|g| !g.mode.is_expired(0))
            .map(|g| g.grantee)
            .collect();

        if audience.is_empty() {
            // No grantees — tear down interface if it exists
            self.documents.remove(artifact_id);
            if let Ok(mut tracker) = self.head_tracker.write() {
                tracker.remove_artifact(artifact_id);
            }
            self.teardown(artifact_id).await
        } else {
            // Has grantees — ensure interface exists and members are correct
            self.get_or_create_document(artifact_id, entry)?;
            self.ensure(artifact_id, &audience).await
        }
    }

    /// Ensure an interface exists for this artifact with the given audience members.
    async fn ensure(&self, artifact_id: &ArtifactId, audience: &[MemberId]) -> Result<()> {
        let interface_id = artifact_interface_id(artifact_id);

        if !self.active.contains_key(artifact_id) {
            // Create the interface with deterministic ID and key
            let key_seed = artifact_key_seed(artifact_id);
            match self
                .node
                .create_interface_with_seed(interface_id, &key_seed, Some("artifact-sync"), vec![])
                .await
            {
                Ok(_) => {
                    self.active.insert(*artifact_id, interface_id);
                    info!(
                        artifact = %artifact_id,
                        interface = %interface_id,
                        "Created sync interface for artifact"
                    );
                }
                Err(e) => {
                    // Interface may already exist (e.g., loaded from persistence)
                    // Check if it's in the node's interfaces
                    if self.node.list_interfaces().contains(&interface_id) {
                        self.active.insert(*artifact_id, interface_id);
                        debug!(
                            artifact = %artifact_id,
                            "Sync interface already exists, tracking it"
                        );
                    } else {
                        warn!(
                            artifact = %artifact_id,
                            error = %e,
                            "Failed to create sync interface"
                        );
                        return Err(IndraError::Artifact(format!(
                            "Failed to create sync interface: {}",
                            e
                        )));
                    }
                }
            }
        }

        // Reconcile members — add any audience members not yet in the interface
        for member_id in audience {
            if *member_id != self.self_id {
                if let Ok(public_key) = iroh::PublicKey::from_bytes(member_id) {
                    let peer = indras_transport::IrohIdentity::new(public_key);
                    if let Err(e) = self.node.add_member(&interface_id, peer).await {
                        debug!(
                            member = %hex::encode(&member_id[..4]),
                            error = %e,
                            "Failed to add member to artifact interface (may already be a member)"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Tear down the sync interface for an artifact.
    async fn teardown(&self, artifact_id: &ArtifactId) -> Result<()> {
        if let Some((_, interface_id)) = self.active.remove(artifact_id) {
            if let Err(e) = self.node.leave_interface(&interface_id).await {
                warn!(
                    artifact = %artifact_id,
                    interface = %interface_id,
                    error = %e,
                    "Failed to leave sync interface"
                );
            } else {
                info!(
                    artifact = %artifact_id,
                    interface = %interface_id,
                    "Torn down sync interface for artifact"
                );
            }
        }
        Ok(())
    }

    /// Get the interface ID for an artifact, if it has an active sync interface.
    pub fn get_interface_id(&self, artifact_id: &ArtifactId) -> Option<InterfaceId> {
        self.active.get(artifact_id).map(|entry| *entry.value())
    }

    /// Check if an artifact has an active sync interface.
    pub fn is_syncing(&self, artifact_id: &ArtifactId) -> bool {
        self.active.contains_key(artifact_id)
    }

    /// Get the number of actively syncing artifacts.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Get or create the Automerge document for an artifact.
    ///
    /// If the document doesn't exist, creates a new one from the entry's metadata.
    pub fn get_or_create_document(&self, artifact_id: &ArtifactId, entry: &HomeArtifactEntry) -> Result<()> {
        if !self.documents.contains_key(artifact_id) {
            let doc = ArtifactDocument::new(
                artifact_id,
                &self.self_id,
                "collection",
                entry.created_at,
            );
            self.documents.insert(*artifact_id, doc);
            debug!(artifact = %artifact_id, "Created Automerge document for artifact");
        }
        Ok(())
    }

    /// After a local mutation, prepare sync payloads for all audience members.
    ///
    /// Returns a list of (recipient, payload) pairs ready for transport dispatch.
    pub fn on_local_mutation(
        &self,
        artifact_id: &ArtifactId,
        audience: &[MemberId],
    ) -> Result<Vec<(MemberId, ArtifactSyncPayload)>> {
        let mut doc = self.documents.get_mut(artifact_id).ok_or_else(|| {
            IndraError::Artifact(format!("No document for artifact {artifact_id}"))
        })?;
        let tracker = self.head_tracker.read().map_err(|_| {
            IndraError::Artifact("Head tracker lock poisoned".to_string())
        })?;
        let payloads = RawSync::broadcast_payloads(
            &mut *doc,
            &tracker,
            artifact_id,
            audience,
            &self.self_id,
        );
        Ok(payloads)
    }

    /// Handle an incoming sync payload from a peer.
    ///
    /// Applies the changes to the local document and updates the head tracker.
    pub fn on_incoming_payload(
        &self,
        sender: &MemberId,
        payload: ArtifactSyncPayload,
    ) -> Result<()> {
        let artifact_id = payload.artifact_id;
        let mut doc = self.documents.get_mut(&artifact_id).ok_or_else(|| {
            IndraError::Artifact(format!("No document for artifact {artifact_id}"))
        })?;
        let mut tracker = self.head_tracker.write().map_err(|_| {
            IndraError::Artifact("Head tracker lock poisoned".to_string())
        })?;
        RawSync::apply_payload(&mut *doc, &mut tracker, payload, sender).map_err(|e| {
            IndraError::Artifact(format!("Failed to apply sync payload: {e}"))
        })?;
        info!(
            artifact = %artifact_id,
            sender = %hex::encode(&sender[..4]),
            "Applied incoming sync payload"
        );
        Ok(())
    }

    /// Serialize the head tracker for persistence.
    pub fn persist_tracker(&self) -> Result<Vec<u8>> {
        let tracker = self.head_tracker.read().map_err(|_| {
            IndraError::Artifact("Head tracker lock poisoned".to_string())
        })?;
        tracker.save().map_err(|e| {
            IndraError::Artifact(format!("Failed to persist tracker: {e}"))
        })
    }

    /// Load head tracker state from persisted bytes.
    pub fn load_tracker(&self, bytes: &[u8]) -> Result<()> {
        let loaded = HeadTracker::load(bytes).map_err(|e| {
            IndraError::Artifact(format!("Failed to load tracker: {e}"))
        })?;
        let mut tracker = self.head_tracker.write().map_err(|_| {
            IndraError::Artifact("Head tracker lock poisoned".to_string())
        })?;
        *tracker = loaded;
        Ok(())
    }

    /// Persist a single artifact document as bytes.
    pub fn persist_document(&self, artifact_id: &ArtifactId) -> Result<Option<Vec<u8>>> {
        match self.documents.get_mut(artifact_id) {
            Some(mut doc) => Ok(Some(doc.save())),
            None => Ok(None),
        }
    }

    /// Load a document from persisted bytes.
    pub fn load_document(&self, artifact_id: &ArtifactId, bytes: &[u8]) -> Result<()> {
        let doc = ArtifactDocument::load(bytes).map_err(|e| {
            IndraError::Artifact(format!("Failed to load document: {e}"))
        })?;
        self.documents.insert(*artifact_id, doc);
        Ok(())
    }
}

// Simple hex encoding for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifact_interface_id_deterministic() {
        let id = ArtifactId::Doc([0xAB; 32]);
        let iface1 = artifact_interface_id(&id);
        let iface2 = artifact_interface_id(&id);
        assert_eq!(iface1, iface2);
    }

    #[test]
    fn test_artifact_interface_id_unique_per_artifact() {
        let id1 = ArtifactId::Doc([0xAB; 32]);
        let id2 = ArtifactId::Doc([0xCD; 32]);
        let iface1 = artifact_interface_id(&id1);
        let iface2 = artifact_interface_id(&id2);
        assert_ne!(iface1, iface2);
    }

    #[test]
    fn test_artifact_interface_id_blob_vs_doc_same_bytes() {
        let blob = ArtifactId::Blob([0xAB; 32]);
        let doc = ArtifactId::Doc([0xAB; 32]);
        // Same bytes → same interface ID
        // This is fine because ArtifactIds are content-addressed (Blob) or randomly
        // generated (Doc), so collisions between variants don't happen in practice.
        let iface_blob = artifact_interface_id(&blob);
        let iface_doc = artifact_interface_id(&doc);
        assert_eq!(iface_blob, iface_doc);
    }

    #[test]
    fn test_artifact_key_seed_deterministic() {
        let id = ArtifactId::Doc([0xAB; 32]);
        let seed1 = artifact_key_seed(&id);
        let seed2 = artifact_key_seed(&id);
        assert_eq!(seed1, seed2);
    }

    #[test]
    fn test_artifact_key_seed_differs_from_interface_id() {
        let id = ArtifactId::Doc([0xAB; 32]);
        let iface = artifact_interface_id(&id);
        let seed = artifact_key_seed(&id);
        assert_ne!(iface.as_bytes(), &seed);
    }
}
