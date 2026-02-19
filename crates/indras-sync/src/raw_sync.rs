//! Stateless functions for producing and consuming sync payloads using
//! Automerge's `save_after` / `load_incremental`.
//!
//! [`RawSync`] sits below any transport or scheduling layer. Callers supply
//! the mutable document and a [`HeadTracker`] that remembers what each peer
//! already has; [`RawSync`] handles the delta computation and tracker update.

use automerge::ChangeHash;
use indras_artifacts::{ArtifactId, PlayerId};
use serde::{Deserialize, Serialize};

use crate::artifact_document::ArtifactDocument;
use crate::error::SyncError;
use crate::head_tracker::HeadTracker;

// ---------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------

/// A wire payload carrying one artifact's incremental changes to a peer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactSyncPayload {
    /// Which artifact these changes belong to.
    pub artifact_id: ArtifactId,
    /// The sender's heads at the moment of encoding (raw bytes for compact
    /// postcard serialization).
    pub sender_heads: Vec<[u8; 32]>,
    /// Bytes produced by `ArtifactDocument::save_after`.  May be empty when
    /// the recipient is already up-to-date.
    pub changes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// RawSync
// ---------------------------------------------------------------------------

/// Stateless helpers for the `save_after` / `load_incremental` sync pattern.
pub struct RawSync;

impl RawSync {
    /// Build a sync payload addressed to one recipient.
    ///
    /// `tracker.get(artifact_id, recipient)` is used to find the heads that
    /// the recipient is known to already have.  An unknown recipient (empty
    /// slice) triggers a full export.
    pub fn prepare_payload(
        doc: &mut ArtifactDocument,
        tracker: &HeadTracker,
        artifact_id: &ArtifactId,
        recipient: &PlayerId,
    ) -> ArtifactSyncPayload {
        let known_heads = tracker.get(artifact_id, recipient);
        let changes = doc.save_after(known_heads);
        let current_heads = doc.get_heads();
        ArtifactSyncPayload {
            artifact_id: *artifact_id,
            sender_heads: current_heads.iter().map(|h| h.0).collect(),
            changes,
        }
    }

    /// Apply a received payload to a local document and record the sender's
    /// heads in the tracker.
    ///
    /// `load_incremental` is idempotent — duplicate or already-known changes
    /// are silently ignored by Automerge.
    pub fn apply_payload(
        doc: &mut ArtifactDocument,
        tracker: &mut HeadTracker,
        payload: ArtifactSyncPayload,
        sender: &PlayerId,
    ) -> Result<(), SyncError> {
        if !payload.changes.is_empty() {
            doc.load_incremental(&payload.changes)?;
        }
        let sender_heads: Vec<ChangeHash> =
            payload.sender_heads.into_iter().map(ChangeHash).collect();
        tracker.update(&payload.artifact_id, sender, sender_heads);
        Ok(())
    }

