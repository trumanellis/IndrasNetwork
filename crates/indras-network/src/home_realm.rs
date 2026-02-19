//! Home Realm - personal realm for each user with multi-device sync.
//!
//! The HomeRealm is a special personal realm that:
//! - Is deterministically derived from the user's member ID
//! - Supports multi-device sync (same user can access from multiple devices)
//! - Is eagerly created when the network initializes
//! - Contains stored artifacts (images, files, etc.)
//!
//! Unlike shared realms, the home realm is unique to each user and doesn't
//! require invite codes to access from other devices.
//!
//! Domain-specific document types (quests, notes, etc.) are managed by
//! the sync-engine layer, not here.

use crate::access::{AccessMode, ArtifactStatus};
use crate::artifact::ArtifactId;
use crate::artifact_index::{ArtifactIndex, HomeArtifactEntry};
use crate::artifact_sync::ArtifactSyncRegistry;
use crate::document::Document;
use crate::error::{IndraError, Result};
use crate::member::MemberId;
use crate::network::RealmId;
use crate::util::guess_mime_type;
use indras_core::InterfaceId;
use indras_node::IndrasNode;
use indras_storage::ContentRef;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Generate a deterministic home realm ID from a member ID.
///
/// The same member_id always produces the same realm_id, enabling
/// multi-device sync - all devices belonging to the same user will
/// access the same home realm.
///
/// # Example
///
/// ```ignore
/// let realm_id = home_realm_id(my_member_id);
/// // This realm ID will be the same on all my devices
/// ```
pub fn home_realm_id(member_id: MemberId) -> RealmId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"home-realm-v1:");
    hasher.update(&member_id);
    InterfaceId::new(*hasher.finalize().as_bytes())
}

/// Deterministic key seed for a member's home realm.
///
/// Used to derive the symmetric encryption key so that all devices
/// belonging to the same user compute the same `InterfaceKey`.
/// Anyone who knows the MemberId can derive this.
pub fn home_key_seed(member_id: &MemberId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"home-key-v1:");
    hasher.update(member_id);
    *hasher.finalize().as_bytes()
}

/// Metadata for artifacts stored in the home realm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeArtifactMetadata {
    /// Human-readable name for the artifact.
    pub name: String,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Size in bytes.
    pub size: u64,
}

/// Type alias for backward compatibility.
pub type LegacyHomeArtifactMetadata = HomeArtifactMetadata;

/// A wrapper around the home realm providing artifact management.
///
/// The HomeRealm is a personal realm unique to each user, containing:
/// - Stored artifacts (images, files, etc.)
/// - An artifact index with access control and tree composition
///
/// Domain-specific document types (quests, notes, etc.) are managed by
/// the sync-engine layer.
///
/// # Example
///
/// ```ignore
/// // Get the home realm (eagerly created on network init)
/// let home = network.home_realm().await?;
///
/// // Upload a file
/// let artifact_id = home.upload("photo.png").await?;
///
/// // Grant access to another member
/// home.grant_access(&artifact_id, peer_id, AccessMode::Read).await?;
/// ```
pub struct HomeRealm {
    /// The realm ID (deterministically derived from member ID).
    id: RealmId,
    /// Reference to the underlying node.
    node: Arc<IndrasNode>,
    /// Our own member ID.
    self_id: MemberId,
    /// Artifact sync registry for automatic P2P sync of shared artifacts.
    sync_registry: Arc<ArtifactSyncRegistry>,
}

impl HomeRealm {
    /// Create a new HomeRealm wrapper.
    ///
    /// This is called internally by IndrasNetwork during initialization.
    pub(crate) async fn new(
        id: RealmId,
        node: Arc<IndrasNode>,
        self_id: MemberId,
    ) -> Result<Self> {
        let sync_registry = Arc::new(ArtifactSyncRegistry::new(node.clone(), self_id));
        let home = Self { id, node, self_id, sync_registry };
        home.reconcile_artifact_sync().await;
        Ok(home)
    }

