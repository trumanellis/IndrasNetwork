//! Integration tests for the artifact sync system.
//!
//! Tests end-to-end sync scenarios with multiple peers using
//! ArtifactDocument, HeadTracker, and RawSync.

use indras_artifacts::{AccessGrant, AccessMode, ArtifactId, PlayerId};
use indras_sync::{ArtifactDocument, HeadTracker, RawSync};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn player(seed: u8) -> PlayerId {
    [seed; 32]
}

fn doc_id(seed: u8) -> ArtifactId {
    ArtifactId::Doc([seed; 32])
}

fn blob_id(seed: u8) -> ArtifactId {
    ArtifactId::Blob([seed; 32])
}

// ---------------------------------------------------------------------------
// 1. test_basic_sync
// A creates a Story, appends 3 refs. Syncs to B (B starts as empty()).
// B has all 3 refs, same heads as A.
// ---------------------------------------------------------------------------

#[test]
fn test_basic_sync() {
    let artifact_id = doc_id(1);
    let player_a = player(1);
    let player_b = player(2);

    let mut doc_a = ArtifactDocument::new(&artifact_id, &player_a, "story", 1000);
    doc_a.append_ref(&blob_id(10), 0, Some("intro"));
    doc_a.append_ref(&blob_id(11), 1, Some("chapter-1"));
    doc_a.append_ref(&blob_id(12), 2, Some("chapter-2"));

    let tracker_a = HeadTracker::new();
    let payload = RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);

    let mut doc_b = ArtifactDocument::empty();
    let mut tracker_b = HeadTracker::new();
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload, &player_a).unwrap();

    let refs_b = doc_b.references();
    assert_eq!(refs_b.len(), 3, "B should have all 3 refs");

    let ids_b: Vec<ArtifactId> = refs_b.iter().map(|r| r.artifact_id).collect();
    assert!(ids_b.contains(&blob_id(10)));
    assert!(ids_b.contains(&blob_id(11)));
    assert!(ids_b.contains(&blob_id(12)));

    assert_eq!(
        doc_a.get_heads(),
        doc_b.get_heads(),
        "A and B heads must match after full sync"
    );
}

// ---------------------------------------------------------------------------
// 2. test_offline_convergence
// A and B both fork from a common base. A appends ref_a, B appends ref_b
// (offline). Exchange payloads. Both converge with both refs. Heads match.
// ---------------------------------------------------------------------------

#[test]
fn test_offline_convergence() {
    let artifact_id = doc_id(2);
    let steward = player(1);
    let player_a = player(2);
    let player_b = player(3);

    let mut base = ArtifactDocument::new(&artifact_id, &steward, "gallery", 1000);
    let mut doc_a = base.fork().unwrap();
    let mut doc_b = base.fork().unwrap();

    // Each peer appends a distinct ref while offline.
    let ref_a = blob_id(20);
    let ref_b = blob_id(21);
    doc_a.append_ref(&ref_a, 0, Some("from-a"));
    doc_b.append_ref(&ref_b, 1, Some("from-b"));

    // Neither knows the other — fresh trackers produce full payloads.
    let tracker_empty = HeadTracker::new();
    let payload_a_to_b =
        RawSync::prepare_payload(&mut doc_a, &tracker_empty, &artifact_id, &player_b);
    let payload_b_to_a =
        RawSync::prepare_payload(&mut doc_b, &tracker_empty, &artifact_id, &player_a);

    let mut tracker_a = HeadTracker::new();
    let mut tracker_b = HeadTracker::new();
    RawSync::apply_payload(&mut doc_a, &mut tracker_a, payload_b_to_a, &player_b).unwrap();
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload_a_to_b, &player_a).unwrap();

    let refs_a = doc_a.references();
    let refs_b = doc_b.references();
    assert_eq!(refs_a.len(), 2, "A must converge to 2 refs");
    assert_eq!(refs_b.len(), 2, "B must converge to 2 refs");

    let ids_a: Vec<ArtifactId> = refs_a.iter().map(|r| r.artifact_id).collect();
    let ids_b: Vec<ArtifactId> = refs_b.iter().map(|r| r.artifact_id).collect();
    assert!(ids_a.contains(&ref_a));
    assert!(ids_a.contains(&ref_b));
    assert!(ids_b.contains(&ref_a));
    assert!(ids_b.contains(&ref_b));

    assert_eq!(
        doc_a.get_heads(),
        doc_b.get_heads(),
        "heads must converge after offline exchange"
    );
}

