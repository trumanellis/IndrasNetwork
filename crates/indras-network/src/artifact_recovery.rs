//! Peer recovery protocol for artifacts after device loss.
//!
//! When a user loses their device, peers who have access to their
//! artifacts can help restore them. The protocol:
//!
//! 1. User calls `recover_from_peers(contacts)` after device loss
//! 2. For each contact, send `RecoveryRequest` via their shared realm
//! 3. Peer checks local blob store and grant records
//! 4. Peer responds with `RecoveryManifest` listing recoverable artifacts
//! 5. User selects what to recover
//! 6. Peer sends blob data + metadata
//! 7. User's node rebuilds `ArtifactIndex` entries

use crate::access::AccessMode;
use crate::artifact::ArtifactId;
use crate::member::MemberId;
use serde::{Deserialize, Serialize};

/// A request to recover artifacts from a peer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactRecoveryRequest {
    /// The artifact to recover (or all if None).
    pub artifact_id: Option<ArtifactId>,
    /// Who is requesting recovery.
    pub requester: MemberId,
}

/// Response to a recovery request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ArtifactRecoveryResponse {
    /// The artifact is available for recovery.
    Available {
        /// Artifact content hash.
        id: ArtifactId,
        /// Size in bytes.
        size: u64,
        /// Basic metadata (name, mime type).
        name: String,
        /// MIME type if known.
        mime_type: Option<String>,
    },
    /// The artifact is not available.
    Unavailable {
        /// Artifact content hash.
        id: ArtifactId,
        /// Reason it's unavailable.
        reason: String,
    },
}

/// A single recoverable artifact in a manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoverableArtifact {
    /// Content hash.
    pub id: ArtifactId,
    /// Human-readable name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// What access mode the peer has.
    pub access_mode: AccessMode,
    /// Who originally owned this artifact.
    pub owner: MemberId,
}

/// Manifest of artifacts available for recovery from a peer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecoveryManifest {
    /// List of artifacts available for recovery.
    pub artifacts: Vec<RecoverableArtifact>,
}

impl RecoveryManifest {
    /// Create a new empty manifest.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a recoverable artifact to the manifest.
    pub fn add(&mut self, artifact: RecoverableArtifact) {
        self.artifacts.push(artifact);
    }

    /// Get the number of recoverable artifacts.
    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    /// Check if the manifest is empty.
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    /// Total size of all recoverable artifacts in bytes.
    pub fn total_size(&self) -> u64 {
        self.artifacts.iter().map(|a| a.size).sum()
    }

    /// Filter to only fully recoverable artifacts (permanent access).
    pub fn fully_recoverable(&self) -> Vec<&RecoverableArtifact> {
        self.artifacts
            .iter()
            .filter(|a| matches!(a.access_mode, AccessMode::Permanent | AccessMode::Transfer))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_artifact() -> RecoverableArtifact {
        RecoverableArtifact {
            id: [0x42u8; 32],
            name: "test.pdf".to_string(),
            size: 1024,
            mime_type: Some("application/pdf".to_string()),
            access_mode: AccessMode::Permanent,
            owner: [1u8; 32],
        }
    }

    #[test]
    fn test_manifest_new() {
        let manifest = RecoveryManifest::new();
        assert!(manifest.is_empty());
        assert_eq!(manifest.len(), 0);
        assert_eq!(manifest.total_size(), 0);
    }

    #[test]
    fn test_manifest_add() {
        let mut manifest = RecoveryManifest::new();
        manifest.add(test_artifact());
        assert_eq!(manifest.len(), 1);
        assert_eq!(manifest.total_size(), 1024);
    }

    #[test]
    fn test_manifest_fully_recoverable() {
        let mut manifest = RecoveryManifest::new();

        // Permanent - fully recoverable
        manifest.add(test_artifact());

        // Revocable - not fully recoverable
        let mut revocable = test_artifact();
        revocable.access_mode = AccessMode::Revocable;
        revocable.id = [0x43u8; 32];
        manifest.add(revocable);

        // Transfer - fully recoverable
        let mut transfer = test_artifact();
        transfer.access_mode = AccessMode::Transfer;
        transfer.id = [0x44u8; 32];
        manifest.add(transfer);

        assert_eq!(manifest.len(), 3);
        assert_eq!(manifest.fully_recoverable().len(), 2);
    }

    #[test]
    fn test_recovery_request_serialization() {
        let request = ArtifactRecoveryRequest {
            artifact_id: Some([0x42u8; 32]),
            requester: [1u8; 32],
        };

        let bytes = postcard::to_allocvec(&request).unwrap();
        let deserialized: ArtifactRecoveryRequest = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(deserialized.artifact_id, Some([0x42u8; 32]));
        assert_eq!(deserialized.requester, [1u8; 32]);
    }

    #[test]
    fn test_recovery_response_serialization() {
        let response = ArtifactRecoveryResponse::Available {
            id: [0x42u8; 32],
            size: 2048,
            name: "document.pdf".to_string(),
            mime_type: Some("application/pdf".to_string()),
        };

        let bytes = postcard::to_allocvec(&response).unwrap();
        let deserialized: ArtifactRecoveryResponse = postcard::from_bytes(&bytes).unwrap();

        match deserialized {
            ArtifactRecoveryResponse::Available { id, size, name, mime_type } => {
                assert_eq!(id, [0x42u8; 32]);
                assert_eq!(size, 2048);
                assert_eq!(name, "document.pdf");
                assert_eq!(mime_type, Some("application/pdf".to_string()));
            }
            _ => panic!("Expected Available response"),
        }
    }
}
