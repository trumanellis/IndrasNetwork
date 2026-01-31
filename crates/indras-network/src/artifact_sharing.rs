//! Revocable artifact sharing with code-enforced deletion.
//!
//! This module provides a revocable artifact sharing system where any artifact
//! shared to a realm can be recalled/unshared at any time. The system provides
//! **code-enforced guarantees** that other members' nodes also erase the shared
//! content, using both encryption-based revocation and audit logging for defense
//! in depth.
//!
//! ## Overview
//!
//! When an artifact is shared:
//! 1. A per-artifact encryption key is generated
//! 2. The artifact content is encrypted with the key
//! 3. The mapping from content hash to key is stored in the KeyRegistry
//! 4. The encrypted artifact + metadata is broadcast to the realm
//!
//! When an artifact is recalled:
//! 1. A signed `RevocationEntry` is created
//! 2. The hash→key mapping is deleted from the KeyRegistry
//! 3. An `ArtifactRecalled` event is broadcast to the realm
//! 4. A tombstone is created in chat
//! 5. Online peers immediately delete their local copy
//! 6. Offline peers delete on next sync
//!
//! ## Security Model
//!
//! Defense in depth:
//! 1. **Encryption Revocation**: Without the key, content is undecryptable
//! 2. **Deletion Commands**: Signed commands instruct nodes to delete blobs
//! 3. **Audit Trail**: Tombstones record who had access and when recall occurred
//! 4. **Compliance Logging**: Track which nodes acknowledged deletion

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Size of artifact encryption keys (ChaCha20-Poly1305).
pub const ARTIFACT_KEY_SIZE: usize = 32;

/// Per-artifact encryption key (ChaCha20-Poly1305).
pub type ArtifactKey = [u8; ARTIFACT_KEY_SIZE];

/// Unique artifact identifier (BLAKE3 hash of encrypted content).
pub type ArtifactHash = [u8; 32];

/// Status of an artifact's shareability.
///
/// **Deprecated**: Use `crate::access::ArtifactStatus` for the new shared filesystem.
/// Kept for backward compatibility with existing `Artifact` struct.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SharingStatus {
    /// Actively shared - key available for decryption.
    Shared,
    /// Recalled - key deleted, content should be purged.
    Recalled {
        /// When the artifact was recalled (tick timestamp).
        recalled_at: u64,
        /// Who recalled the artifact (member ID as hex string).
        recalled_by: String,
    },
}

impl Default for SharingStatus {
    fn default() -> Self {
        Self::Shared
    }
}

impl SharingStatus {
    /// Check if the artifact is currently shared (not recalled).
    pub fn is_shared(&self) -> bool {
        matches!(self, Self::Shared)
    }

    /// Check if the artifact has been recalled.
    pub fn is_recalled(&self) -> bool {
        matches!(self, Self::Recalled { .. })
    }
}

/// Extended artifact with sharing controls.
///
/// **Deprecated**: Use `crate::artifact_index::HomeArtifactEntry` for the new shared filesystem.
/// Kept for backward compatibility with the legacy `share_artifact_revocable` flow.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SharedArtifact {
    /// BLAKE3 hash of encrypted content.
    pub hash: ArtifactHash,
    /// Original filename.
    pub name: String,
    /// Size in bytes (of encrypted content).
    pub size: u64,
    /// MIME type of the original content.
    pub mime_type: Option<String>,
    /// Who shared this artifact (member ID as hex string).
    pub sharer: String,
    /// When it was shared (tick timestamp).
    pub shared_at: u64,
    /// Current sharing status.
    pub status: SharingStatus,
}

impl SharedArtifact {
    /// Create a new shared artifact in the Shared state.
    pub fn new(
        hash: ArtifactHash,
        name: String,
        size: u64,
        mime_type: Option<String>,
        sharer: String,
        shared_at: u64,
    ) -> Self {
        Self {
            hash,
            name,
            size,
            mime_type,
            sharer,
            shared_at,
            status: SharingStatus::Shared,
        }
    }

    /// Check if this artifact is still accessible (not recalled).
    pub fn is_accessible(&self) -> bool {
        self.status.is_shared()
    }

    /// Mark this artifact as recalled.
    ///
    /// Returns `false` if already recalled.
    pub fn recall(&mut self, recalled_at: u64, recalled_by: String) -> bool {
        if self.status.is_recalled() {
            return false;
        }
        self.status = SharingStatus::Recalled {
            recalled_at,
            recalled_by,
        };
        true
    }

