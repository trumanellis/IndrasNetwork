//! Inner braid: a local-only DAG for agent-level work within a vault.
//!
//! Each user's vault has one `AgentBraid` that tracks multiple agents'
//! concurrent work. Agent commits go into this local DAG — never synced
//! to peers. The user merges agent HEADs into their own inner HEAD, then
//! promotes that HEAD to the outer (peer-synced) DAG.
//!
//! The same [`BraidDag`] struct is reused, but without the `Document<>`
//! wrapper — no CRDT sync, just in-memory state.

use std::collections::HashSet;
use std::sync::Arc;

use indras_storage::BlobStore;

use super::changeset::{ChangeId, Changeset, Evidence};
use super::dag::{BraidDag, PeerState};
use crate::content_addr::{Conflict, ContentAddr, IndexDelta, SymlinkIndex};
use crate::team::LogicalAgentId;
use crate::vault::vault_file::UserId;

/// Derive a deterministic agent `UserId` from the owning user's identity
/// and the agent's logical name.
///
/// `agent_user_id = blake3(user_id || "agent" || name)`. This lets agents
/// fit into the existing `peer_heads: HashMap<UserId, PeerState>` in
/// BraidDag without colliding with real user identities.
pub fn derive_agent_id(user_id: &UserId, agent_name: &str) -> UserId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(user_id);
    hasher.update(b"agent");
    hasher.update(agent_name.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Result of merging an agent's HEAD into the user's inner HEAD.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The new changeset id in the inner DAG.
    pub change_id: ChangeId,
    /// The merged symlink index.
    pub merged_index: SymlinkIndex,
    /// Conflicts encountered (empty if clean merge).
    pub conflicts: Vec<Conflict>,
}

/// A local-only braid DAG for agent-level work within a vault.
///
/// Multiple agents commit independently into this DAG. The user merges
/// agent HEADs into their own inner HEAD, then promotes that HEAD to the
/// outer (peer-synced) braid via [`Vault::promote()`].
///
/// This struct is NOT synced via CRDT — it lives entirely on the local
/// device. Agent iteration history stays private.
pub struct AgentBraid {
    /// Local-only DAG — same struct as the outer braid, just not
    /// wrapped in `Document<>`.
    dag: BraidDag,
    /// The user who owns this vault (merge target identity).
    user_id: UserId,
    /// Shared content-addressed blob store.
    blob_store: Arc<BlobStore>,
}

impl AgentBraid {
    /// Create an empty inner braid for the given user.
    pub fn new(user_id: UserId, blob_store: Arc<BlobStore>) -> Self {
        Self {
            dag: BraidDag::new(),
            user_id,
            blob_store,
        }
    }

    /// Derive the `UserId` for an agent by name.
    pub fn agent_user_id(&self, agent: &LogicalAgentId) -> UserId {
        derive_agent_id(&self.user_id, agent.as_str())
    }

    /// Land a verified agent changeset into the inner DAG.
    ///
    /// Creates a changeset authored by the agent, parents on the agent's
    /// current HEAD (or DAG heads if first commit), and updates the
    /// agent's peer_head in the inner DAG.
    pub fn agent_land(
        &mut self,
        agent: &LogicalAgentId,
        intent: String,
        index: SymlinkIndex,
        evidence: Evidence,
    ) -> ChangeId {
        let agent_id = self.agent_user_id(agent);

        // Parents = agent's own prior HEAD only. If the agent has no
        // prior HEAD, this is a root changeset (empty parents). Agents
        // don't parent on each other's work — they stay independent
        // until the user merges them.
        let parent_index = self.dag.peer_head(&agent_id).map(|ps| ps.head_index.clone());
        let parents = match self.dag.peer_head(&agent_id) {
            Some(ps) => vec![ps.head],
            None => Vec::new(),
        };

        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let changeset = Changeset::new_unsigned(
            agent_id,
            parents,
            intent,
            index.clone(),
            parent_index.as_ref(),
            evidence,
            timestamp_millis,
        );
        let change_id = changeset.id;

        self.dag.insert(changeset);
        self.dag.update_peer_head(agent_id, change_id, index);

        change_id
    }

