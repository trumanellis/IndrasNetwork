//! `BraidDag`: the repo-level CRDT document that holds the changeset DAG.

use super::changeset::{ChangeId, Changeset, PatchManifest};
use crate::content_addr::{ContentAddr, SymlinkIndex};
use crate::vault::vault_file::UserId;
use indras_network::document::DocumentSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Per-peer state within the shared DAG: which changeset a peer has checked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerState {
    /// Which changeset this peer has checked out.
    pub head: ChangeId,
    /// The symlink index of `head`, so peers can materialize without
    /// traversing the DAG.
    pub head_index: SymlinkIndex,
    /// Timestamp of last head update (LWW tiebreaker, Unix millis).
    pub updated_ms: i64,
}

impl PeerState {
    /// Legacy accessor — returns the head index as a `PatchManifest`.
    pub fn head_manifest(&self) -> PatchManifest {
        PatchManifest::from(&self.head_index)
    }
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
        head_index: SymlinkIndex,
    ) {
        self.peer_heads.insert(
            user_id,
            PeerState {
                head,
                head_index,
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

    /// All content addresses referenced anywhere in the DAG.
    ///
    /// Unions entries from every peer HEAD's `head_index` with entries
    /// from every stored changeset's `index`. This is the full reference
    /// set used by GC to decide which blobs are still reachable — any
    /// `ContentAddr` not in this set is safe to delete from the blob
    /// store (subject to the staged-deletion grace period).
    pub fn all_referenced_addrs(&self) -> HashSet<ContentAddr> {
        let mut addrs = HashSet::new();
        for ps in self.peer_heads.values() {
            for (_, addr) in ps.head_index.iter() {
                addrs.insert(*addr);
            }
        }
        for cs in self.changesets.values() {
            for (_, addr) in cs.index.iter() {
                addrs.insert(*addr);
            }
        }
        addrs
    }

    /// All changesets reachable forward from `id` (inclusive).
    ///
    /// Walks child edges by inverting the stored parent pointers into a
    /// per-changeset child index, then BFS from `id`. Changesets whose
    /// parents are missing from the DAG (incomplete sync) are still
    /// considered — they are just not reached from `id` unless listed as
    /// a parent by some stored changeset.
    ///
    /// Returns `id` itself even if the DAG does not contain it; this
    /// mirrors [`ancestors`](Self::ancestors), which is silent on
    /// missing roots, and keeps `rollup` robust against stale checkpoints.
    pub fn descendants_inclusive(&self, id: &ChangeId) -> HashSet<ChangeId> {
        // Build reverse (parent -> children) index once per call. Callers
        // that roll up in a loop can cache this externally if needed.
        let mut children: HashMap<ChangeId, Vec<ChangeId>> = HashMap::new();
        for cs in self.changesets.values() {
            for p in &cs.parents {
                children.entry(*p).or_default().push(cs.id);
            }
        }

        let mut seen = HashSet::new();
        let mut queue: VecDeque<ChangeId> = VecDeque::new();
        seen.insert(*id);
        queue.push_back(*id);
        while let Some(next) = queue.pop_front() {
            if let Some(kids) = children.get(&next) {
                for k in kids {
                    if seen.insert(*k) {
                        queue.push_back(*k);
                    }
                }
            }
        }
        seen
    }

    /// Roll up the DAG at `checkpoint_id`: prune every changeset that is
    /// not a descendant (inclusive) of the checkpoint.
    ///
    /// Returns the set of [`ContentAddr`]s that became unreferenced — the
    /// caller feeds this to blob-store GC (typically via staged deletion
    /// so re-referenced blobs during sync have a grace period).
    ///
    /// Peer HEADs are not touched; callers should ensure every peer HEAD
    /// is already a descendant of the checkpoint before calling, or the
    /// HEAD's `head_index` will reference ContentAddrs that appear live
    /// without a corresponding changeset backing them.
    pub fn rollup(&mut self, checkpoint_id: ChangeId) -> HashSet<ContentAddr> {
        let before = self.all_referenced_addrs();
        let descendants = self.descendants_inclusive(&checkpoint_id);
        self.changesets.retain(|id, _| descendants.contains(id));
        let after = self.all_referenced_addrs();
        before.difference(&after).copied().collect()
    }

    /// Content addresses referenced by any current peer HEAD (live tier).
    ///
    /// Subset of [`all_referenced_addrs`](Self::all_referenced_addrs)
    /// restricted to addrs in `peer_heads`. Useful after a rollup to
    /// distinguish "still live" content from "only referenced by old
    /// history that is about to be pruned".
    pub fn live_addrs(&self) -> HashSet<ContentAddr> {
        let mut addrs = HashSet::new();
        for ps in self.peer_heads.values() {
            for (_, addr) in ps.head_index.iter() {
                addrs.insert(*addr);
            }
        }
        addrs
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
        Changeset::new(
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

    // ── Reference-set queries for GC (Phase 5) ──────────────────────────

    fn addr(byte: u8) -> ContentAddr {
        ContentAddr::new([byte; 32], 0)
    }

    #[test]
    fn all_referenced_addrs_unions_changesets_and_heads() {
        let mut dag = BraidDag::new();
        // Root references addr(1); child references addr(2).
        let root = mk(agent(1), vec![], "root", 1, 10);
        let child = mk(agent(1), vec![root.id], "child", 2, 20);
        let (_, child_id) = (root.id, child.id);
        let child_index = child.index.clone();
        dag.insert(root);
        dag.insert(child);

        // Peer HEAD points at child only.
        dag.update_peer_head(agent(1), child_id, child_index);

        let all = dag.all_referenced_addrs();
        assert!(all.contains(&addr(1)), "root's addr must be in all_referenced");
        assert!(all.contains(&addr(2)), "child's addr must be in all_referenced");
        assert_eq!(all.len(), 2);

        let live = dag.live_addrs();
        assert!(live.contains(&addr(2)), "child's addr is live (in HEAD)");
        assert!(
            !live.contains(&addr(1)),
            "root's addr is historical only — not live"
        );
        assert_eq!(live.len(), 1);
    }

    #[test]
    fn live_addrs_is_subset_of_all_referenced() {
        let mut dag = BraidDag::new();
        let root = mk(agent(1), vec![], "r", 1, 10);
        let root_id = root.id;
        let root_index = root.index.clone();
        dag.insert(root);
        dag.update_peer_head(agent(1), root_id, root_index);

        let all = dag.all_referenced_addrs();
        let live = dag.live_addrs();
        for a in &live {
            assert!(all.contains(a), "live must be subset of all_referenced");
        }
    }

    #[test]
    fn all_referenced_addrs_empty_for_empty_dag() {
        let dag = BraidDag::new();
        assert!(dag.all_referenced_addrs().is_empty());
        assert!(dag.live_addrs().is_empty());
    }

    // ── rollup + descendants_inclusive (Phase 5) ──────────────────────

    #[test]
    fn descendants_inclusive_linear_chain() {
        let mut dag = BraidDag::new();
        let a = mk(agent(1), vec![], "A", 1, 10);
        let b = mk(agent(1), vec![a.id], "B", 2, 20);
        let c = mk(agent(1), vec![b.id], "C", 3, 30);
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);
        dag.insert(a);
        dag.insert(b);
        dag.insert(c);

        let from_b = dag.descendants_inclusive(&b_id);
        assert_eq!(from_b.len(), 2);
        assert!(from_b.contains(&b_id));
        assert!(from_b.contains(&c_id));
        assert!(!from_b.contains(&a_id), "ancestors must not be reached forward");

        let from_c = dag.descendants_inclusive(&c_id);
        assert_eq!(from_c, HashSet::from([c_id]));
    }

    #[test]
    fn descendants_inclusive_branch_and_merge() {
        let mut dag = BraidDag::new();
        let root = mk(agent(1), vec![], "R", 1, 10);
        let left = mk(agent(2), vec![root.id], "L", 2, 20);
        let right = mk(agent(3), vec![root.id], "R2", 3, 20);
        let merge = mk(agent(1), vec![left.id, right.id], "M", 4, 30);
        let (root_id, left_id, right_id, merge_id) =
            (root.id, left.id, right.id, merge.id);
        dag.insert(root);
        dag.insert(left);
        dag.insert(right);
        dag.insert(merge);

        let from_root = dag.descendants_inclusive(&root_id);
        assert_eq!(from_root.len(), 4, "all reachable forward from root");
        assert!(from_root.contains(&merge_id));

        let from_left = dag.descendants_inclusive(&left_id);
        assert!(from_left.contains(&left_id));
        assert!(from_left.contains(&merge_id));
        assert!(!from_left.contains(&right_id), "siblings are not descendants");
    }

    #[test]
    fn rollup_prunes_pre_checkpoint_changesets() {
        let mut dag = BraidDag::new();
        // a -> b -> c. Checkpoint at b: prunes a, keeps b and c.
        let a = mk(agent(1), vec![], "A", 1, 10);
        let b = mk(agent(1), vec![a.id], "B", 2, 20);
        let c = mk(agent(1), vec![b.id], "C", 3, 30);
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);
        dag.insert(a);
        dag.insert(b);
        dag.insert(c);

        let freed = dag.rollup(b_id);

        assert!(!dag.contains(&a_id), "pre-checkpoint changeset must be pruned");
        assert!(dag.contains(&b_id), "checkpoint itself must be retained");
        assert!(dag.contains(&c_id), "descendant must be retained");

        // addr(1) was only referenced by A's index — it's freed.
        // addr(2) and addr(3) remain referenced by b and c.
        assert!(freed.contains(&addr(1)), "addr only in pruned changeset is freed");
        assert!(!freed.contains(&addr(2)), "addr still referenced is not freed");
    }

    #[test]
    fn rollup_with_head_pointing_at_checkpoint_keeps_addrs_live() {
        let mut dag = BraidDag::new();
        let a = mk(agent(1), vec![], "A", 1, 10);
        let b = mk(agent(1), vec![a.id], "B", 2, 20);
        let (_, b_id) = (a.id, b.id);
        let b_index = b.index.clone();
        dag.insert(a);
        dag.insert(b);
        dag.update_peer_head(agent(1), b_id, b_index);

        let _ = dag.rollup(b_id);

        let live = dag.live_addrs();
        assert!(live.contains(&addr(2)), "HEAD-referenced addr stays live post-rollup");
        assert!(!live.contains(&addr(1)), "ancestor-only addr is no longer live");
    }
}