    /// Get the hash as a hex string.
    pub fn hash_hex(&self) -> String {
        self.hash.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Get a short hash for display (first 8 hex chars).
    pub fn short_hash(&self) -> String {
        self.hash_hex()[..8].to_string()
    }
}

/// A revocation entry with cryptographic proof.
///
/// This is broadcast to the realm to instruct all nodes to delete
/// their copy of the artifact and remove the decryption key.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RevocationEntry {
    /// Hash of artifact being revoked.
    pub artifact_hash: ArtifactHash,
    /// Who is revoking (must be original sharer, as hex string).
    pub revoked_by: String,
    /// When revocation occurred (tick timestamp).
    pub revoked_at: u64,
    /// ML-DSA-65 signature proving authenticity.
    ///
    /// Signs: artifact_hash || revoked_by || revoked_at
    pub signature: Vec<u8>,
}

impl RevocationEntry {
    /// Create a new revocation entry (without signature - must be signed separately).
    pub fn new(artifact_hash: ArtifactHash, revoked_by: String, revoked_at: u64) -> Self {
        Self {
            artifact_hash,
            revoked_by,
            revoked_at,
            signature: Vec::new(),
        }
    }

    /// Get the data that should be signed.
    pub fn signing_data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(32 + self.revoked_by.len() + 8);
        data.extend_from_slice(&self.artifact_hash);
        data.extend_from_slice(self.revoked_by.as_bytes());
        data.extend_from_slice(&self.revoked_at.to_le_bytes());
        data
    }

    /// Set the signature.
    pub fn set_signature(&mut self, signature: Vec<u8>) {
        self.signature = signature;
    }

    /// Check if this entry has a signature.
    pub fn has_signature(&self) -> bool {
        !self.signature.is_empty()
    }

    /// Get the artifact hash as hex string.
    pub fn hash_hex(&self) -> String {
        self.artifact_hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

/// Encrypted artifact key blob.
///
/// The artifact key is encrypted with the realm's interface key
/// so only realm members can decrypt artifacts.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EncryptedArtifactKey {
    /// Nonce used for encryption (12 bytes for ChaCha20-Poly1305).
    pub nonce: [u8; 12],
    /// Encrypted key data (32 bytes + 16 bytes auth tag).
    pub ciphertext: Vec<u8>,
}

/// Registry mapping artifact hashes to encryption keys.
///
/// **Deprecated**: Use `crate::artifact_index::ArtifactIndex` for the new shared filesystem.
/// Kept for backward compatibility with the legacy revocable sharing flow.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArtifactKeyRegistry {
    /// Active keys: hash → encrypted key blob.
    pub keys: HashMap<ArtifactHash, EncryptedArtifactKey>,
    /// Revocation history for audit trail.
    pub revocations: Vec<RevocationEntry>,
    /// Metadata about shared artifacts (for tombstones and UI).
    pub artifacts: HashMap<ArtifactHash, SharedArtifact>,
}

