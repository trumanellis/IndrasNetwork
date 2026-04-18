//! `BraidDag`: the repo-level CRDT document that holds the changeset DAG.

use super::changeset::{ChangeId, Changeset, PatchManifest};
use crate::vault::vault_file::UserId;
use indras_network::document::DocumentSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Per-peer state within the shared DAG: which changeset a peer has checked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerState {
    /// Which changeset this peer has checked out.
    pub head: ChangeId,
    /// The `PatchManifest` of `head`, so peers can materialize without
    /// traversing the DAG.
    pub head_manifest: PatchManifest,
    /// Timestamp of last head update (LWW tiebreaker, Unix millis).
    pub updated_ms: i64,
}

/// The repo-level CRDT document: a set of changesets keyed by `ChangeId`.
///
/// Merges via set-union. Because `ChangeId` is a content hash, two
/// changesets with the same id are guaranteed to have the same content.
///
/// Heads (DAG tips) are derived on demand from `parents` pointers.
/// Per-peer HEAD tracking is stored in `peer_heads` with LWW merge per peer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BraidDag {
    /// All known changesets, indexed by content id.
    pub changesets: HashMap<ChangeId, Changeset>,
    /// Per-peer HEAD tracking: each peer publishes which changeset they
    /// have checked out. Merged via LWW per peer (highest `updated_ms` wins).
    #[serde(default)]
    pub peer_heads: HashMap<UserId, PeerState>,
}

impl BraidDag {
    /// Create an empty DAG.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a changeset into the DAG.
    ///
    /// If a changeset with this id already exists, it is left untouched
    /// (changesets are immutable by content-addressing).
    pub fn insert(&mut self, cs: Changeset) {
        // TODO(pq-verify): verify cs.signature against author's PQPublicIdentity
        // before accepting. Requires a peer key directory to resolve UserId → verifying key.
        self.changesets.entry(cs.id).or_insert(cs);
    }

    /// Look up a changeset by id.
    pub fn get(&self, id: &ChangeId) -> Option<&Changeset> {
        self.changesets.get(id)
    }

    /// Number of changesets in the DAG.
    pub fn len(&self) -> usize {
        self.changesets.len()
    }

    /// Whether the DAG is empty.
    pub fn is_empty(&self) -> bool {
        self.changesets.is_empty()
    }

    /// Whether the DAG contains a changeset with the given id.
    pub fn contains(&self, id: &ChangeId) -> bool {
        self.changesets.contains_key(id)
    }

    /// Return the current heads — changesets that are no-one's parent.
    pub fn heads(&self) -> HashSet<ChangeId> {
        let mut all_parents: HashSet<ChangeId> = HashSet::new();
        for cs in self.changesets.values() {
            for p in &cs.parents {
                all_parents.insert(*p);
            }
        }
        self.changesets
            .keys()
            .copied()
            .filter(|id| !all_parents.contains(id))
            .collect()
    }