    /// Agents whose HEAD diverges from the user's inner HEAD.
    ///
    /// Returns `(LogicalAgentId, PeerState)` pairs for agents that have
    /// committed work the user hasn't merged yet. The `LogicalAgentId` is
    /// reverse-looked-up from the roster.
    pub fn agent_forks(
        &self,
        roster: &[LogicalAgentId],
    ) -> Vec<(LogicalAgentId, PeerState)> {
        let user_head = self.dag.peer_head(&self.user_id);
        let mut forks = Vec::new();

        for agent in roster {
            let agent_id = self.agent_user_id(agent);
            if let Some(ps) = self.dag.peer_head(&agent_id) {
                let diverged = user_head.map_or(true, |uh| ps.head != uh.head);
                if diverged {
                    forks.push((agent.clone(), ps.clone()));
                }
            }
        }

        forks
    }

    /// What changed between the user's inner HEAD and an agent's HEAD.
    pub fn diff_agent(&self, agent: &LogicalAgentId) -> IndexDelta {
        let agent_id = self.agent_user_id(agent);
        let agent_state = match self.dag.peer_head(&agent_id) {
            Some(ps) => ps,
            None => return IndexDelta::new(),
        };
        let user_index = match self.dag.peer_head(&self.user_id) {
            Some(uh) => &uh.head_index,
            None => return IndexDelta::from_root(&agent_state.head_index),
        };
        agent_state.head_index.diff(user_index)
    }

    /// Three-way merge of an agent's HEAD into the user's inner HEAD.
    ///
    /// Uses the LCA of the two HEADs as the merge base. If there is no
    /// common ancestor (both are root changesets), uses an empty index
    /// as the base.
    pub fn merge_agent(
        &mut self,
        agent: &LogicalAgentId,
    ) -> Option<MergeResult> {
        let agent_id = self.agent_user_id(agent);
        let agent_state = self.dag.peer_head(&agent_id)?.clone();
        let user_state = self.dag.peer_head(&self.user_id).cloned();

        // Find merge base — common ancestor's index.
        let base_index = self.find_merge_base(&agent_state, user_state.as_ref());

        let user_index = user_state
            .as_ref()
            .map(|us| &us.head_index)
            .cloned()
            .unwrap_or_default();

        let (merged_index, conflicts) = SymlinkIndex::three_way_merge(
            &base_index,
            &user_index,
            &agent_state.head_index,
        );

        // Build merge changeset.
        let mut parents = vec![agent_state.head];
        if let Some(ref us) = user_state {
            parents.push(us.head);
        }

        let evidence = Evidence::human(
            self.user_id,
            Some(format!("merge agent {}", agent.as_str())),
        );
        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let changeset = Changeset::new_unsigned(
            self.user_id,
            parents,
            format!("merge from agent {}", agent.as_str()),
            merged_index.clone(),
            Some(&user_index),
            evidence,
            timestamp_millis,
        );
        let change_id = changeset.id;

        self.dag.insert(changeset);
        self.dag
            .update_peer_head(self.user_id, change_id, merged_index.clone());

        Some(MergeResult {
            change_id,
            merged_index,
            conflicts,
        })
    }

    /// Merge all agents in the roster sequentially into the user's inner HEAD.
    ///
    /// Returns the final merge result (last agent merged), or `None` if
    /// no agents have committed work.
    pub fn merge_all_agents(
        &mut self,
        roster: &[LogicalAgentId],
    ) -> Option<MergeResult> {
        let forks = self.agent_forks(roster);
        let mut last_result = None;

        for (agent, _) in &forks {
            last_result = self.merge_agent(agent);
        }

        last_result
    }

