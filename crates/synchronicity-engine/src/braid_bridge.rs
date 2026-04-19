//! Bridge from a realm's [`BraidDag`] to a UI-friendly [`BraidView`].
//!
//! The drawer and sparkline render from a pre-computed [`BraidView`]
//! snapshot so they can update synchronously inside the Dioxus render
//! tree. This module is the glue: it walks a `BraidDag`, resolves
//! authors via `PeerDisplayInfo`, assigns a lane-per-peer and a
//! topological slot per commit, and formats evidence badges + hashes.

use std::collections::HashMap;

use indras_sync_engine::braid::changeset::{ChangeId, Changeset, Evidence};
use indras_sync_engine::braid::dag::BraidDag;

use crate::state::{
    member_hex_for, BraidView, CommitView, ConflictView, EvidenceView, PeerDisplayInfo,
    PeerHeadView, RealmId,
};

/// Build a [`BraidView`] from a [`BraidDag`] snapshot.
///
/// Pure transformation, no I/O. `peers` resolves display names; entries
/// not in the peer directory fall back to a short hex of the user id.
/// `self_user_id` flags which `PeerHeadView` is "you" in the drawer.
pub fn build_braid_view(
    realm_id: RealmId,
    dag: &BraidDag,
    peers: &[PeerDisplayInfo],
    self_user_id: [u8; 32],
    self_display_name: &str,
) -> BraidView {
    if dag.changesets.is_empty() {
        return BraidView {
            realm_id,
            ..BraidView::default()
        };
    }

    // ── Lane assignment ────────────────────────────────────────────
    // Unique authors observed in this DAG. Self first, then the rest
    // in first-seen order — stable within a snapshot.
    let mut lanes: Vec<[u8; 32]> = Vec::new();
    if dag.changesets.values().any(|c| c.author == self_user_id)
        || dag.peer_heads.contains_key(&self_user_id)
    {
        lanes.push(self_user_id);
    }
    for cs in ordered_by_ts(&dag.changesets) {
        if !lanes.contains(&cs.author) {
            lanes.push(cs.author);
        }
    }
    for user_id in dag.peer_heads.keys() {
        if !lanes.contains(user_id) {
            lanes.push(*user_id);
        }
    }

    // ── Slot assignment (topological depth) ────────────────────────
    // depth[id] = 1 + max(depth[parent]); roots = 0. Walk in timestamp
    // order so parents are computed before children (timestamps should
    // respect causality since the author sets them at land time).
    let mut depth: HashMap<ChangeId, usize> = HashMap::new();
    for cs in ordered_by_ts(&dag.changesets) {
        let d = cs
            .parents
            .iter()
            .filter_map(|p| depth.get(p).copied())
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        depth.insert(cs.id, d);
    }

    // ── Assign friendly c1/c2/… labels in temporal order ──────────
    let ts_sorted: Vec<&Changeset> = ordered_by_ts(&dag.changesets);
    let mut short_id_map: HashMap<ChangeId, String> = HashMap::new();
    for (i, cs) in ts_sorted.iter().enumerate() {
        short_id_map.insert(cs.id, format!("c{}", i + 1));
    }

    // ── Build CommitViews (newest first for the drawer's list) ────
    let mut commits: Vec<CommitView> = ts_sorted
        .iter()
        .map(|cs| {
            let author_color = member_hex_for(&cs.author).to_string();
            let author_name = resolve_name(cs.author, peers, self_user_id, self_display_name);
            let lane = lanes
                .iter()
                .position(|id| id == &cs.author)
                .unwrap_or(0);
            let slot = depth.get(&cs.id).copied().unwrap_or(0);
            let parents: Vec<String> = cs.parents.iter().map(short_hex_of_change).collect();
            CommitView {
                short_id: short_id_map.get(&cs.id).cloned().unwrap_or_default(),
                short_hex: short_hex_of_change(&cs.id),
                author_id: cs.author,
                author_name,
                author_color,
                intent: cs.intent.clone(),
                parents,
                evidence: evidence_to_view(&cs.evidence),
                timestamp_ms: cs.timestamp_millis,
                relative_time: crate::state::format_relative_time(cs.timestamp_millis),
                is_merge: cs.parents.len() > 1,
                lane,
                slot,
            }
        })
        .collect();
    commits.reverse(); // newest first

    // ── Build PeerHeadViews (one per peer_heads entry) ────────────
    let mut peer_heads: Vec<PeerHeadView> = lanes
        .iter()
        .filter_map(|user_id| {
            let head_state = dag.peer_heads.get(user_id)?;
            let short_hex = short_hex_of_change(&head_state.head);
            Some(PeerHeadView {
                user_id: *user_id,
                name: resolve_name(*user_id, peers, self_user_id, self_display_name),
                color: member_hex_for(user_id).to_string(),
                is_self: user_id == &self_user_id,
                head_short_id: short_id_map
                    .get(&head_state.head)
                    .cloned()
                    .unwrap_or_else(|| short_hex.chars().take(4).collect::<String>()),
                head_short_hex: short_hex,
                file_count: head_state.head_index.len(),
                relative_time: crate::state::format_relative_time(head_state.updated_ms),
                is_diverged: false, // filled in below
            })
        })
        .collect();

    // Mark diverged peers: anyone whose HEAD is not an ancestor of self's HEAD
    // AND isn't self's HEAD itself. (If we're missing self's HEAD, skip.)
    if let Some(self_head) = dag.peer_heads.get(&self_user_id).map(|ps| ps.head) {
        let ancestors = dag.ancestors(&self_head);
        for head in peer_heads.iter_mut() {
            if head.is_self {
                continue;
            }
            let their_head = dag
                .peer_heads
                .get(&head.user_id)
                .map(|ps| ps.head);
            if let Some(th) = their_head {
                head.is_diverged = th != self_head && !ancestors.contains(&th);
            }
        }
    }

    let pending_forks: Vec<PeerHeadView> =
        peer_heads.iter().filter(|p| p.is_diverged).cloned().collect();

    // Conflicts are surfaced after a three-way merge attempt, which
    // happens at auto_merge_trusted call sites — not at passive read.
    // Leave empty here; Phase 1b exposes merge-attempt conflicts.
    let conflicts: Vec<ConflictView> = Vec::new();

    BraidView {
        realm_id,
        peers: peer_heads,
        commits,
        pending_forks,
        conflicts,
    }
}