    /// Re-create sync interfaces for artifacts that have active grantees.
    ///
    /// Called on startup so that previously-shared artifacts resume syncing
    /// without requiring an explicit `grant_access` call.
    async fn reconcile_artifact_sync(&self) {
        let doc = match self.artifact_index().await {
            Ok(doc) => doc,
            Err(e) => {
                debug!(error = %e, "No artifact index yet, skipping sync reconciliation");
                return;
            }
        };
        let data = doc.read().await;
        let mut count = 0u32;
        for entry in data.active_artifacts() {
            let has_grantees = entry.grants.iter().any(|g| !g.mode.is_expired(0));
            if has_grantees {
                if let Err(e) = self.sync_registry.reconcile(&entry.id, entry).await {
                    warn!(artifact = %entry.id, error = %e, "Failed to reconcile artifact sync on startup");
                } else {
                    count += 1;
                }
            }
        }
        if count > 0 {
            info!(count, "Reconciled artifact sync interfaces on startup");
        }
    }

    /// Get the home realm ID.
    pub fn id(&self) -> RealmId {
        self.id
    }

    /// Get our member ID.
    pub fn member_id(&self) -> MemberId {
        self.self_id
    }

    // ============================================================
    // Artifacts
    // ============================================================

    /// Share an artifact (file) in the home realm.
    ///
    /// This stores the artifact data in blob storage and returns
    /// an artifact ID that can be used to retrieve it later.
    ///
    /// # Arguments
    ///
    /// * `data` - The raw file data
    /// * `metadata` - Metadata about the artifact
    ///
    /// # Example
    ///
    /// ```ignore
    /// let image_data = std::fs::read("photo.png")?;
    /// let artifact_id = home.share_artifact(
    ///     image_data,
    ///     HomeArtifactMetadata {
    ///         name: "photo.png".to_string(),
    ///         mime_type: Some("image/png".to_string()),
    ///         size: 12345,
    ///     },
    /// ).await?;
    /// ```
    pub async fn share_artifact(
        &self,
        data: Vec<u8>,
        metadata: HomeArtifactMetadata,
    ) -> Result<ArtifactId> {
        // Compute BLAKE3 hash for artifact ID
        let hash = blake3::hash(&data);
        let id = ArtifactId::Blob(*hash.as_bytes());

        // Store in blob storage
        let _content_ref = self
            .node
            .storage()
            .store_blob(&data)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to store blob: {}", e)))?;

        debug!(
            artifact_id = %hex::encode(&id.bytes()[..8]),
            name = %metadata.name,
            size = metadata.size,
            "Stored artifact in home realm"
        );

        Ok(id)
    }