// ---------------------------------------------------------------------------
// 3. test_three_peer_group
// A creates doc, adds refs. A syncs to B (empty), A syncs to C (empty).
// All three have same state and same heads.
// ---------------------------------------------------------------------------

#[test]
fn test_three_peer_group() {
    let artifact_id = doc_id(3);
    let player_a = player(1);
    let player_b = player(2);
    let player_c = player(3);

    let mut doc_a = ArtifactDocument::new(&artifact_id, &player_a, "collection", 1000);
    doc_a.append_ref(&blob_id(30), 0, None);
    doc_a.append_ref(&blob_id(31), 1, None);

    let tracker_a = HeadTracker::new();

    // A -> B
    let payload_to_b =
        RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);
    let mut doc_b = ArtifactDocument::empty();
    let mut tracker_b = HeadTracker::new();
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload_to_b, &player_a).unwrap();

    // A -> C
    let payload_to_c =
        RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_c);
    let mut doc_c = ArtifactDocument::empty();
    let mut tracker_c = HeadTracker::new();
    RawSync::apply_payload(&mut doc_c, &mut tracker_c, payload_to_c, &player_a).unwrap();

    assert_eq!(doc_b.references().len(), 2, "B should have 2 refs");
    assert_eq!(doc_c.references().len(), 2, "C should have 2 refs");

    let heads_a = doc_a.get_heads();
    let heads_b = doc_b.get_heads();
    let heads_c = doc_c.get_heads();
    assert_eq!(heads_a, heads_b, "A and B heads must match");
    assert_eq!(heads_a, heads_c, "A and C heads must match");
}

// ---------------------------------------------------------------------------
// 4. test_new_member_full_sync
// A creates doc with multiple refs, grants, and metadata. Later, D joins
// (empty). A sends full sync to D. D has everything A has.
// ---------------------------------------------------------------------------

#[test]
fn test_new_member_full_sync() {
    let artifact_id = doc_id(4);
    let player_a = player(1);
    let player_d = player(4);

    let mut doc_a = ArtifactDocument::new(&artifact_id, &player_a, "vault", 1000);

    // Add refs
    doc_a.append_ref(&blob_id(40), 0, Some("item-0"));
    doc_a.append_ref(&blob_id(41), 1, Some("item-1"));
    doc_a.append_ref(&doc_id(42), 2, None);

    // Add grant
    let grantee = player(9);
    doc_a.add_grant(&AccessGrant {
        grantee,
        mode: AccessMode::Permanent,
        granted_at: 500,
        granted_by: player_a,
    });

    // Add metadata
    doc_a.set_metadata("mime", b"application/octet-stream");
    doc_a.set_metadata("version", b"1");

    // D is a new peer — tracker has no record of D.
    let tracker_a = HeadTracker::new();
    let payload = RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_d);

    let mut doc_d = ArtifactDocument::empty();
    let mut tracker_d = HeadTracker::new();
    RawSync::apply_payload(&mut doc_d, &mut tracker_d, payload, &player_a).unwrap();

    // Verify refs
    assert_eq!(doc_d.references().len(), 3);

    // Verify grant
    let grants_d = doc_d.grants();
    assert_eq!(grants_d.len(), 1);
    assert_eq!(grants_d[0].grantee, grantee);
    assert!(matches!(grants_d[0].mode, AccessMode::Permanent));

    // Verify metadata
    assert_eq!(
        doc_d.get_metadata("mime"),
        Some(b"application/octet-stream".to_vec())
    );
    assert_eq!(doc_d.get_metadata("version"), Some(b"1".to_vec()));

    assert_eq!(
        doc_a.get_heads(),
        doc_d.get_heads(),
        "D should be fully caught up with A"
    );
}

// ---------------------------------------------------------------------------
// 5. test_stale_heads_idempotent
// A creates doc. B forks. A adds ref. A syncs to B. Then A syncs to B again
// with the SAME tracker state (stale). B applies idempotently — no corruption,
// no duplicate refs.
// ---------------------------------------------------------------------------

