//! Integration test: two in-process peers produce a braid via `DocumentSchema::merge`.
//!
//! This test proves that `BraidDag` rides the existing Automerge sync
//! infrastructure correctly. The sync layer calls `DocumentSchema::merge`
//! whenever it reconciles two peers' states.

use indras_network::document::DocumentSchema;
use indras_sync_engine::{BraidDag, ChangeId, Changeset, Evidence, PatchManifest, UserId};

fn agent(byte: u8) -> UserId {
    [byte; 32]
}

fn patch() -> PatchManifest {
    PatchManifest { files: vec![] }
}

fn evidence(a: UserId) -> Evidence {
    Evidence::Agent {
        compiled: true,
        tests_passed: vec!["indras-sync-engine".into()],
        lints_clean: true,
        runtime_ms: 0,
        signed_by: a,
    }
}

fn make_changeset(author: UserId, parents: Vec<ChangeId>, intent: &str, ts: i64) -> Changeset {
    Changeset::new_unsigned(
        author,
        parents,
        intent.into(),
        patch(),
        evidence(author),
        ts,
    )
}

#[test]
fn two_peer_concurrent_braid() {
    let agent_a = agent(0xAA);
    let agent_b = agent(0xBB);

    let cs_a = make_changeset(agent_a, vec![], "a1: peer A initial commit", 1_000);
    let cs_a_id = cs_a.id;
    let mut dag_a = BraidDag::new();
    dag_a.insert(cs_a);

    let cs_b = make_changeset(agent_b, vec![], "b1: peer B initial commit", 1_001);
    let cs_b_id = cs_b.id;
    let mut dag_b = BraidDag::new();
    dag_b.insert(cs_b);

    assert_ne!(cs_a_id, cs_b_id);

    dag_a.merge(dag_b.clone());
    dag_b.merge(dag_a.clone());

    assert_eq!(dag_a.len(), 2);
    assert_eq!(dag_b.len(), 2);
    assert!(dag_a.contains(&cs_a_id));
    assert!(dag_a.contains(&cs_b_id));
    assert!(dag_b.contains(&cs_a_id));
    assert!(dag_b.contains(&cs_b_id));

    let heads_a = dag_a.heads();
    let heads_b = dag_b.heads();

    assert_eq!(heads_a.len(), 2);
    assert_eq!(heads_b.len(), 2);
    assert!(heads_a.contains(&cs_a_id));
    assert!(heads_a.contains(&cs_b_id));
    assert_eq!(heads_a, heads_b);
}

#[test]
fn braid_dag_postcard_roundtrip() {
    let a = agent(0x01);
    let b = agent(0x02);

    let cs1 = make_changeset(a, vec![], "first", 100);
    let cs2 = make_changeset(b, vec![cs1.id], "second", 200);
    let (id1, id2) = (cs1.id, cs2.id);

    let mut dag = BraidDag::new();
    dag.insert(cs1);
    dag.insert(cs2);

    let bytes = postcard::to_allocvec(&dag).expect("BraidDag must serialize via postcard");
    assert!(!bytes.is_empty());

    let restored: BraidDag =
        postcard::from_bytes(&bytes).expect("BraidDag must deserialize via postcard");

    assert_eq!(restored.len(), 2);
    assert!(restored.contains(&id1));
    assert!(restored.contains(&id2));

    let heads = restored.heads();
    assert_eq!(heads.len(), 1);
    assert!(heads.contains(&id2));
}

#[test]
fn braid_dag_merge_is_idempotent() {
    let a = agent(0xCC);
    let cs = make_changeset(a, vec![], "idempotent", 500);
    let cs_id = cs.id;

    let mut dag = BraidDag::new();
    dag.insert(cs);

    let snapshot = dag.clone();

    dag.merge(snapshot.clone());
    dag.merge(snapshot.clone());

    assert_eq!(dag.len(), 1);
    assert!(dag.contains(&cs_id));
}
