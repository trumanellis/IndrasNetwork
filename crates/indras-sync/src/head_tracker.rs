//! Tracks last-known Automerge heads per (ArtifactId, peer) pair.
//!
//! This is a pure data structure with no Automerge document. It persists
//! known sync state so that incremental syncs can be performed rather than
//! full syncs every time.

use std::collections::HashMap;

use automerge::ChangeHash;
use indras_artifacts::{ArtifactId, PlayerId};
use serde::{Deserialize, Serialize};

use crate::error::SyncError;

/// Tracks the last-known Automerge heads for each (ArtifactId, peer) pair.
///
/// An absent entry means a full sync is needed. A present entry contains the
/// heads that were last exchanged, enabling incremental sync.
pub struct HeadTracker {
    heads: HashMap<(ArtifactId, PlayerId), Vec<ChangeHash>>,
}

impl HeadTracker {
    /// Create a new, empty tracker.
    pub fn new() -> Self {
        Self {
            heads: HashMap::new(),
        }
    }

    /// Record the latest known heads for a (artifact, peer) pair.
    ///
    /// Overwrites any previously stored heads.
    pub fn update(&mut self, artifact_id: &ArtifactId, peer: &PlayerId, heads: Vec<ChangeHash>) {
        self.heads.insert((*artifact_id, *peer), heads);
    }

    /// Return the last-known heads for a (artifact, peer) pair.
    ///
    /// Returns an empty slice if the pair is unknown, which signals that a
    /// full sync is needed.
    pub fn get(&self, artifact_id: &ArtifactId, peer: &PlayerId) -> &[ChangeHash] {
        self.heads
            .get(&(*artifact_id, *peer))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Remove all tracked state for a peer across every artifact.
    pub fn remove_peer(&mut self, peer: &PlayerId) {
        self.heads.retain(|(_, p), _| p != peer);
    }

    /// Remove all tracked state for an artifact across every peer.
    pub fn remove_artifact(&mut self, artifact_id: &ArtifactId) {
        self.heads.retain(|(a, _), _| a != artifact_id);
    }

    /// Serialize the tracker to bytes using postcard.
    pub fn save(&self) -> Result<Vec<u8>, SyncError> {
        let entries = self
            .heads
            .iter()
            .map(|((artifact_id, peer), heads)| HeadTrackerEntry {
                artifact_id: *artifact_id,
                peer: *peer,
                heads: heads.iter().map(|h| h.0).collect(),
            })
            .collect();

        let data = HeadTrackerData { entries };
        postcard::to_allocvec(&data).map_err(|e| SyncError::Serialization(e.to_string()))
    }

    /// Deserialize a tracker from bytes produced by [`Self::save`].
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError> {
        let data: HeadTrackerData =
            postcard::from_bytes(bytes).map_err(|e| SyncError::Deserialization(e.to_string()))?;

        let heads = data
            .entries
            .into_iter()
            .map(|entry| {
                let key = (entry.artifact_id, entry.peer);
                let hashes = entry.heads.into_iter().map(ChangeHash).collect();
                (key, hashes)
            })
            .collect();

        Ok(Self { heads })
    }
}

impl Default for HeadTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct HeadTrackerData {
    entries: Vec<HeadTrackerEntry>,
}

#[derive(Serialize, Deserialize)]
struct HeadTrackerEntry {
    artifact_id: ArtifactId,
    peer: PlayerId,
    heads: Vec<[u8; 32]>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> ChangeHash {
        ChangeHash([byte; 32])
    }

    fn peer(byte: u8) -> PlayerId {
        [byte; 32]
    }

    fn artifact(byte: u8) -> ArtifactId {
        ArtifactId::Doc([byte; 32])
    }

    #[test]
    fn test_new_empty() {
        let tracker = HeadTracker::new();
        // An arbitrary query on an empty tracker returns empty.
        assert!(tracker.get(&artifact(0xAA), &peer(1)).is_empty());
    }