    /// Build payloads for every member of `audience`, excluding `self_id`.
    ///
    /// Returns `(recipient, payload)` pairs ready for dispatch.
    pub fn broadcast_payloads(
        doc: &mut ArtifactDocument,
        tracker: &HeadTracker,
        artifact_id: &ArtifactId,
        audience: &[PlayerId],
        self_id: &PlayerId,
    ) -> Vec<(PlayerId, ArtifactSyncPayload)> {
        audience
            .iter()
            .filter(|p| *p != self_id)
            .map(|recipient| {
                let payload = Self::prepare_payload(doc, tracker, artifact_id, recipient);
                (*recipient, payload)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_document::ArtifactDocument;
    use indras_artifacts::{ArtifactId, PlayerId};

    fn make_player(seed: u8) -> PlayerId {
        [seed; 32]
    }

    fn make_doc_id(seed: u8) -> ArtifactId {
        ArtifactId::Doc([seed; 32])
    }

    // Helper: fresh ArtifactDocument with the given artifact_id.
    fn new_doc(artifact_id: &ArtifactId, steward: &PlayerId) -> ArtifactDocument {
        ArtifactDocument::new(artifact_id, steward, "story", 1000)
    }

    // -----------------------------------------------------------------------
    // 1. prepare_payload with known heads produces a smaller (incremental) payload
    // -----------------------------------------------------------------------

    #[test]
    fn test_prepare_payload_known_heads() {
        let artifact_id = make_doc_id(1);
        let steward = make_player(1);
        let mut doc = new_doc(&artifact_id, &steward);

        // Fork gives peer B its own copy at the current base state.
        let mut doc_b = doc.fork().unwrap();
        let base_heads = doc_b.get_heads();

        // Add changes on the original.
        let child = make_doc_id(10);
        doc.append_ref(&child, 0, Some("label"));

        // Full sync (unknown peer) captures everything.
        let mut tracker_full = HeadTracker::new();
        let full_payload =
            RawSync::prepare_payload(&mut doc, &tracker_full, &artifact_id, &make_player(99));

        // Incremental sync: tracker already knows peer B has base_heads.
        let peer_b = make_player(2);
        let mut tracker_inc = HeadTracker::new();
        tracker_inc.update(&artifact_id, &peer_b, base_heads);
        let inc_payload =
            RawSync::prepare_payload(&mut doc, &tracker_inc, &artifact_id, &peer_b);

        // Incremental payload is non-empty (there are new changes) …
        assert!(!inc_payload.changes.is_empty(), "incremental must carry new changes");
        // … but smaller than the full export.
        assert!(
            inc_payload.changes.len() < full_payload.changes.len(),
            "incremental payload should be smaller than full payload"
        );
    }

    // -----------------------------------------------------------------------
    // 2. prepare_payload for unknown peer returns full content
    // -----------------------------------------------------------------------

    #[test]
    fn test_prepare_payload_unknown_peer() {
        let artifact_id = make_doc_id(2);
        let steward = make_player(1);
        let mut doc = new_doc(&artifact_id, &steward);

        let child = make_doc_id(20);
        doc.append_ref(&child, 0, None);

        let tracker = HeadTracker::new(); // peer is unknown
        let recipient = make_player(9);
        let payload = RawSync::prepare_payload(&mut doc, &tracker, &artifact_id, &recipient);

        // Applying to an empty doc shell should transfer the ref.
        let mut fresh_doc = ArtifactDocument::empty();
        let mut fresh_tracker = HeadTracker::new();
        RawSync::apply_payload(&mut fresh_doc, &mut fresh_tracker, payload, &steward).unwrap();

        let refs = fresh_doc.references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].artifact_id, child);
    }

    // -----------------------------------------------------------------------
    // 3. apply_payload updates the document and the tracker
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_payload_updates_doc_and_tracker() {
        let artifact_id = make_doc_id(3);
        let player_a = make_player(1);
        let player_b = make_player(2);

        let mut doc_a = new_doc(&artifact_id, &player_a);
        let child = make_doc_id(30);
        doc_a.append_ref(&child, 0, Some("test"));

        let tracker_a = HeadTracker::new();
        let payload =
            RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);

        let expected_sender_heads = doc_a.get_heads();

        let mut doc_b = ArtifactDocument::empty();
        let mut tracker_b = HeadTracker::new();
        RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload, &player_a).unwrap();

