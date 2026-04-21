//! `Vault::sync_all` — the one-call composite that stitches the full
//! commit pipeline together.
//!
//! Individually, the vault already exposes the pieces:
//!
//! - [`Vault::merge_agent`] folds one agent's inner HEAD into the user's
//!   inner HEAD.
//! - [`Vault::promote`] lifts the user's inner HEAD to a signed outer
//!   changeset and advances `peer_heads`.
//! - [`Vault::auto_merge_trusted`] pulls any trusted peer forks.
//!
//! A single UI action or IPC commit wants all of them run in order —
//! merge every agent, promote if there is anything new to promote,
//! absorb trusted peer work, and materialize the resulting outer HEAD
//! to the on-disk vault root. [`Vault::sync_all`] does exactly that and
//! returns a [`SyncAllReport`] so the caller can render a post-sync
//! summary without a second round-trip.
//!
//! This is the *library* side. The app rewires its commit entry points
//! to call `sync_all` after `land_agent_snapshot` in Phase 2's second
//! subtask.

use std::sync::Arc;

use indras_network::error::Result;
use indras_storage::ContentRef;

use crate::braid::ChangeId;
use crate::content_addr::Conflict;
use crate::team::LogicalAgentId;
use crate::vault::Vault;
use crate::UserId;

/// Outcome of a single [`Vault::sync_all`] call.
///
/// Every field is a plain summary — no shared-lock guards, no borrows
/// back into the vault — so the caller can log, render, or forward it
/// without holding any vault state alive.
#[derive(Debug, Default, Clone)]
pub struct SyncAllReport {
    /// Each bound agent that had diverged inner-HEAD work, paired with
    /// the merge changeset id inserted into the inner DAG. Empty when
    /// no agents had pending commits.
    pub agent_merges: Vec<(LogicalAgentId, ChangeId)>,
    /// The outer-DAG changeset id produced by `promote`, if the user's
    /// inner HEAD differed from the outer HEAD at call time. `None` if
    /// there was nothing to promote.
    pub promoted: Option<ChangeId>,
    /// Trusted peers whose forks auto-merged into the user's outer
    /// HEAD, paired with each merge's changeset id.
    pub peer_merges: Vec<(UserId, ChangeId)>,
    /// Number of files written (or refreshed) under the vault root
    /// after the pipeline converged. Zero if no outer HEAD exists yet.
    pub materialized: usize,
    /// Aggregated conflicts surfaced during agent merges. Empty for
    /// clean syncs. `sync_all` does NOT abort on conflicts — it records
    /// them and lets the UI decide how to surface resolution.
    pub conflicts: Vec<Conflict>,
}

impl Vault {
    /// Run the full braid sync pipeline in one call.
    ///
    /// Steps, in order:
    ///
    /// 1. Merge every agent in `roster` whose inner HEAD differs from
    ///    the user's inner HEAD.
    /// 2. If the merged inner index is not reflected on the outer DAG,
    ///    call [`Vault::promote`] to publish a signed outer changeset
    ///    and advance the user's `peer_head`. The existing inner-braid
    ///    rollup policy (see `fancy-wiggling-pony` Phase 5) fires
    ///    inside `promote`.
    /// 3. Auto-merge trusted peer forks via
    ///    [`Vault::auto_merge_trusted`].
    /// 4. Materialize the resulting outer HEAD's file index to the
    ///    vault root on disk.
    ///
    /// Returns a [`SyncAllReport`] with everything that happened.
    /// Errors propagate out of step 2 (`promote`); blob-load or write
    /// failures during materialization are logged but not surfaced —
    /// the sync still counts as landed from the CRDT's point of view.
    pub async fn sync_all(
        &self,
        intent: String,
        roster: &[LogicalAgentId],
    ) -> Result<SyncAllReport> {
        let mut report = SyncAllReport::default();

        // Step 1: merge all agents under one inner-braid write lock.
        {
            let mut inner = self.inner_braid.write().await;
            let forks = inner.agent_forks(roster);
            for (agent, _state) in forks {
                if let Some(result) = inner.merge_agent(&agent) {
                    report.agent_merges.push((agent, result.change_id));
                    report.conflicts.extend(result.conflicts);
                }
            }
        }

        // Step 2: promote if the inner HEAD's index is not reflected on
        // the outer HEAD. Comparing head_indexes (not change_ids) means
        // we don't churn an empty promote when the outer DAG already
        // reflects every file in the inner HEAD.
        let needs_promote = {
            let inner_idx = self
                .inner_braid
                .read()
                .await
                .user_head()
                .map(|ps| ps.head_index.clone());
            let outer_idx = self
                .dag
                .read()
                .await
                .peer_head(&self.user_id)
                .map(|ps| ps.head_index.clone());
            match (inner_idx, outer_idx) {
                (Some(inner), Some(outer)) => inner != outer,
                (Some(_), None) => true,
                _ => false,
            }
        };
        if needs_promote {
            report.promoted = Some(self.promote(intent).await?);
        }

        // Step 3: pull in trusted peers' work.
        report.peer_merges = self.auto_merge_trusted().await;

        // Step 4: write the outer HEAD's file set to disk.
        report.materialized = self.materialize_user_outer_head().await;

        Ok(report)
    }

    /// Write every file referenced by the user's current outer-HEAD
    /// symlink index to the vault root on disk. Returns the number of
    /// files successfully written.
    ///
    /// Tolerant: missing blobs or write failures are logged and skipped
    /// rather than failing the whole materialization. The caller is
    /// `sync_all`, which records the count but doesn't error on
    /// partials.
    async fn materialize_user_outer_head(&self) -> usize {
        let index = {
            let dag = self.dag.read().await;
            match dag.peer_head(&self.user_id) {
                Some(ps) => ps.head_index.clone(),
                None => return 0,
            }
        };
        let blob = Arc::clone(&self.blob_store);
        let mut written = 0;
        for (logical_path, addr) in index.iter() {
            let content_ref = ContentRef::new(addr.hash, addr.size);
            let bytes = match blob.load(&content_ref).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(
                        path = %logical_path.0,
                        error = %e,
                        "sync_all: blob load failed; skipping file"
                    );
                    continue;
                }
            };
            let target = self.vault_path.join(&logical_path.0);
            if let Some(parent) = target.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            match tokio::fs::write(&target, &bytes).await {
                Ok(()) => written += 1,
                Err(e) => tracing::warn!(
                    path = %logical_path.0,
                    error = %e,
                    "sync_all: write to vault root failed"
                ),
            }
        }
        written
    }
}
