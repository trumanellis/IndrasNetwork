//! Local gate: a thin orchestration struct that pairs a workspace root and
//! agent identity with a [`BraidDag`].
//!
//! Unlike the old `indras-braid::LocalRepo`, this version carries **no**
//! in-memory `SourceTree` and **no** `PayloadStore` — the vault IS the
//! working tree, and vault content storage already handles blobs.
//!
//! The primary entry point for committing work is
//! [`RealmBraid::try_land`](super::realm_braid::RealmBraid::try_land), which
//! owns the vault/DAG document handles. `LocalRepo` is useful for tests that
//! need a DAG + agent identity without a full realm.

use std::path::PathBuf;

use super::{
    changeset::ChangeId,
    dag::BraidDag,
    verification::VerificationFailure,
};
use crate::vault::vault_file::UserId;

/// Errors returned by braid gate operations.
#[derive(Debug, thiserror::Error)]
pub enum TryLandError {
    /// `try_land` was called with an empty edit list — nothing to commit.
    #[error("nothing to land: touched path list is empty")]
    NothingToLand,

    /// The verification suite reported a failure.
    #[error("verification failed: {0}")]
    Verification(#[from] VerificationFailure),

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A network/document-layer error occurred.
    #[error("network error: {0}")]
    Network(#[from] indras_network::error::IndraError),
}

/// A thin local-agent handle: workspace root, agent identity, and a DAG.
///
/// For production use, prefer
/// [`RealmBraid`](super::realm_braid::RealmBraid) which wires the DAG and
/// vault documents through a `Realm`. `LocalRepo` is primarily a convenience
/// for tests and offline analysis.
pub struct LocalRepo {
    /// In-memory DAG for this agent.
    pub dag: BraidDag,
    /// Absolute path to the Cargo workspace root on disk.
    pub workspace_root: PathBuf,
    /// Identity of the agent operating this repo.
    pub agent: UserId,
}

impl std::fmt::Debug for LocalRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalRepo")
            .field("workspace_root", &self.workspace_root)
            .field("agent", &self.agent)
            .field("dag_len", &self.dag.len())
            .finish_non_exhaustive()
    }
}

impl LocalRepo {
    /// Create a new, empty `LocalRepo`.
    pub fn new(workspace_root: PathBuf, agent: UserId) -> Self {
        Self {
            dag: BraidDag::new(),
            workspace_root,
            agent,
        }
    }

    /// Return the current DAG head ids, sorted for stable iteration.
    pub fn head_ids(&self) -> Vec<ChangeId> {
        let mut heads: Vec<ChangeId> = self.dag.heads().into_iter().collect();
        heads.sort();
        heads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(byte: u8) -> UserId {
        [byte; 32]
    }

    #[test]
    fn head_ids_empty_when_no_changesets() {
        let repo = LocalRepo::new(PathBuf::from("/tmp/test-workspace"), agent(1));
        assert!(repo.head_ids().is_empty());
    }

    #[test]
    fn debug_format_includes_agent_and_root() {
        let repo = LocalRepo::new(PathBuf::from("/tmp/xx"), agent(7));
        let s = format!("{repo:?}");
        assert!(s.contains("LocalRepo"));
        assert!(s.contains("/tmp/xx"));
    }
}