    #[test]
    fn test_update_and_get() {
        let mut tracker = HeadTracker::new();
        let art = artifact(0xAA);
        let p = peer(1);
        let heads = vec![hash(1), hash(2)];

        tracker.update(&art, &p, heads.clone());

        let got = tracker.get(&art, &p);
        assert_eq!(got, heads.as_slice());
    }

    #[test]
    fn test_unknown_peer_returns_empty() {
        let tracker = HeadTracker::new();
        assert!(tracker.get(&artifact(0xBB), &peer(2)).is_empty());
    }

    #[test]
    fn test_update_overwrites() {
        let mut tracker = HeadTracker::new();
        let art = artifact(0xAA);
        let p = peer(1);

        tracker.update(&art, &p, vec![hash(1)]);
        tracker.update(&art, &p, vec![hash(2), hash(3)]);

        let got = tracker.get(&art, &p);
        assert_eq!(got, &[hash(2), hash(3)]);
    }

    #[test]
    fn test_remove_peer() {
        let mut tracker = HeadTracker::new();
        let art1 = artifact(0xAA);
        let art2 = artifact(0xBB);
        let p1 = peer(1);
        let p2 = peer(2);

        tracker.update(&art1, &p1, vec![hash(1)]);
        tracker.update(&art2, &p1, vec![hash(2)]);
        tracker.update(&art1, &p2, vec![hash(3)]);

        tracker.remove_peer(&p1);

        // p1 entries gone.
        assert!(tracker.get(&art1, &p1).is_empty());
        assert!(tracker.get(&art2, &p1).is_empty());

        // p2 entry survives.
        assert_eq!(tracker.get(&art1, &p2), &[hash(3)]);
    }

    #[test]
    fn test_remove_artifact() {
        let mut tracker = HeadTracker::new();
        let art1 = artifact(0xAA);
        let art2 = artifact(0xBB);
        let p1 = peer(1);
        let p2 = peer(2);

        tracker.update(&art1, &p1, vec![hash(1)]);
        tracker.update(&art1, &p2, vec![hash(2)]);
        tracker.update(&art2, &p1, vec![hash(3)]);

        tracker.remove_artifact(&art1);

        // art1 entries gone.
        assert!(tracker.get(&art1, &p1).is_empty());
        assert!(tracker.get(&art1, &p2).is_empty());

        // art2 entry survives.
        assert_eq!(tracker.get(&art2, &p1), &[hash(3)]);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut tracker = HeadTracker::new();
        let art = artifact(0xAA);
        let p = peer(1);
        let heads = vec![hash(1), hash(2), hash(3)];

        tracker.update(&art, &p, heads.clone());

        let bytes = tracker.save().expect("save should succeed");
        let loaded = HeadTracker::load(&bytes).expect("load should succeed");

        assert_eq!(loaded.get(&art, &p), heads.as_slice());
    }

    #[test]
    fn test_multiple_artifacts_and_peers() {
        let mut tracker = HeadTracker::new();

        let arts = [artifact(0xAA), artifact(0xBB), artifact(0xCC)];
        let peers = [peer(1), peer(2), peer(3)];

        // Populate every combination.
        for (ai, art) in arts.iter().enumerate() {
            for (pi, p) in peers.iter().enumerate() {
                let h = hash((ai * 10 + pi) as u8);
                tracker.update(art, p, vec![h]);
            }
        }

        // Verify each combination independently.
        for (ai, art) in arts.iter().enumerate() {
            for (pi, p) in peers.iter().enumerate() {
                let expected = hash((ai * 10 + pi) as u8);
                assert_eq!(tracker.get(art, p), &[expected]);
            }
        }

        // Roundtrip.
        let bytes = tracker.save().expect("save");
        let loaded = HeadTracker::load(&bytes).expect("load");

        for (ai, art) in arts.iter().enumerate() {
            for (pi, p) in peers.iter().enumerate() {
                let expected = hash((ai * 10 + pi) as u8);
                assert_eq!(loaded.get(art, p), &[expected]);
            }
        }
    }
}