impl ArtifactKeyRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a new artifact key and metadata.
    pub fn store(
        &mut self,
        artifact: SharedArtifact,
        encrypted_key: EncryptedArtifactKey,
    ) -> bool {
        let hash = artifact.hash;

        // Don't overwrite if already exists
        if self.keys.contains_key(&hash) {
            return false;
        }

        self.keys.insert(hash, encrypted_key);
        self.artifacts.insert(hash, artifact);
        true
    }

    /// Revoke access to an artifact.
    ///
    /// Returns `true` if the artifact was revoked, `false` if it didn't exist
    /// or was already revoked.
    pub fn revoke(&mut self, entry: RevocationEntry) -> bool {
        let hash = entry.artifact_hash;

        // Remove the key (this is the critical revocation step)
        if self.keys.remove(&hash).is_none() {
            // Key didn't exist - might already be revoked
            return false;
        }

        // Update artifact metadata to show recalled status
        if let Some(artifact) = self.artifacts.get_mut(&hash) {
            artifact.recall(entry.revoked_at, entry.revoked_by.clone());
        }

        // Record in revocation history for audit
        self.revocations.push(entry);

        true
    }

    /// Check if an artifact has been revoked.
    pub fn is_revoked(&self, hash: &ArtifactHash) -> bool {
        // Revoked if key is gone but we have metadata showing recalled status
        if self.keys.contains_key(hash) {
            return false;
        }

        // Check if it was ever known
        if let Some(artifact) = self.artifacts.get(hash) {
            return artifact.status.is_recalled();
        }

        // Unknown artifact - not revoked (never existed)
        false
    }

    /// Get the encrypted key for an artifact (None if revoked or unknown).
    pub fn get_key(&self, hash: &ArtifactHash) -> Option<&EncryptedArtifactKey> {
        self.keys.get(hash)
    }

    /// Get artifact metadata.
    pub fn get_artifact(&self, hash: &ArtifactHash) -> Option<&SharedArtifact> {
        self.artifacts.get(hash)
    }

    /// Get all active (non-revoked) artifacts.
    pub fn active_artifacts(&self) -> impl Iterator<Item = &SharedArtifact> {
        self.artifacts.values().filter(|a| a.is_accessible())
    }

    /// Get all revoked artifacts (for tombstone display).
    pub fn revoked_artifacts(&self) -> impl Iterator<Item = &SharedArtifact> {
        self.artifacts.values().filter(|a| a.status.is_recalled())
    }

    /// Get all revocation entries for audit purposes.
    pub fn revocation_history(&self) -> &[RevocationEntry] {
        &self.revocations
    }

    /// Get revocation entry for a specific artifact hash.
    pub fn get_revocation(&self, hash: &ArtifactHash) -> Option<&RevocationEntry> {
        self.revocations.iter().find(|r| &r.artifact_hash == hash)
    }

    /// Check if a member can revoke an artifact.
    ///
    /// Only the original sharer can revoke their artifacts.
    pub fn can_revoke(&self, hash: &ArtifactHash, member_id: &str) -> bool {
        if let Some(artifact) = self.artifacts.get(hash) {
            artifact.sharer == member_id && artifact.is_accessible()
        } else {
            false
        }
    }

    /// Get count of active artifacts.
    pub fn active_count(&self) -> usize {
        self.keys.len()
    }

    /// Get count of revoked artifacts.
    pub fn revoked_count(&self) -> usize {
        self.revocations.len()
    }
}

/// Tombstone entry for chat history when an artifact is recalled.
///
/// This provides minimal metadata about the recalled artifact
/// for display in chat without revealing the content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArtifactTombstone {
    /// That an artifact existed (always true for tombstones).
    pub existed: bool,
    /// When it was originally shared (tick timestamp).
    pub shared_at: u64,
    /// Who shared it (member ID as hex string).
    pub sharer: String,
    /// When it was recalled (tick timestamp).
    pub recalled_at: u64,
}

impl ArtifactTombstone {
    /// Create a new tombstone from a recalled artifact.
    pub fn from_artifact(artifact: &SharedArtifact) -> Option<Self> {
        match &artifact.status {
            SharingStatus::Recalled {
                recalled_at,
                recalled_by: _,
            } => Some(Self {
                existed: true,
                shared_at: artifact.shared_at,
                sharer: artifact.sharer.clone(),
                recalled_at: *recalled_at,
            }),
            SharingStatus::Shared => None,
        }
    }
}

/// Acknowledgment that a recall was processed by a peer.
///
/// Used for compliance logging and ensuring all peers have
/// deleted the artifact.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecallAcknowledgment {
    /// Hash of the recalled artifact.
    pub artifact_hash: ArtifactHash,
    /// Member who acknowledged (as hex string).
    pub acknowledged_by: String,
    /// When acknowledged (tick timestamp).
    pub acknowledged_at: u64,
    /// Whether the local blob was deleted.
    pub blob_deleted: bool,
    /// Whether the key was removed from registry.
    pub key_removed: bool,
}

impl RecallAcknowledgment {
    /// Create a new acknowledgment.
    pub fn new(
        artifact_hash: ArtifactHash,
        acknowledged_by: String,
        acknowledged_at: u64,
        blob_deleted: bool,
        key_removed: bool,
    ) -> Self {
        Self {
            artifact_hash,
            acknowledged_by,
            acknowledged_at,
            blob_deleted,
            key_removed,
        }
    }