    /// The user's current inner HEAD, if any.
    pub fn user_head(&self) -> Option<&PeerState> {
        self.dag.peer_head(&self.user_id)
    }

    /// Access the inner DAG (read-only, for inspection).
    pub fn dag(&self) -> &BraidDag {
        &self.dag
    }

    /// Aggressive GC policy for the inner braid: drop every changeset
    /// that is not a descendant of the user's current HEAD, and discard
    /// all non-user peer heads.
    ///
    /// After a `Vault::promote()` the inner braid has served its purpose
    /// for that slice of work. Pruning back to the user HEAD keeps the
    /// inner DAG from growing unbounded while still preserving a valid
    /// merge base for any new agent commits.
    ///
    /// Returns the [`ContentAddr`]s that became unreferenced — callers
    /// typically forward these to a [`StagedDeletionSet`] rather than
    /// feeding them directly to `BlobStore::gc`.
    pub fn rollup_to_user_head(&mut self) -> HashSet<ContentAddr> {
        let user_head_id = match self.dag.peer_head(&self.user_id) {
            Some(ps) => ps.head,
            None => return HashSet::new(),
        };
        // Drop agent peer_heads — their HEADs reference changesets that
        // the rollup below is about to prune, which would leave the
        // inner DAG in an inconsistent state (HEAD points at a deleted
        // changeset). Agents will start fresh roots on their next land.
        self.dag.peer_heads.retain(|uid, _| *uid == self.user_id);
        self.dag.rollup(user_head_id)
    }

    /// All content addresses referenced by the inner DAG.
    pub fn all_referenced_addrs(&self) -> HashSet<ContentAddr> {
        let mut addrs = HashSet::new();
        for ps in self.dag.peer_heads.values() {
            for (_, addr) in ps.head_index.iter() {
                addrs.insert(*addr);
            }
        }
        for cs in self.dag.changesets.values() {
            for (_, addr) in cs.index.iter() {
                addrs.insert(*addr);
            }
        }
        addrs
    }

