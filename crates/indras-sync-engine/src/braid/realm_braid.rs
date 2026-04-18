//! `RealmBraid`: extension trait exposing the braid DAG on the vault realm.
//!
//! The braid DAG lives directly on the vault realm so that all vault
//! members — both human users and AI agents — participate in the same
//! DAG. There is no separate team realm.

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
    verification::{self, VerificationFailure, VerificationRequest},
};
use crate::realm_vault::RealmVault;
use crate::vault::vault_file::UserId;

/// Run the verification suite (build + test + clippy) for the given
/// crates without inserting a changeset. Returns [`Evidence`] on
/// success, [`VerificationFailure`] on failure.
///
/// This is the standalone version of the gate that `try_land` runs
/// internally. Use it when you want to verify *before* deciding
/// whether to commit (e.g., the UI's "Verify" button) or when you
/// need Evidence for a manually-assembled [`Changeset`].
pub async fn verify_only(
    crates: Vec<String>,
    workspace_root: PathBuf,
    agent: UserId,
) -> std::result::Result<Evidence, VerificationFailure> {
    let req = VerificationRequest {
        crates,
        workspace_root,
        agent,
        run_clippy: true,
        run_tests: true,
    };
    verification::run(&req).await
}

/// Realm extension trait adding braid-DAG access and the `try_land` gate.
///
/// The DAG document lives directly on the vault realm — all vault
/// members participate.
#[allow(async_fn_in_trait)]
pub trait RealmBraid {
    /// Get the vault realm's braid-DAG document.
    ///
    /// Safe to call repeatedly — the document lookup returns a fresh handle
    /// backed by the same in-memory state.
    async fn braid_dag(&self) -> Result<Document<BraidDag>>;

    /// Snapshot the current vault state for the given paths and produce a
    /// [`PatchManifest`] referencing their content hashes.
    ///
    /// Paths not present in the vault index are skipped silently. The
    /// returned manifest's `files` are sorted by path for deterministic
    /// hashing.
    async fn snapshot_patch(&self, paths: &[String]) -> Result<PatchManifest>;

    /// Run verification for `crates`, and if green, insert a changeset
    /// whose `patch` is the provided [`PatchManifest`].
    ///
    /// The caller is responsible for building the manifest — typically
    /// by snapshotting a [`LocalWorkspaceIndex`](crate::workspace::LocalWorkspaceIndex)
    /// of agent-owned working-tree state. This lets agent disk edits
    /// remain device-local until `try_land`; a synced `vault_index` is
    /// no longer consulted on the commit path.
    ///
    /// Flow:
    /// 1. Reject empty manifests.
    /// 2. Run the full verification suite.
    /// 3. Build a changeset whose `parents` are the current DAG heads.
    /// 4. Insert into the DAG.
    async fn try_land(
        &self,
        intent: String,
        manifest: PatchManifest,
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
    async fn braid_dag_subscribe(
        &self,
    ) -> Result<broadcast::Receiver<DocumentChange<BraidDag>>>;
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
        manifest: PatchManifest,
        crates: Vec<String>,
        workspace_root: PathBuf,
        agent: UserId,
    ) -> std::result::Result<ChangeId, TryLandError> {
        if manifest.files.is_empty() {
            return Err(TryLandError::NothingToLand);
        }

        // 1. Run verification on the caller-declared crates.
        let evidence = verify_only(crates, workspace_root.clone(), agent).await?;

        // 2. Build the changeset with parents = current DAG heads.
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

    async fn braid_dag_subscribe(
        &self,
    ) -> Result<broadcast::Receiver<DocumentChange<BraidDag>>> {
        Ok(self.braid_dag().await?.subscribe())
    }
}