        // Doc B now has the ref.
        let refs = doc_b.references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].artifact_id, child);

        // Tracker B records A's heads.
        let recorded = tracker_b.get(&artifact_id, &player_a);
        let expected: Vec<ChangeHash> =
            expected_sender_heads.iter().map(|h| ChangeHash(h.0)).collect();
        assert_eq!(recorded, expected.as_slice());
    }

    // -----------------------------------------------------------------------
    // 4. apply_payload is idempotent (applying the same payload twice)
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_payload_idempotent() {
        let artifact_id = make_doc_id(4);
        let player_a = make_player(1);
        let player_b = make_player(2);

        let mut doc_a = new_doc(&artifact_id, &player_a);
        let child = make_doc_id(40);
        doc_a.append_ref(&child, 0, None);

        let tracker_a = HeadTracker::new();
        let payload =
            RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);

        let mut doc_b = ArtifactDocument::empty();
        let mut tracker_b = HeadTracker::new();

        // Apply once.
        RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload.clone(), &player_a).unwrap();
        // Apply again with the same bytes.
        RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload, &player_a).unwrap();

        // Still exactly one ref — no duplication.
        let refs = doc_b.references();
        assert_eq!(refs.len(), 1, "idempotent: no duplicate refs after double apply");
        assert_eq!(refs[0].artifact_id, child);
    }

    // -----------------------------------------------------------------------
    // 5. broadcast_payloads skips self
    // -----------------------------------------------------------------------

    #[test]
    fn test_broadcast_skips_self() {
        let artifact_id = make_doc_id(5);
        let player_a = make_player(1);
        let player_b = make_player(2);
        let player_c = make_player(3);

        let mut doc = new_doc(&artifact_id, &player_a);
        let tracker = HeadTracker::new();

        let audience = vec![player_a, player_b, player_c];
        let self_id = player_b;

        let payloads =
            RawSync::broadcast_payloads(&mut doc, &tracker, &artifact_id, &audience, &self_id);

        // A and C get payloads; B (self) is skipped.
        assert_eq!(payloads.len(), 2);
        let recipients: Vec<PlayerId> = payloads.iter().map(|(p, _)| *p).collect();
        assert!(recipients.contains(&player_a));
        assert!(recipients.contains(&player_c));
        assert!(!recipients.contains(&player_b));
    }

    // -----------------------------------------------------------------------
    // 6. A-to-B round-trip: B ends up with the same refs and heads as A
    // -----------------------------------------------------------------------

    #[test]
    fn test_a_to_b_roundtrip() {
        let artifact_id = make_doc_id(6);
        let player_a = make_player(1);
        let player_b = make_player(2);

        let mut doc_a = new_doc(&artifact_id, &player_a);
        let child1 = make_doc_id(61);
        let child2 = make_doc_id(62);
        doc_a.append_ref(&child1, 0, Some("first"));
        doc_a.append_ref(&child2, 1, Some("second"));

        let tracker_a = HeadTracker::new();
        let payload =
            RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);

        let mut doc_b = ArtifactDocument::empty();
        let mut tracker_b = HeadTracker::new();
        RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload, &player_a).unwrap();

        // Same refs.
        let refs_b = doc_b.references();
        assert_eq!(refs_b.len(), 2);
        let ids_b: Vec<ArtifactId> = refs_b.iter().map(|r| r.artifact_id).collect();
        assert!(ids_b.contains(&child1));
        assert!(ids_b.contains(&child2));

        // Same heads — B is now fully caught up with A.
        assert_eq!(doc_a.get_heads(), doc_b.get_heads());
    }

    // -----------------------------------------------------------------------
    // 7. Offline convergence: A and B both fork from base, append different
    //    refs, then exchange payloads — both converge with all refs.
    // -----------------------------------------------------------------------

    #[test]
    fn test_offline_convergence() {
        let artifact_id = make_doc_id(7);
        let steward = make_player(1);
        let player_a = make_player(2);
        let player_b = make_player(3);

        // Shared base state.
        let mut base = new_doc(&artifact_id, &steward);
        let mut doc_a = base.fork().unwrap();
        let mut doc_b = base.fork().unwrap();

        // Each side appends a distinct ref while offline.
        let child_a = make_doc_id(71);
        let child_b = make_doc_id(72);
        doc_a.append_ref(&child_a, 0, Some("from-a"));
        doc_b.append_ref(&child_b, 1, Some("from-b"));

        // Prepare payloads (neither knows the other).
        let tracker_empty = HeadTracker::new();
        let payload_a_to_b =
            RawSync::prepare_payload(&mut doc_a, &tracker_empty, &artifact_id, &player_b);
        let payload_b_to_a =
            RawSync::prepare_payload(&mut doc_b, &tracker_empty, &artifact_id, &player_a);

        // Exchange.
        let mut tracker_a = HeadTracker::new();
        let mut tracker_b = HeadTracker::new();
        RawSync::apply_payload(&mut doc_a, &mut tracker_a, payload_b_to_a, &player_b).unwrap();
        RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload_a_to_b, &player_a).unwrap();

        // Both have all refs.
        let refs_a = doc_a.references();
        let refs_b = doc_b.references();
        assert_eq!(refs_a.len(), 2, "A must converge to 2 refs");
        assert_eq!(refs_b.len(), 2, "B must converge to 2 refs");

        let ids_a: Vec<ArtifactId> = refs_a.iter().map(|r| r.artifact_id).collect();
        let ids_b: Vec<ArtifactId> = refs_b.iter().map(|r| r.artifact_id).collect();
        assert!(ids_a.contains(&child_a));
        assert!(ids_a.contains(&child_b));
        assert!(ids_b.contains(&child_a));
        assert!(ids_b.contains(&child_b));

        // Heads converge.
        assert_eq!(doc_a.get_heads(), doc_b.get_heads(), "heads must converge after exchange");
    }

    // -----------------------------------------------------------------------
    // 8. Three-peer relay: A creates, syncs to B, B relays to C.
    //    All three end up with matching heads.
    // -----------------------------------------------------------------------

    #[test]
    fn test_three_peer_relay() {
        let artifact_id = make_doc_id(8);
        let player_a = make_player(1);
        let player_b = make_player(2);
        let player_c = make_player(3);

        // A creates the document and appends a ref.
        let mut doc_a = new_doc(&artifact_id, &player_a);
        let child = make_doc_id(80);
        doc_a.append_ref(&child, 0, Some("relay-test"));

        // --- A → B ---
        let tracker_a = HeadTracker::new();
        let payload_a_to_b =
            RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);

        let mut doc_b = ArtifactDocument::empty();
        let mut tracker_b = HeadTracker::new();
        RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload_a_to_b, &player_a).unwrap();

        // B now has A's data.
        assert_eq!(doc_b.references().len(), 1);
        assert_eq!(doc_a.get_heads(), doc_b.get_heads());

        // --- B → C (relay) ---
        let payload_b_to_c =
            RawSync::prepare_payload(&mut doc_b, &tracker_b, &artifact_id, &player_c);

        let mut doc_c = ArtifactDocument::empty();
        let mut tracker_c = HeadTracker::new();
        RawSync::apply_payload(&mut doc_c, &mut tracker_c, payload_b_to_c, &player_b).unwrap();

        // C has A's data via B.
        let refs_c = doc_c.references();
        assert_eq!(refs_c.len(), 1);
        assert_eq!(refs_c[0].artifact_id, child);

        // All three have the same heads.
        let heads_a = doc_a.get_heads();
        let heads_b = doc_b.get_heads();
        let heads_c = doc_c.get_heads();
        assert_eq!(heads_a, heads_b, "A and B heads must match");
        assert_eq!(heads_b, heads_c, "B and C heads must match");
    }
}