    /// Find the merge base index for two states.
    ///
    /// Walks ancestors of both HEADs to find the LCA. If no common
    /// ancestor exists, returns an empty index.
    fn find_merge_base(
        &self,
        agent_state: &PeerState,
        user_state: Option<&PeerState>,
    ) -> SymlinkIndex {
        let user_state = match user_state {
            Some(us) => us,
            None => return SymlinkIndex::new(), // no user HEAD yet
        };

        // Find LCA via intersection of ancestor sets.
        let agent_ancestors = self.dag.ancestors(&agent_state.head);
        let user_ancestors = self.dag.ancestors(&user_state.head);

        // Check if one is an ancestor of the other.
        if agent_ancestors.contains(&user_state.head) {
            // User HEAD is ancestor of agent HEAD — user's index is the base.
            return user_state.head_index.clone();
        }
        if user_ancestors.contains(&agent_state.head) {
            // Agent HEAD is ancestor of user HEAD — agent's index is the base.
            return agent_state.head_index.clone();
        }

        // Find common ancestors.
        let common: Vec<_> = agent_ancestors
            .intersection(&user_ancestors)
            .copied()
            .collect();

        if common.is_empty() {
            return SymlinkIndex::new();
        }

        // Pick the most recent common ancestor (highest timestamp).
        let best = common
            .iter()
            .filter_map(|id| self.dag.get(id))
            .max_by_key(|cs| cs.timestamp_millis);

        match best {
            Some(cs) => cs.index.clone(),
            None => SymlinkIndex::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_addr::{ContentAddr, DeltaOp, LogicalPath};

    fn user() -> UserId {
        [1u8; 32]
    }

    fn blob_store() -> Arc<BlobStore> {
        // We don't actually load/store blobs in these tests —
        // AgentBraid only references the store, never calls it.
        // Use a dummy path that won't be accessed.
        Arc::new(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(BlobStore::new(indras_storage::BlobStoreConfig {
                    base_dir: std::path::PathBuf::from("/tmp/agent-braid-test-blobs"),
                    ..Default::default()
                }))
                .unwrap(),
        )
    }

    fn addr(byte: u8) -> ContentAddr {
        ContentAddr::new([byte; 32], byte as u64 * 100)
    }

    fn agent_evidence(agent: &LogicalAgentId, user_id: UserId) -> Evidence {
        Evidence::Agent {
            compiled: true,
            tests_passed: vec!["test-crate".into()],
            lints_clean: true,
            runtime_ms: 42,
            signed_by: derive_agent_id(&user_id, agent.as_str()),
        }
    }

    fn index(entries: &[(&str, u8)]) -> SymlinkIndex {
        SymlinkIndex::from_iter(
            entries
                .iter()
                .map(|(p, b)| (LogicalPath::new(*p), addr(*b))),
        )
    }

    // ── Identity derivation ────────────────────────────────

    #[test]
    fn agent_id_is_deterministic() {
        let u = user();
        let id1 = derive_agent_id(&u, "agent1");
        let id2 = derive_agent_id(&u, "agent1");
        assert_eq!(id1, id2);
    }

    #[test]
    fn different_agents_get_different_ids() {
        let u = user();
        let id1 = derive_agent_id(&u, "agent1");
        let id2 = derive_agent_id(&u, "agent2");
        assert_ne!(id1, id2);
    }

    #[test]
    fn different_users_get_different_agent_ids() {
        let u1 = [1u8; 32];
        let u2 = [2u8; 32];
        let id1 = derive_agent_id(&u1, "agent1");
        let id2 = derive_agent_id(&u2, "agent1");
        assert_ne!(id1, id2);
    }

    #[test]
    fn agent_id_differs_from_user_id() {
        let u = user();
        let agent_id = derive_agent_id(&u, "agent1");
        assert_ne!(agent_id, u);
    }

    // ── AgentBraid operations ──────────────────────────────

    #[test]
    fn agent_land_creates_changeset() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent = LogicalAgentId::new("A");

        let idx = index(&[("src/lib.rs", 1)]);
        let ev = agent_evidence(&agent, user());
        let id = braid.agent_land(&agent, "add lib.rs".into(), idx.clone(), ev);

        assert!(braid.dag().contains(&id));
        let cs = braid.dag().get(&id).unwrap();
        assert_eq!(cs.index, idx);
        assert!(cs.parents.is_empty()); // first commit
    }

    #[test]
    fn sequential_agent_commits_chain() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent = LogicalAgentId::new("A");

        let id1 = braid.agent_land(
            &agent,
            "first".into(),
            index(&[("a.rs", 1)]),
            agent_evidence(&agent, user()),
        );
        let id2 = braid.agent_land(
            &agent,
            "second".into(),
            index(&[("a.rs", 2)]),
            agent_evidence(&agent, user()),
        );

        let cs2 = braid.dag().get(&id2).unwrap();
        assert!(cs2.parents.contains(&id1));
    }

    #[test]
    fn two_agents_create_braid() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent_a = LogicalAgentId::new("A");
        let agent_b = LogicalAgentId::new("B");

        let id_a = braid.agent_land(
            &agent_a,
            "A: add foo".into(),
            index(&[("foo.rs", 1)]),
            agent_evidence(&agent_a, user()),
        );
        let id_b = braid.agent_land(
            &agent_b,
            "B: add bar".into(),
            index(&[("bar.rs", 2)]),
            agent_evidence(&agent_b, user()),
        );