    /// Share a file from the filesystem.
    ///
    /// Convenience method that reads the file and shares it.
    pub async fn share_file(&self, path: impl AsRef<Path>) -> Result<ArtifactId> {
        let path = path.as_ref();

        // Read the file
        let file_data = tokio::fs::read(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file: {}", e)))?;

        // Get filename
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Get file size
        let size = file_data.len() as u64;

        // Guess MIME type from extension
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(guess_mime_type);

        // Share the artifact
        let id = self
            .share_artifact(
                file_data,
                HomeArtifactMetadata {
                    name: name.clone(),
                    mime_type: mime_type.clone(),
                    size,
                },
            )
            .await?;

        Ok(id)
    }

    /// Get an artifact by ID.
    ///
    /// Retrieves the raw data for a previously shared artifact.
    pub async fn get_artifact(&self, id: &ArtifactId) -> Result<Vec<u8>> {
        // Create a content reference (we don't know the exact size, use 0 as placeholder)
        let content_ref = ContentRef::new(*id.bytes(), 0);

        let data = self
            .node
            .storage()
            .resolve_blob(&content_ref)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to fetch artifact: {}", e)))?;

        Ok(data.to_vec())
    }

    // ============================================================
    // Shared Filesystem (ArtifactIndex)
    // ============================================================

    /// Get the artifact index document for this home realm.
    ///
    /// The artifact index is the source of truth for all artifacts
    /// owned by this user.
    pub async fn artifact_index(&self) -> Result<Document<ArtifactIndex>> {
        Document::new(self.id, "artifacts".to_string(), Arc::clone(&self.node)).await
    }

    /// Upload an artifact to the home realm filesystem.
    ///
    /// Reads the file, computes its BLAKE3 hash, stores the blob,
    /// and adds an entry to the ArtifactIndex with no grants.
    /// Idempotent: if the hash already exists, returns the existing entry ID.
    pub async fn upload(&self, path: impl AsRef<Path>) -> Result<ArtifactId> {
        let path = path.as_ref();

        // Read the file
        let file_data = tokio::fs::read(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file: {}", e)))?;

        // Compute BLAKE3 hash
        let hash = blake3::hash(&file_data);
        let id = ArtifactId::Blob(*hash.as_bytes());

        // Store blob
        let _content_ref = self
            .node
            .storage()
            .store_blob(&file_data)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to store blob: {}", e)))?;

        // Get filename and metadata
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let size = file_data.len() as u64;
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(guess_mime_type);

        // Add to artifact index (idempotent)
        let doc = self.artifact_index().await?;
        doc.update(|index| {
            let entry = HomeArtifactEntry {
                id,
                name,
                mime_type,
                size,
                created_at: 0, // Will be set by caller or default tick
                encrypted_key: None,
                status: crate::access::ArtifactStatus::Active,
                grants: Vec::new(),
                provenance: None,
                location: None,
            };
            index.store(entry);
        })
        .await?;

        debug!(
            artifact_id = %hex::encode(&id.bytes()[..8]),
            "Uploaded artifact to home realm filesystem"
        );

        Ok(id)
    }

    /// Grant access to an artifact for a specific member.
    pub async fn grant_access(
        &self,
        id: &ArtifactId,
        grantee: MemberId,
        mode: AccessMode,
    ) -> Result<()> {
        let doc = self.artifact_index().await?;
        let self_id = self.self_id;
        let mut result = Ok(());
        doc.update(|index| {
            if let Err(e) = index.grant(id, grantee, mode.clone(), self_id, 0) {
                result = Err(IndraError::Artifact(format!("Grant failed: {}", e)));
            }
        })
        .await?;

        // Reconcile sync interface after grant change
        if result.is_ok() {
            let data = doc.read().await;
            if let Some(entry) = data.get(id) {
                self.sync_registry.reconcile(id, entry).await?;
            }
        }

        result
    }

    /// Ensure a DM Story artifact exists in the index and both peers have access.
    ///
    /// Creates the artifact entry if it doesn't exist, then grants Permanent
    /// access to the peer (the steward/self already has implicit access).
    /// This is idempotent — calling it multiple times has no effect.
    pub async fn ensure_dm_story(
        &self,
        artifact_id: &ArtifactId,
        peer_id: MemberId,
    ) -> Result<()> {
        let doc = self.artifact_index().await?;
        let artifact_id_copy = *artifact_id;

        // Create the entry if it doesn't exist
        doc.update(|index| {
            if index.get(&artifact_id_copy).is_none() {
                let entry = HomeArtifactEntry {
                    id: artifact_id_copy,
                    name: "DM".to_string(),
                    mime_type: None,
                    size: 0,
                    created_at: 0,
                    encrypted_key: None,
                    status: ArtifactStatus::Active,
                    grants: Vec::new(),
                    provenance: None,
                    location: None,
                };
                index.store(entry);
            }
        })
        .await?;

        // Grant peer permanent access (triggers sync registry reconcile)
        // Ignore AlreadyGranted errors
        if let Err(e) = self.grant_access(artifact_id, peer_id, AccessMode::Permanent).await {
            let err_str = format!("{}", e);
            if !err_str.contains("AlreadyGranted") {
                return Err(e);
            }
        }

        Ok(())
    }

    /// Ensure a realm's Tree artifact exists in the index with self as grantee.
    ///
    /// Creates the artifact entry if it doesn't exist, then grants self
    /// Permanent access (which triggers the sync registry to create/maintain
    /// the NInterface). This is idempotent.
    pub async fn ensure_realm_artifact(
        &self,
        artifact_id: &ArtifactId,
        name: &str,
    ) -> Result<()> {
        let doc = self.artifact_index().await?;
        let artifact_id_copy = *artifact_id;
        let name = name.to_string();
        let self_id = self.self_id;

        doc.update(|index| {
            if index.get(&artifact_id_copy).is_none() {
                let entry = HomeArtifactEntry {
                    id: artifact_id_copy,
                    name,
                    mime_type: None,
                    size: 0,
                    created_at: 0,
                    encrypted_key: None,
                    status: ArtifactStatus::Active,
                    grants: Vec::new(),
                    provenance: None,
                    location: None,
                };
                index.store(entry);
            }
        })
        .await?;

        // Grant self permanent access (triggers sync registry reconcile)
        if let Err(e) = self.grant_access(artifact_id, self_id, AccessMode::Permanent).await {
            let err_str = format!("{}", e);
            if !err_str.contains("AlreadyGranted") {
                return Err(e);
            }
        }

        Ok(())
    }

    /// Revoke a member's access to an artifact.
    pub async fn revoke_access(
        &self,
        id: &ArtifactId,
        grantee: &MemberId,
    ) -> Result<()> {
        let doc = self.artifact_index().await?;
        let grantee = *grantee;
        let mut result = Ok(());
        doc.update(|index| {
            if let Err(e) = index.revoke_access(id, &grantee) {
                result = Err(IndraError::Artifact(format!("Revoke failed: {}", e)));
            }
        })
        .await?;

        // Reconcile sync interface after revoke
        if result.is_ok() {
            let data = doc.read().await;
            if let Some(entry) = data.get(id) {
                self.sync_registry.reconcile(id, entry).await?;
            }
        }

        result
    }

    /// Recall an artifact — remove all revocable/timed grants, keep permanent.
    pub async fn recall(&self, id: &ArtifactId) -> Result<bool> {
        let doc = self.artifact_index().await?;
        let mut recalled = false;
        doc.update(|index| {
            recalled = index.recall(id, 0);
        })
        .await?;

        if recalled {
            let content_ref = indras_storage::ContentRef::new(*id.bytes(), 0);
            let _ = self.node.storage().delete_blob(&content_ref).await;

            // Reconcile sync interface after recall
            let data = doc.read().await;
            if let Some(entry) = data.get(id) {
                self.sync_registry.reconcile(id, entry).await?;
            }
        }

        Ok(recalled)
    }

    /// Transfer ownership of an artifact to another member.
    ///
    /// Returns the entry for the recipient to add to their index.
    pub async fn transfer(
        &self,
        id: &ArtifactId,
        to: MemberId,
    ) -> Result<HomeArtifactEntry> {
        let doc = self.artifact_index().await?;
        let self_id = self.self_id;
        let mut transfer_result: std::result::Result<HomeArtifactEntry, crate::access::TransferError> = Err(crate::access::TransferError::NotFound);
        doc.update(|index| {
            transfer_result = index.transfer(id, to, self_id, 0);
        })
        .await?;

        let recipient_entry = transfer_result.map_err(|e| IndraError::Artifact(format!("Transfer failed: {}", e)))?;

        // Reconcile sync interface after transfer (artifact now has different grants)
        let data = doc.read().await;
        if let Some(entry) = data.get(id) {
            self.sync_registry.reconcile(id, entry).await?;
        }

        Ok(recipient_entry)
    }

    /// Get all artifacts shared with a specific member.
    pub async fn shared_with(&self, member: &MemberId) -> Result<Vec<HomeArtifactEntry>> {
        let doc = self.artifact_index().await?;
        let data = doc.read().await;
        Ok(data.accessible_by(member, 0).into_iter().cloned().collect())
    }

    // ============================================================
    // Tree composition
    // ============================================================

    // ============================================================
    // Documents (generic)
    // ============================================================

    /// Get a typed document by name.
    ///
    /// This is the generic document accessor for HomeRealm, analogous to
    /// `Realm::document()`. Extension traits in the sync-engine layer use
    /// this to create domain-specific documents (quests, notes, etc.).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let doc = home.document::<MyDocument>("my-doc").await?;
    /// doc.update(|d| d.items.push(item)).await?;
    /// ```
    pub async fn document<T: crate::document::DocumentSchema>(
        &self,
        name: &str,
    ) -> Result<Document<T>> {
        Document::new(self.id, name.to_string(), Arc::clone(&self.node)).await
    }

    // ============================================================
    // Escape hatches
    // ============================================================

    /// Access the underlying node.
    pub fn node(&self) -> &IndrasNode {
        &self.node
    }

    /// Get a cloned Arc to the underlying node.
    ///
    /// Useful for extension traits that need ownership of the Arc
    /// to create Document instances.
    pub fn node_arc(&self) -> Arc<IndrasNode> {
        Arc::clone(&self.node)
    }
}

impl Clone for HomeRealm {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            node: Arc::clone(&self.node),
            self_id: self.self_id,
            sync_registry: Arc::clone(&self.sync_registry),
        }
    }
}