#[test]
fn test_stale_heads_idempotent() {
    let artifact_id = doc_id(5);
    let player_a = player(1);
    let player_b = player(2);

    let mut doc_a = ArtifactDocument::new(&artifact_id, &player_a, "document", 1000);
    let mut doc_b = doc_a.fork().unwrap();
    let mut tracker_b = HeadTracker::new();

    // A adds a ref, B gets it.
    doc_a.append_ref(&blob_id(50), 0, Some("only-ref"));

    // First sync — tracker_a records that B has the base heads.
    let tracker_a = HeadTracker::new();
    let payload1 = RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload1, &player_a).unwrap();

    assert_eq!(doc_b.references().len(), 1);

    // Second sync using the *same* (stale) tracker_a — this re-sends an
    // overlapping or full payload. Must be idempotent.
    let payload2 = RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload2, &player_a).unwrap();

    let refs = doc_b.references();
    assert_eq!(
        refs.len(),
        1,
        "idempotent: B must not gain duplicate refs from stale resync"
    );
    assert_eq!(refs[0].label, Some("only-ref".to_string()));
}

// ---------------------------------------------------------------------------
// 6. test_concurrent_metadata
// A and B fork from base. A sets key1, B sets key2. Exchange payloads.
// Both have both keys.
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_metadata() {
    let artifact_id = doc_id(6);
    let steward = player(1);
    let player_a = player(2);
    let player_b = player(3);

    let mut base = ArtifactDocument::new(&artifact_id, &steward, "document", 1000);
    let mut doc_a = base.fork().unwrap();
    let mut doc_b = base.fork().unwrap();

    doc_a.set_metadata("key1", b"value-from-a");
    doc_b.set_metadata("key2", b"value-from-b");

    let tracker_empty = HeadTracker::new();
    let payload_a_to_b =
        RawSync::prepare_payload(&mut doc_a, &tracker_empty, &artifact_id, &player_b);
    let payload_b_to_a =
        RawSync::prepare_payload(&mut doc_b, &tracker_empty, &artifact_id, &player_a);

    let mut tracker_a = HeadTracker::new();
    let mut tracker_b = HeadTracker::new();
    RawSync::apply_payload(&mut doc_a, &mut tracker_a, payload_b_to_a, &player_b).unwrap();
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload_a_to_b, &player_a).unwrap();

    // Both peers have both keys.
    assert_eq!(
        doc_a.get_metadata("key1"),
        Some(b"value-from-a".to_vec()),
        "A should retain key1"
    );
    assert_eq!(
        doc_a.get_metadata("key2"),
        Some(b"value-from-b".to_vec()),
        "A should gain key2"
    );
    assert_eq!(
        doc_b.get_metadata("key1"),
        Some(b"value-from-a".to_vec()),
        "B should gain key1"
    );
    assert_eq!(
        doc_b.get_metadata("key2"),
        Some(b"value-from-b".to_vec()),
        "B should retain key2"
    );

    assert_eq!(
        doc_a.get_heads(),
        doc_b.get_heads(),
        "heads must converge after metadata exchange"
    );
}

// ---------------------------------------------------------------------------
// 7. test_grant_sync
// A creates doc, adds a grant with AccessMode::Permanent. Syncs to B (empty).
// B sees the grant with correct fields.
// ---------------------------------------------------------------------------

#[test]
fn test_grant_sync() {
    let artifact_id = doc_id(7);
    let player_a = player(1);
    let player_b = player(2);
    let grantee = player(99);

    let mut doc_a = ArtifactDocument::new(&artifact_id, &player_a, "story", 1000);
    doc_a.add_grant(&AccessGrant {
        grantee,
        mode: AccessMode::Permanent,
        granted_at: 1234,
        granted_by: player_a,
    });

    let tracker_a = HeadTracker::new();
    let payload = RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);

    let mut doc_b = ArtifactDocument::empty();
    let mut tracker_b = HeadTracker::new();
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload, &player_a).unwrap();

    let grants_b = doc_b.grants();
    assert_eq!(grants_b.len(), 1, "B should see exactly 1 grant");

    let g = &grants_b[0];
    assert_eq!(g.grantee, grantee, "grantee must round-trip");
    assert!(matches!(g.mode, AccessMode::Permanent), "mode must be Permanent");
    assert_eq!(g.granted_at, 1234, "granted_at must round-trip");
    assert_eq!(g.granted_by, player_a, "granted_by must round-trip");
}

// ---------------------------------------------------------------------------
// 8. test_head_tracker_persistence
// Create tracker, update several entries. Save to bytes, load back.
// All entries preserved.
// ---------------------------------------------------------------------------