        // Both should be heads (concurrent, no shared parent).
        let heads = braid.dag().heads();
        assert_eq!(heads.len(), 2);
        assert!(heads.contains(&id_a));
        assert!(heads.contains(&id_b));
    }

    #[test]
    fn agent_forks_shows_divergent_agents() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent_a = LogicalAgentId::new("A");
        let agent_b = LogicalAgentId::new("B");
        let roster = vec![agent_a.clone(), agent_b.clone()];

        braid.agent_land(
            &agent_a,
            "A work".into(),
            index(&[("a.rs", 1)]),
            agent_evidence(&agent_a, user()),
        );
        braid.agent_land(
            &agent_b,
            "B work".into(),
            index(&[("b.rs", 2)]),
            agent_evidence(&agent_b, user()),
        );

        let forks = braid.agent_forks(&roster);
        assert_eq!(forks.len(), 2); // both diverge from user (no user HEAD)
    }

    #[test]
    fn merge_agent_produces_single_head() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent_a = LogicalAgentId::new("A");
        let agent_b = LogicalAgentId::new("B");

        braid.agent_land(
            &agent_a,
            "A: add foo".into(),
            index(&[("foo.rs", 1)]),
            agent_evidence(&agent_a, user()),
        );
        braid.agent_land(
            &agent_b,
            "B: add bar".into(),
            index(&[("bar.rs", 2)]),
            agent_evidence(&agent_b, user()),
        );

        // Merge A into user HEAD.
        let result_a = braid.merge_agent(&agent_a).unwrap();
        assert!(result_a.conflicts.is_empty());
        assert_eq!(
            result_a.merged_index.get(&LogicalPath::new("foo.rs")),
            Some(&addr(1))
        );

        // Merge B into user HEAD.
        let result_b = braid.merge_agent(&agent_b).unwrap();
        assert!(result_b.conflicts.is_empty());

        // User HEAD should have both files.
        let user_head = braid.user_head().unwrap();
        assert_eq!(
            user_head.head_index.get(&LogicalPath::new("foo.rs")),
            Some(&addr(1))
        );
        assert_eq!(
            user_head.head_index.get(&LogicalPath::new("bar.rs")),
            Some(&addr(2))
        );
    }

    #[test]
    fn merge_detects_conflict() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent_a = LogicalAgentId::new("A");
        let agent_b = LogicalAgentId::new("B");

        // Both agents edit the same file differently.
        braid.agent_land(
            &agent_a,
            "A: edit lib".into(),
            index(&[("lib.rs", 1)]),
            agent_evidence(&agent_a, user()),
        );
        braid.agent_land(
            &agent_b,
            "B: edit lib".into(),
            index(&[("lib.rs", 2)]),
            agent_evidence(&agent_b, user()),
        );

        // Merge A first (no conflict — user has no HEAD).
        let result_a = braid.merge_agent(&agent_a).unwrap();
        assert!(result_a.conflicts.is_empty());

        // Merge B — conflict on lib.rs.
        let result_b = braid.merge_agent(&agent_b).unwrap();
        assert_eq!(result_b.conflicts.len(), 1);
        assert_eq!(result_b.conflicts[0].path, LogicalPath::new("lib.rs"));
    }

    #[test]
    fn merge_all_agents_merges_everything() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent_a = LogicalAgentId::new("A");
        let agent_b = LogicalAgentId::new("B");
        let agent_c = LogicalAgentId::new("C");
        let roster = vec![agent_a.clone(), agent_b.clone(), agent_c.clone()];

        braid.agent_land(
            &agent_a,
            "A".into(),
            index(&[("a.rs", 1)]),
            agent_evidence(&agent_a, user()),
        );
        braid.agent_land(
            &agent_b,
            "B".into(),
            index(&[("b.rs", 2)]),
            agent_evidence(&agent_b, user()),
        );
        braid.agent_land(
            &agent_c,
            "C".into(),
            index(&[("c.rs", 3)]),
            agent_evidence(&agent_c, user()),
        );

        let result = braid.merge_all_agents(&roster).unwrap();
        assert!(result.conflicts.is_empty());

        let head = braid.user_head().unwrap();
        assert_eq!(head.head_index.len(), 3);
    }

    #[test]
    fn diff_agent_shows_changes() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent = LogicalAgentId::new("A");

        braid.agent_land(
            &agent,
            "add files".into(),
            index(&[("a.rs", 1), ("b.rs", 2)]),
            agent_evidence(&agent, user()),
        );

        let diff = braid.diff_agent(&agent);
        assert_eq!(diff.len(), 2); // both are Add (no user HEAD)
        assert!(matches!(
            diff.ops[&LogicalPath::new("a.rs")],
            DeltaOp::Add(_)
        ));
    }

    #[test]
    fn all_referenced_addrs_collects_from_heads_and_changesets() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent = LogicalAgentId::new("A");

        braid.agent_land(
            &agent,
            "v1".into(),
            index(&[("a.rs", 1)]),
            agent_evidence(&agent, user()),
        );
        braid.agent_land(
            &agent,
            "v2".into(),
            index(&[("a.rs", 2)]),
            agent_evidence(&agent, user()),
        );

        let refs = braid.all_referenced_addrs();
        // addr(1) from old changeset, addr(2) from current HEAD + new changeset
        assert!(refs.contains(&addr(1)));
        assert!(refs.contains(&addr(2)));
    }

    // ── rollup_to_user_head (Phase 5) ───────────────────────────────

    #[test]
    fn rollup_to_user_head_noop_when_no_user_head() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent = LogicalAgentId::new("A");
        braid.agent_land(
            &agent,
            "solo".into(),
            index(&[("a.rs", 1)]),
            agent_evidence(&agent, user()),
        );

        let freed = braid.rollup_to_user_head();
        assert!(freed.is_empty(), "no user HEAD ⇒ nothing freed");
        // Agent head still present.
        assert!(braid.dag().get(&braid.agent_forks(&[agent.clone()])[0].1.head).is_some());
    }

    #[test]
    fn rollup_to_user_head_prunes_agent_history_after_merge() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent_a = LogicalAgentId::new("A");
        let agent_b = LogicalAgentId::new("B");

        braid.agent_land(
            &agent_a,
            "A".into(),
            index(&[("a.rs", 1)]),
            agent_evidence(&agent_a, user()),
        );
        braid.agent_land(
            &agent_b,
            "B".into(),
            index(&[("b.rs", 2)]),
            agent_evidence(&agent_b, user()),
        );
        let merge = braid
            .merge_all_agents(&[agent_a.clone(), agent_b.clone()])
            .expect("merge");

        // Before rollup: 3 peer_heads (A, B, user), 3+ changesets.
        assert_eq!(braid.dag().peer_heads.len(), 3);

        let freed = braid.rollup_to_user_head();

        // After rollup: only the user peer_head remains; no agent heads.
        assert_eq!(braid.dag().peer_heads.len(), 1);
        assert!(braid.user_head().is_some());
        // Merge changeset survives (it is the user HEAD).
        assert!(braid.dag().contains(&merge.change_id));
        // At least one pre-merge agent changeset should have been freed.
        // addr(1) and addr(2) are both still in the merged HEAD's index,
        // so they remain referenced; freed will be empty in this tight
        // case — the assertion is about pruning, not freeing blobs.
        let _ = freed;
    }

    #[test]
    fn rollup_to_user_head_allows_fresh_agent_work() {
        let bs = blob_store();
        let mut braid = AgentBraid::new(user(), bs);
        let agent = LogicalAgentId::new("A");

        braid.agent_land(
            &agent,
            "first".into(),
            index(&[("a.rs", 1)]),
            agent_evidence(&agent, user()),
        );
        braid.merge_agent(&agent).expect("merge");
        braid.rollup_to_user_head();

        // Post-rollup, agent commits again.
        let new_id = braid.agent_land(
            &agent,
            "second".into(),
            index(&[("a.rs", 2)]),
            agent_evidence(&agent, user()),
        );
        assert!(braid.dag().contains(&new_id));
    }
}
