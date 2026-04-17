//! `RealmBraid`: extension trait exposing the braid DAG through a synced vault.
//!
//! In the two-realm architecture, braid DAG state lives on the **team realm**
//! — a separate realm materialized deterministically from the synced vault's
//! id (see [`RealmTeam`](crate::realm_team::RealmTeam)). Non-team devices
//! that only sync the vault realm never pull the DAG; only devices that host
//! agents for the team join the team realm and subscribe.
//!
//! The trait is still rooted on the synced-vault [`Realm`] because that's
//! the handle callers naturally have. Each DAG-touching method takes an
//! `&IndrasNetwork` so it can resolve the team realm via `ensure_team_realm`.
//! `snapshot_patch` stays on the vault realm because it reads the vault's
//! own file index.

use std::path::PathBuf;

use chrono::Utc;
use indras_network::document::{Document, DocumentChange};
use indras_network::error::{IndraError, Result};
use indras_network::{IndrasNetwork, Realm};
use tokio::sync::broadcast;

use super::{
    changeset::{ChangeId, Changeset, Evidence, PatchFile, PatchManifest},
    dag::BraidDag,
    gate::TryLandError,
    verification::{self, VerificationFailure, VerificationRequest},
};
use crate::realm_team::RealmTeam;
use crate::realm_vault::RealmVault;
use crate::vault::vault_file::UserId;

/// Human-readable label for the team realm when it is first materialized.
/// The id is derived deterministically; the name is purely cosmetic.
pub(crate) const TEAM_REALM_NAME: &str = "team-realm";

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

/// Resolve the team realm handle for this vault realm, materializing it via
/// deterministic derivation if this device hasn't opened it yet.
async fn team_realm_handle(vault_realm: &Realm, network: &IndrasNetwork) -> Result<Realm> {
    let team_realm_id = vault_realm
        .ensure_team_realm(network, TEAM_REALM_NAME)
        .await?;
    network
        .get_realm_by_id(&team_realm_id)
        .ok_or_else(|| IndraError::RealmNotFound {
            id: format!("{team_realm_id:?}"),
        })
}

/// Realm extension trait adding braid-DAG access and the `try_land` gate.
#[allow(async_fn_in_trait)]
pub trait RealmBraid {
    /// Get (or create) the team realm's braid-DAG document.
    ///
    /// `self` is the synced-vault realm; the DAG document itself lives on
    /// the derived team realm. Safe to call repeatedly — the team realm is
    /// idempotent and the document lookup just returns a fresh handle.
    async fn braid_dag(&self, network: &IndrasNetwork) -> Result<Document<BraidDag>>;

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
    /// 4. Insert into the DAG on the team realm.
    async fn try_land(
        &self,
        network: &IndrasNetwork,
        intent: String,
        manifest: PatchManifest,
        crates: Vec<String>,
        workspace_root: PathBuf,
        agent: UserId,
    ) -> std::result::Result<ChangeId, TryLandError>;

    /// Read the current heads of the braid DAG (on the team realm).
    async fn braid_heads(&self, network: &IndrasNetwork) -> Result<Vec<ChangeId>>;

    /// Subscribe to braid-DAG change events.
    ///
    /// Yields a [`DocumentChange`] each time the DAG is updated locally or
    /// merged from a peer. Use this to surface "peer X published a verified
    /// changeset" notifications; the caller decides whether to `checkout`
    /// that changeset into their vault.
    async fn braid_dag_subscribe(
        &self,
        network: &IndrasNetwork,
    ) -> Result<broadcast::Receiver<DocumentChange<BraidDag>>>;
}

impl RealmBraid for Realm {
    async fn braid_dag(&self, network: &IndrasNetwork) -> Result<Document<BraidDag>> {
        let team = team_realm_handle(self, network).await?;
        team.document::<BraidDag>("braid-dag").await
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
        network: &IndrasNetwork,
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
        let dag = self.braid_dag(network).await?;
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

    async fn braid_heads(&self, network: &IndrasNetwork) -> Result<Vec<ChangeId>> {
        let dag = self.braid_dag(network).await?;
        let mut heads: Vec<ChangeId> = dag.read().await.heads().into_iter().collect();
        heads.sort();
        Ok(heads)
    }

    async fn braid_dag_subscribe(
        &self,
        network: &IndrasNetwork,
    ) -> Result<broadcast::Receiver<DocumentChange<BraidDag>>> {
        Ok(self.braid_dag(network).await?.subscribe())
    }
}
