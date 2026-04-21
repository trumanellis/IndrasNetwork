//! Integration test: three in-process peers (A, B, C) produce a braid via
//! `BraidDag::merge`, then collapse it with a 4th merge changeset.

use indras_network::document::DocumentSchema;
use indras_sync_engine::{BraidDag, ChangeId, Changeset, Evidence, PatchManifest, UserId};
use std::collections::HashSet;

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
        patch().into(),
        None,
        evidence(author),
        ts,
    )
}

#[test]
fn three_peer_concurrent_braid() {
    let agent_a = agent(0xAA);
    let agent_b = agent(0xBB);
    let agent_c = agent(0xCC);

    let cs_a = make_changeset(agent_a, vec![], "a1: peer A root", 1_000);
    let cs_b = make_changeset(agent_b, vec![], "b1: peer B root", 1_001);
    let cs_c = make_changeset(agent_c, vec![], "c1: peer C root", 1_002);
    let (id_a, id_b, id_c) = (cs_a.id, cs_b.id, cs_c.id);

    assert_ne!(id_a, id_b);
    assert_ne!(id_b, id_c);
    assert_ne!(id_a, id_c);

    let mut dag_a = BraidDag::new();
    dag_a.insert(cs_a);
    let mut dag_b = BraidDag::new();
    dag_b.insert(cs_b);
    let mut dag_c = BraidDag::new();
    dag_c.insert(cs_c);

    dag_b.merge(dag_a.clone());
    dag_c.merge(dag_b.clone());
    dag_a.merge(dag_c.clone());

    dag_b.merge(dag_a.clone());
    dag_c.merge(dag_b.clone());

    assert_eq!(dag_a.len(), 3);
    assert_eq!(dag_b.len(), 3);
    assert_eq!(dag_c.len(), 3);

    let expected_ids: HashSet<ChangeId> = [id_a, id_b, id_c].into_iter().collect();

    for (label, dag) in [("A", &dag_a), ("B", &dag_b), ("C", &dag_c)] {
        let actual: HashSet<ChangeId> = dag.changesets.keys().copied().collect();
        assert_eq!(actual, expected_ids, "peer {label}");
    }

    let heads_a = dag_a.heads();
    assert_eq!(heads_a.len(), 3);
    assert_eq!(heads_a, expected_ids);

    let cs_merge = make_changeset(agent_a, vec![id_a, id_b, id_c], "m: merge", 2_000);
    let id_merge = cs_merge.id;
    dag_a.insert(cs_merge);

    dag_b.merge(dag_a.clone());
    dag_c.merge(dag_b.clone());

    assert_eq!(dag_a.len(), 4);
    assert_eq!(dag_b.len(), 4);
    assert_eq!(dag_c.len(), 4);

    assert_eq!(dag_a.heads().len(), 1);
    assert_eq!(dag_b.heads().len(), 1);
    assert_eq!(dag_c.heads().len(), 1);
    assert!(dag_a.heads().contains(&id_merge));
    assert!(dag_b.heads().contains(&id_merge));
    assert!(dag_c.heads().contains(&id_merge));
}

#[test]
fn three_peer_dag_is_deterministic_across_peers() {
    let agent_a = agent(0xAA);
    let agent_b = agent(0xBB);
    let agent_c = agent(0xCC);

    let cs_a = make_changeset(agent_a, vec![], "a1: peer A root", 1_000);
    let cs_b = make_changeset(agent_b, vec![], "b1: peer B root", 1_001);
    let cs_c = make_changeset(agent_c, vec![], "c1: peer C root", 1_002);
    let (id_a, id_b, id_c) = (cs_a.id, cs_b.id, cs_c.id);

    let mut dag_a = BraidDag::new();
    dag_a.insert(cs_a);
    let mut dag_b = BraidDag::new();
    dag_b.insert(cs_b);
    let mut dag_c = BraidDag::new();
    dag_c.insert(cs_c);

    dag_b.merge(dag_a.clone());
    dag_c.merge(dag_b.clone());
    dag_a.merge(dag_c.clone());
    dag_b.merge(dag_a.clone());
    dag_c.merge(dag_b.clone());

    let cs_merge = make_changeset(agent_a, vec![id_a, id_b, id_c], "m: merge", 2_000);
    let id_merge = cs_merge.id;
    dag_a.insert(cs_merge);

    dag_b.merge(dag_a.clone());
    dag_c.merge(dag_b.clone());

    let expected: HashSet<ChangeId> = [id_a, id_b, id_c, id_merge].into_iter().collect();

    let ids_a: HashSet<ChangeId> = dag_a.changesets.keys().copied().collect();
    let ids_b: HashSet<ChangeId> = dag_b.changesets.keys().copied().collect();
    let ids_c: HashSet<ChangeId> = dag_c.changesets.keys().copied().collect();

    assert_eq!(ids_a, expected);
    assert_eq!(ids_b, expected);
    assert_eq!(ids_c, expected);
    assert_eq!(ids_a, ids_b);
    assert_eq!(ids_b, ids_c);
}

#[test]
fn braid_merge_changeset_has_three_parents() {
    let agent_a = agent(0xAA);
    let agent_b = agent(0xBB);
    let agent_c = agent(0xCC);

    let cs_a = make_changeset(agent_a, vec![], "a1", 1_000);
    let cs_b = make_changeset(agent_b, vec![], "b1", 1_001);
    let cs_c = make_changeset(agent_c, vec![], "c1", 1_002);
    let (id_a, id_b, id_c) = (cs_a.id, cs_b.id, cs_c.id);

    let cs_merge = make_changeset(agent_a, vec![id_a, id_b, id_c], "m: merge", 2_000);
    let id_merge = cs_merge.id;

    let mut dag = BraidDag::new();
    dag.insert(cs_a);
    dag.insert(cs_b);
    dag.insert(cs_c);
    dag.insert(cs_merge);

    let retrieved = dag.get(&id_merge).expect("merge changeset must be present");
    assert_eq!(retrieved.parents.len(), 3);

    let parent_set: HashSet<ChangeId> = retrieved.parents.iter().copied().collect();
    let expected_parents: HashSet<ChangeId> = [id_a, id_b, id_c].into_iter().collect();
    assert_eq!(parent_set, expected_parents);
}