#[test]
fn test_head_tracker_persistence() {
    let mut tracker = HeadTracker::new();

    // Populate several (artifact, peer) pairs by syncing real documents so
    // the ChangeHashes actually come from Automerge (no fake bytes needed).
    let artifacts = [doc_id(80), doc_id(81), blob_id(82)];
    let peers = [player(10), player(11), player(12)];

    for (ai, art) in artifacts.iter().enumerate() {
        for (pi, p) in peers.iter().enumerate() {
            let mut doc =
                ArtifactDocument::new(art, p, "story", (ai * 100 + pi) as i64);
            doc.append_ref(&blob_id((ai * 10 + pi) as u8), 0, None);
            let heads = doc.get_heads();
            tracker.update(art, p, heads);
        }
    }

    // Save and reload.
    let bytes = tracker.save().expect("tracker save must succeed");
    let loaded = HeadTracker::load(&bytes).expect("tracker load must succeed");

    // Every entry must survive the round-trip.
    for art in &artifacts {
        for p in &peers {
            let original = tracker.get(art, p);
            let restored = loaded.get(art, p);
            assert_eq!(
                original, restored,
                "heads for ({art:?}, {p:?}) must survive save/load"
            );
            assert!(!restored.is_empty(), "restored heads must not be empty");
        }
    }
}

// ---------------------------------------------------------------------------
// 9. test_document_persistence_with_delta
// A creates doc, saves. Adds refs. Saves again. Load from first save, apply
// incremental from second save. Verify all refs present.
// ---------------------------------------------------------------------------

#[test]
fn test_document_persistence_with_delta() {
    let artifact_id = doc_id(9);
    let player_a = player(1);

    let mut doc = ArtifactDocument::new(&artifact_id, &player_a, "inbox", 1000);

    // First save — baseline snapshot.
    let snapshot_v1 = doc.save();
    let heads_v1 = doc.get_heads();

    // Add refs after the first save.
    doc.append_ref(&blob_id(90), 0, Some("first"));
    doc.append_ref(&blob_id(91), 1, Some("second"));

    // Delta captures only what changed since v1.
    let delta = doc.save_after(&heads_v1);
    assert!(!delta.is_empty(), "delta must be non-empty after appending refs");

    // Load from the first snapshot.
    let mut doc_restored = ArtifactDocument::load(&snapshot_v1).expect("load v1 must succeed");
    assert_eq!(
        doc_restored.references().len(),
        0,
        "v1 snapshot has no refs yet"
    );

    // Apply the delta.
    let ops = doc_restored.load_incremental(&delta).expect("apply delta must succeed");
    assert!(ops > 0, "delta must apply at least one operation");

    let refs = doc_restored.references();
    assert_eq!(refs.len(), 2, "restored doc must have both refs after delta");

    let ids: Vec<ArtifactId> = refs.iter().map(|r| r.artifact_id).collect();
    assert!(ids.contains(&blob_id(90)));
    assert!(ids.contains(&blob_id(91)));

    // Final heads must match the live doc.
    assert_eq!(
        doc.get_heads(),
        doc_restored.get_heads(),
        "restored doc heads must match live doc after delta"
    );
}

// ---------------------------------------------------------------------------
// 10. test_large_ref_list
// A creates doc, appends 500 refs. Full sync to B (empty).
// B has all 500 refs.
// ---------------------------------------------------------------------------

#[test]
fn test_large_ref_list() {
    const REF_COUNT: usize = 500;

    let artifact_id = doc_id(10);
    let player_a = player(1);
    let player_b = player(2);

    let mut doc_a = ArtifactDocument::new(&artifact_id, &player_a, "collection", 1000);

    for i in 0..REF_COUNT {
        // Use a blob id derived from i; wrap the byte around 255.
        let seed = (i % 256) as u8;
        // Position encodes the full index to keep them distinct.
        doc_a.append_ref(&blob_id(seed), i as u64, Some(&format!("ref-{i}")));
    }

    assert_eq!(
        doc_a.references().len(),
        REF_COUNT,
        "doc_a must have all {REF_COUNT} refs before sync"
    );

    let tracker_a = HeadTracker::new();
    let payload = RawSync::prepare_payload(&mut doc_a, &tracker_a, &artifact_id, &player_b);

    let mut doc_b = ArtifactDocument::empty();
    let mut tracker_b = HeadTracker::new();
    RawSync::apply_payload(&mut doc_b, &mut tracker_b, payload, &player_a).unwrap();

    let refs_b = doc_b.references();
    assert_eq!(
        refs_b.len(),
        REF_COUNT,
        "B must receive all {REF_COUNT} refs after full sync"
    );

    assert_eq!(
        doc_a.get_heads(),
        doc_b.get_heads(),
        "heads must match after large sync"
    );
}