    /// Check if the recall was fully processed.
    pub fn is_complete(&self) -> bool {
        self.blob_deleted && self.key_removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hash() -> ArtifactHash {
        [0x42u8; 32]
    }

    fn test_encrypted_key() -> EncryptedArtifactKey {
        EncryptedArtifactKey {
            nonce: [0u8; 12],
            ciphertext: vec![0u8; 48], // 32 key + 16 auth tag
        }
    }

    fn test_artifact() -> SharedArtifact {
        SharedArtifact::new(
            test_hash(),
            "test.pdf".to_string(),
            1024,
            Some("application/pdf".to_string()),
            "abc123".to_string(),
            100,
        )
    }

    #[test]
    fn test_sharing_status() {
        let status = SharingStatus::Shared;
        assert!(status.is_shared());
        assert!(!status.is_recalled());

        let recalled = SharingStatus::Recalled {
            recalled_at: 200,
            recalled_by: "xyz789".to_string(),
        };
        assert!(!recalled.is_shared());
        assert!(recalled.is_recalled());
    }

    #[test]
    fn test_shared_artifact_recall() {
        let mut artifact = test_artifact();
        assert!(artifact.is_accessible());

        let recalled = artifact.recall(200, "abc123".to_string());
        assert!(recalled);
        assert!(!artifact.is_accessible());

        // Can't recall twice
        let recalled_again = artifact.recall(300, "abc123".to_string());
        assert!(!recalled_again);
    }

    #[test]
    fn test_registry_store_and_get() {
        let mut registry = ArtifactKeyRegistry::new();
        let artifact = test_artifact();
        let hash = artifact.hash;
        let key = test_encrypted_key();

        assert!(registry.store(artifact, key));
        assert!(registry.get_key(&hash).is_some());
        assert!(registry.get_artifact(&hash).is_some());
        assert!(!registry.is_revoked(&hash));
        assert_eq!(registry.active_count(), 1);
    }

    #[test]
    fn test_registry_revoke() {
        let mut registry = ArtifactKeyRegistry::new();
        let artifact = test_artifact();
        let hash = artifact.hash;
        let sharer = artifact.sharer.clone();

        registry.store(artifact, test_encrypted_key());

        let entry = RevocationEntry::new(hash, sharer.clone(), 200);
        assert!(registry.revoke(entry));

        assert!(registry.is_revoked(&hash));
        assert!(registry.get_key(&hash).is_none());
        assert_eq!(registry.active_count(), 0);
        assert_eq!(registry.revoked_count(), 1);

        // Check revocation history
        let revocation = registry.get_revocation(&hash);
        assert!(revocation.is_some());
        assert_eq!(revocation.unwrap().revoked_by, sharer);
    }

    #[test]
    fn test_registry_can_revoke() {
        let mut registry = ArtifactKeyRegistry::new();
        let artifact = test_artifact();
        let hash = artifact.hash;
        let sharer = artifact.sharer.clone();

        registry.store(artifact, test_encrypted_key());

        // Owner can revoke
        assert!(registry.can_revoke(&hash, &sharer));

        // Non-owner cannot revoke
        assert!(!registry.can_revoke(&hash, "other_member"));

        // Revoke it
        registry.revoke(RevocationEntry::new(hash, sharer.clone(), 200));

        // Can't revoke already revoked
        assert!(!registry.can_revoke(&hash, &sharer));
    }

    #[test]
    fn test_revocation_entry_signing_data() {
        let entry = RevocationEntry::new(test_hash(), "abc123".to_string(), 12345);
        let data = entry.signing_data();

        // Should contain hash + member + timestamp
        assert_eq!(data.len(), 32 + 6 + 8); // hash + "abc123" + u64
    }

    #[test]
    fn test_tombstone_creation() {
        let mut artifact = test_artifact();
        artifact.recall(200, "abc123".to_string());

        let tombstone = ArtifactTombstone::from_artifact(&artifact);
        assert!(tombstone.is_some());

        let ts = tombstone.unwrap();
        assert!(ts.existed);
        assert_eq!(ts.shared_at, 100);
        assert_eq!(ts.recalled_at, 200);
        assert_eq!(ts.sharer, "abc123");
    }

    #[test]
    fn test_tombstone_not_created_for_active() {
        let artifact = test_artifact();
        let tombstone = ArtifactTombstone::from_artifact(&artifact);
        assert!(tombstone.is_none());
    }

    #[test]
    fn test_recall_acknowledgment() {
        let ack = RecallAcknowledgment::new(test_hash(), "member1".to_string(), 300, true, true);

        assert!(ack.is_complete());

        let partial = RecallAcknowledgment::new(test_hash(), "member2".to_string(), 300, true, false);
        assert!(!partial.is_complete());
    }
}
