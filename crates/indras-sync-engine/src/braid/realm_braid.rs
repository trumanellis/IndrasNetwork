//! `RealmBraid`: extension trait exposing the braid DAG on a `Realm`.
//!
//! Mirrors the shape of [`RealmVault`](crate::realm_vault::RealmVault).
//! The braid DAG is a single named document (`"braid-dag"`) stored on the
//! realm; source file bytes continue to live in the realm's `vault_index`
//! document. Changesets reference vault file versions by `(path, hash)`.

use std::path::PathBuf;

use chrono::Utc;
use indras_network::document::{Document, DocumentChange};
use indras_network::error::Result;
use indras_network::Realm;
use tokio::sync::broadcast;

use super::{
    changeset::{ChangeId, Changeset, Evidence, PatchFile, PatchManifest},
    dag::BraidDag,
    gate::TryLandError,
    verification::{self, VerificationRequest},
};
use crate::realm_vault::RealmVault;
use crate::vault::vault_file::UserId;

/// Realm extension trait adding braid-DAG access and the `try_land` gate.
#[allow(async_fn_in_trait)]
pub trait RealmBraid {
    /// Get (or create) the realm's braid-DAG document.
    async fn braid_dag(&self) -> Result<Document<BraidDag>>;

    /// Snapshot the current vault state for the given paths and produce a
    /// [`PatchManifest`] referencing their content hashes.
    ///
    /// Paths not present in the vault index are skipped silently. The
    /// returned manifest's `files` are sorted by path for deterministic
    /// hashing.
    async fn snapshot_patch(&self, paths: &[String]) -> Result<PatchManifest>;

    /// Run verification for `crates`, and if green, insert a changeset whose
    /// patch references the vault's current state of `touched_paths`.
    ///
    /// Flow:
    /// 1. Reject empty `touched_paths`.
    /// 2. Run the full verification suite.
    /// 3. Snapshot the vault for `touched_paths` into a [`PatchManifest`].
    /// 4. Build a changeset whose `parents` are the current DAG heads.
    /// 5. Insert into the DAG (no disk I/O — the caller already wrote).
    async fn try_land(
        &self,
        intent: String,
        touched_paths: Vec<String>,
        crates: Vec<String>,
        workspace_root: PathBuf,
        agent: UserId,
    ) -> std::result::Result<ChangeId, TryLandError>;

    /// Read the current heads of the braid DAG.
    async fn braid_heads(&self) -> Result<Vec<ChangeId>>;

    /// Subscribe to braid-DAG change events.
    ///
    /// Yields a [`DocumentChange`] each time the DAG is updated locally or
    /// merged from a peer. Use this to surface "peer X published a verified
    /// changeset" notifications; the caller decides whether to `checkout`
    /// that changeset into their vault.
    async fn braid_dag_subscribe(&self) -> Result<broadcast::Receiver<DocumentChange<BraidDag>>>;
}

impl RealmBraid for Realm {
    async fn braid_dag(&self) -> Result<Document<BraidDag>> {
        self.document::<BraidDag>("braid-dag").await
    }

    async fn snapshot_patch(&self, paths: &[String]) -> Result<PatchManifest> {
        let idx = self.vault_index().await?;
        let doc = idx.read().await;
        let mut files: Vec<PatchFile> = Vec::new();
        for path in paths {
            if let Some(vf) = doc.files.get(path) {
                if vf.deleted {
                    continue;
                }
                files.push(PatchFile {
                    path: vf.path.clone(),
                    hash: vf.hash,
                    size: vf.size,
                });
            }
        }
        Ok(PatchManifest::new(files))
    }

    async fn try_land(
        &self,
        intent: String,
        touched_paths: Vec<String>,
        crates: Vec<String>,
        workspace_root: PathBuf,
        agent: UserId,
    ) -> std::result::Result<ChangeId, TryLandError> {
        if touched_paths.is_empty() {
            return Err(TryLandError::NothingToLand);
        }

        // 1. Run verification on the caller-declared crates.
        let req = VerificationRequest {
            crates,
            workspace_root,
            agent,
            run_clippy: true,
            run_tests: true,
        };
        let evidence: Evidence = verification::run(&req).await?;

        // 2. Snapshot the vault for the touched paths.
        let manifest = self.snapshot_patch(&touched_paths).await?;

        // 3. Build the changeset with parents = current DAG heads.
        let dag = self.braid_dag().await?;
        let mut parents: Vec<ChangeId> = dag.read().await.heads().into_iter().collect();
        parents.sort();

        let timestamp_millis = Utc::now().timestamp_millis();
        let changeset = Changeset::new(
            agent,
            parents,
            intent,
            manifest,
            evidence,
            timestamp_millis,
        );
        let change_id = changeset.id;

        dag.update(|d| d.insert(changeset)).await?;

        Ok(change_id)
    }

    async fn braid_heads(&self) -> Result<Vec<ChangeId>> {
        let dag = self.braid_dag().await?;
        let mut heads: Vec<ChangeId> = dag.read().await.heads().into_iter().collect();
        heads.sort();
        Ok(heads)
    }

    async fn braid_dag_subscribe(&self) -> Result<broadcast::Receiver<DocumentChange<BraidDag>>> {
        Ok(self.braid_dag().await?.subscribe())
    }
}