/// Deterministic timestamp ordering (ties broken by `ChangeId` hash).
fn ordered_by_ts(changesets: &HashMap<ChangeId, Changeset>) -> Vec<&Changeset> {
    let mut v: Vec<&Changeset> = changesets.values().collect();
    v.sort_by(|a, b| {
        a.timestamp_millis
            .cmp(&b.timestamp_millis)
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    });
    v
}

/// First 8 hex chars of a ChangeId (4 bytes).
fn short_hex_of_change(id: &ChangeId) -> String {
    let bytes = id.as_bytes();
    let mut s = String::with_capacity(8);
    for b in bytes.iter().take(4) {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Resolve a user id to a display name via the peer directory.
fn resolve_name(
    user_id: [u8; 32],
    peers: &[PeerDisplayInfo],
    self_user_id: [u8; 32],
    self_display_name: &str,
) -> String {
    if user_id == self_user_id && !self_display_name.is_empty() {
        return self_display_name.to_string();
    }
    if let Some(p) = peers.iter().find(|p| p.member_id == user_id) {
        return p.name.clone();
    }
    user_id.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

/// Map braid `Evidence` to the UI badge enum.
fn evidence_to_view(e: &Evidence) -> EvidenceView {
    match e {
        Evidence::Agent {
            compiled,
            tests_passed,
            lints_clean,
            ..
        } => {
            let all_ok = *compiled && *lints_clean;
            if all_ok {
                let test_count = tests_passed.len();
                let summary = if test_count > 0 {
                    format!("build · tests {test_count}/{test_count} · lint")
                } else {
                    "build · lint".to_string()
                };
                EvidenceView::AgentPass { summary }
            } else if !*compiled {
                EvidenceView::AgentFail {
                    reason: "build failed".to_string(),
                }
            } else {
                EvidenceView::AgentFail {
                    reason: "lint not clean".to_string(),
                }
            }
        }
        Evidence::Human { message, .. } => EvidenceView::Human {
            message: message.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_sync_engine::braid::changeset::PatchManifest;
    use indras_sync_engine::vault::vault_file::UserId;

    fn agent(b: u8) -> UserId {
        [b; 32]
    }

    fn human_evidence(u: UserId) -> Evidence {
        Evidence::Human {
            approved_by: u,
            approved_at_ms: 100,
            message: None,
        }
    }

    fn mk(author: UserId, parents: Vec<ChangeId>, intent: &str, ts: i64) -> Changeset {
        Changeset::new(
            author,
            parents,
            intent.into(),
            PatchManifest::default(),
            human_evidence(author),
            ts,
        )
    }

    #[test]
    fn empty_dag_yields_empty_view() {
        let dag = BraidDag::new();
        let view = build_braid_view([0; 32], &dag, &[], agent(1), "Love");
        assert!(view.commits.is_empty());
        assert!(view.peers.is_empty());
    }

    #[test]
    fn single_linear_chain_assigns_increasing_slots() {
        let mut dag = BraidDag::new();
        let a = mk(agent(1), vec![], "a", 10);
        let b = mk(agent(1), vec![a.id], "b", 20);
        let c = mk(agent(1), vec![b.id], "c", 30);
        dag.insert(a);
        dag.insert(b);
        dag.insert(c);

        let view = build_braid_view([0; 32], &dag, &[], agent(1), "Love");
        assert_eq!(view.commits.len(), 3);
        // Commits come out newest-first, so c is index 0
        assert_eq!(view.commits[0].slot, 2);
        assert_eq!(view.commits[1].slot, 1);
        assert_eq!(view.commits[2].slot, 0);
        // All same lane
        assert!(view.commits.iter().all(|c| c.lane == 0));
    }

    #[test]
    fn concurrent_authors_land_on_different_lanes() {
        let mut dag = BraidDag::new();
        let root = mk(agent(1), vec![], "root", 10);
        let fork_a = mk(agent(2), vec![root.id], "A side", 20);
        let fork_b = mk(agent(3), vec![root.id], "B side", 21);
        dag.insert(root);
        dag.insert(fork_a);
        dag.insert(fork_b);

        let view = build_braid_view([0; 32], &dag, &[], agent(1), "Love");
        let lanes: std::collections::HashSet<usize> =
            view.commits.iter().map(|c| c.lane).collect();
        assert!(lanes.len() >= 3, "three authors → three lanes");
    }

    #[test]
    fn merge_commit_flagged_is_merge() {
        let mut dag = BraidDag::new();
        let a = mk(agent(1), vec![], "a", 10);
        let b = mk(agent(2), vec![], "b", 11);
        let m = mk(agent(1), vec![a.id, b.id], "merge", 20);
        dag.insert(a);
        dag.insert(b);
        dag.insert(m);

        let view = build_braid_view([0; 32], &dag, &[], agent(1), "Love");
        assert!(
            view.commits.iter().any(|c| c.is_merge && c.parents.len() == 2),
            "merge commit should set is_merge"
        );
    }

    #[test]
    fn self_user_is_tagged_on_peer_head() {
        use indras_sync_engine::content_addr::SymlinkIndex;

        let mut dag = BraidDag::new();
        let a = mk(agent(1), vec![], "a", 10);
        let id = a.id;
        dag.insert(a);
        dag.update_peer_head(agent(1), id, SymlinkIndex::default());

        let view = build_braid_view([0; 32], &dag, &[], agent(1), "Love");
        let head = view.peers.iter().find(|p| p.is_self).expect("self head");
        assert_eq!(head.name, "Love");
    }
}