impl std::fmt::Debug for HomeRealm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HomeRealm")
            .field("id", &hex::encode(&self.id.as_bytes()[..8]))
            .field("member_id", &hex::encode(&self.self_id[..8]))
            .finish()
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

    fn test_member_id() -> MemberId {
        [1u8; 32]
    }

    fn another_member_id() -> MemberId {
        [2u8; 32]
    }

    #[test]
    fn test_home_realm_id_deterministic() {
        // Same member_id should always produce the same realm_id
        let id1 = home_realm_id(test_member_id());
        let id2 = home_realm_id(test_member_id());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_home_realm_id_unique() {
        // Different member_ids should produce different realm_ids
        let id1 = home_realm_id(test_member_id());
        let id2 = home_realm_id(another_member_id());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_home_realm_id_prefix() {
        // Verify the hash is based on the expected prefix
        let member_id = test_member_id();
        let expected_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"home-realm-v1:");
            hasher.update(&member_id);
            hasher.finalize()
        };

        let realm_id = home_realm_id(member_id);
        assert_eq!(realm_id.as_bytes(), expected_hash.as_bytes());
    }

    #[test]
    fn test_home_key_seed_deterministic() {
        let k1 = home_key_seed(&test_member_id());
        let k2 = home_key_seed(&test_member_id());
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_home_key_seed_unique_per_member() {
        let k1 = home_key_seed(&test_member_id());
        let k2 = home_key_seed(&another_member_id());
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_home_key_seed_differs_from_realm_id() {
        let seed = home_key_seed(&test_member_id());
        let realm = home_realm_id(test_member_id());
        assert_ne!(seed, *realm.as_bytes(), "Key seed must differ from realm ID");
    }

    #[test]
    fn test_artifact_metadata_serialization() {
        let metadata = HomeArtifactMetadata {
            name: "test.png".to_string(),
            mime_type: Some("image/png".to_string()),
            size: 12345,
        };

        // Test postcard serialization round-trip
        let bytes = postcard::to_allocvec(&metadata).unwrap();
        let deserialized: HomeArtifactMetadata = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(metadata.name, deserialized.name);
        assert_eq!(metadata.mime_type, deserialized.mime_type);
        assert_eq!(metadata.size, deserialized.size);
    }
}