    /// Update (or set) a peer's HEAD to the given changeset.
    pub fn update_peer_head(
        &mut self,
        user_id: UserId,
        head: ChangeId,
        head_manifest: PatchManifest,
    ) {
        self.peer_heads.insert(
            user_id,
            PeerState {
                head,
                head_manifest,
                updated_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
    }

    /// Look up a peer's current HEAD state.
    pub fn peer_head(&self, user_id: &UserId) -> Option<&PeerState> {
        self.peer_heads.get(user_id)
    }

    /// All peer HEAD states.
    pub fn all_peer_heads(&self) -> &HashMap<UserId, PeerState> {
        &self.peer_heads
    }

    /// Return all ancestors of `id` (exclusive) via BFS over parent pointers.
    ///
    /// Missing parents are skipped silently (the DAG may be partially
    /// populated during sync).
    pub fn ancestors(&self, id: &ChangeId) -> HashSet<ChangeId> {
        let mut seen: HashSet<ChangeId> = HashSet::new();
        let mut queue: VecDeque<ChangeId> = VecDeque::new();
        if let Some(cs) = self.changesets.get(id) {
            for p in &cs.parents {
                queue.push_back(*p);
            }
        }
        while let Some(next) = queue.pop_front() {
            if !seen.insert(next) {
                continue;
            }
            if let Some(cs) = self.changesets.get(&next) {
                for p in &cs.parents {
                    if !seen.contains(p) {
                        queue.push_back(*p);
                    }
                }
            }
        }
        seen
    }
}

impl DocumentSchema for BraidDag {
    /// Merge via HashMap union for changesets (same id = same content) and
    /// LWW per peer for `peer_heads` (highest `updated_ms` wins).
    fn merge(&mut self, remote: Self) {
        for (id, cs) in remote.changesets {
            // TODO(pq-verify): verify cs.signature before accepting remote changesets.
            self.changesets.entry(id).or_insert(cs);
        }
        for (user_id, remote_ps) in remote.peer_heads {
            match self.peer_heads.get(&user_id) {
                Some(local_ps) if local_ps.updated_ms >= remote_ps.updated_ms => {
                    // Local is newer or equal — keep it.
                }
                _ => {
                    self.peer_heads.insert(user_id, remote_ps);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::changeset::{Evidence, PatchFile, PatchManifest};
    use crate::vault::vault_file::UserId;

    fn agent(byte: u8) -> UserId {
        [byte; 32]
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

    fn patch(byte: u8) -> PatchManifest {
        PatchManifest::new(vec![PatchFile {
            path: "src/lib.rs".into(),
            hash: [byte; 32],
            size: 0,
        }])
    }

    fn mk(author: UserId, parents: Vec<ChangeId>, intent: &str, patch_byte: u8, ts: i64) -> Changeset {
        Changeset::new_unsigned(
            author,
            parents,
            intent.into(),
            patch(patch_byte),
            evidence(author),
            ts,
        )
    }

    #[test]
    fn dag_heads_single_root() {
        let mut dag = BraidDag::new();
        let root = mk(agent(1), vec![], "root", 1, 10);
        let root_id = root.id;
        dag.insert(root);

        let heads = dag.heads();
        assert_eq!(heads.len(), 1);
        assert!(heads.contains(&root_id));
    }

    #[test]
    fn dag_heads_linear_chain() {
        let mut dag = BraidDag::new();
        let a = mk(agent(1), vec![], "A", 1, 10);
        let b = mk(agent(1), vec![a.id], "B", 2, 20);
        let c = mk(agent(1), vec![b.id], "C", 3, 30);
        let c_id = c.id;
        dag.insert(a);
        dag.insert(b);
        dag.insert(c);

        let heads = dag.heads();
        assert_eq!(heads.len(), 1);
        assert!(heads.contains(&c_id));
    }

    #[test]
    fn dag_heads_concurrent_braid() {
        let mut dag = BraidDag::new();
        let r = mk(agent(1), vec![], "R", 1, 10);
        let r_id = r.id;
        let a = mk(agent(2), vec![r_id], "A", 2, 20);
        let b = mk(agent(3), vec![r_id], "B", 3, 20);
        let (a_id, b_id) = (a.id, b.id);
        dag.insert(r);
        dag.insert(a);
        dag.insert(b);

        let heads = dag.heads();
        assert_eq!(heads.len(), 2, "concurrent children produce a braid");
        assert!(heads.contains(&a_id));
        assert!(heads.contains(&b_id));

        let m = mk(agent(1), vec![a_id, b_id], "M", 4, 30);
        let m_id = m.id;
        dag.insert(m);

        let heads = dag.heads();
        assert_eq!(heads.len(), 1);
        assert!(heads.contains(&m_id));
    }

    #[test]
    fn dag_merge_set_union() {
        let a = mk(agent(1), vec![], "A", 1, 10);
        let b = mk(agent(2), vec![], "B", 2, 10);
        let (a_id, b_id) = (a.id, b.id);

        let mut left = BraidDag::new();
        left.insert(a);
        let mut right = BraidDag::new();
        right.insert(b);

        left.merge(right);

        assert_eq!(left.len(), 2);
        assert!(left.contains(&a_id));
        assert!(left.contains(&b_id));
    }

    #[test]
    fn dag_merge_idempotent() {
        let a = mk(agent(1), vec![], "A", 1, 10);
        let mut dag = BraidDag::new();
        dag.insert(a.clone());
        let snapshot = dag.clone();

        dag.merge(snapshot.clone());
        assert_eq!(dag, snapshot, "merging same doc twice is a no-op");

        dag.merge(snapshot.clone());
        assert_eq!(dag, snapshot);
    }

    #[test]
    fn dag_ancestors_walk() {
        let mut dag = BraidDag::new();
        let a = mk(agent(1), vec![], "A", 1, 10);
        let b = mk(agent(1), vec![a.id], "B", 2, 20);
        let c = mk(agent(1), vec![b.id], "C", 3, 30);
        let d = mk(agent(2), vec![b.id], "D", 4, 30);
        let (a_id, b_id, c_id, d_id) = (a.id, b.id, c.id, d.id);
        dag.insert(a);
        dag.insert(b);
        dag.insert(c);
        dag.insert(d);

        let anc_c = dag.ancestors(&c_id);
        assert_eq!(anc_c.len(), 2);
        assert!(anc_c.contains(&a_id));
        assert!(anc_c.contains(&b_id));

        let anc_d = dag.ancestors(&d_id);
        assert_eq!(anc_d.len(), 2);
        assert!(anc_d.contains(&a_id));
        assert!(anc_d.contains(&b_id));

        let anc_a = dag.ancestors(&a_id);
        assert!(anc_a.is_empty(), "root has no ancestors");
    }
}
