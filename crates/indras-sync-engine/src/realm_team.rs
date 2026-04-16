//! `RealmTeam`: extension trait for the team-realm lifecycle on a synced vault.
//!
//! A synced vault's [`VaultFileDocument`](crate::VaultFileDocument) carries a
//! [`Team`](crate::team::Team). When the team gains its first agent-hosting
//! device, that device must create a **team realm** — a separate realm whose
//! only job is to host the braid DAG for this team. The team realm's id is
//! written back into `VaultFileDocument::team.team_realm_id` so that every
//! device + connection subscribing to the vault learns the id and can choose
//! to join.
//!
//! Race handling: two devices may concurrently create team realms. The
//! [`Team`](crate::team::Team) merge breaks ties byte-lexicographically, so
//! all peers converge on a single id. The losing realm is a cheap dead
//! record; a later subtask can GC it.

use indras_network::{IndrasNetwork, Realm, RealmId, error::Result};

use crate::realm_vault::RealmVault;

/// Realm extension trait for team-realm creation and lookup.
#[allow(async_fn_in_trait)]
pub trait RealmTeam {
    /// Ensure a team realm exists for this synced vault, returning its id.
    ///
    /// If `VaultFileDocument::team.team_realm_id` is already `Some`, returns
    /// that id without creating anything. Otherwise:
    ///
    /// 1. Creates a fresh realm via [`IndrasNetwork::create_realm`].
    /// 2. Writes the new id into the vault document's team field.
    /// 3. Returns the id the document ultimately holds after merge (which
    ///    may differ from the created id if a peer concurrently set a
    ///    lower-byte id).
    async fn ensure_team_realm(
        &self,
        network: &IndrasNetwork,
        team_realm_name: &str,
    ) -> Result<RealmId>;
}

impl RealmTeam for Realm {
    async fn ensure_team_realm(
        &self,
        network: &IndrasNetwork,
        team_realm_name: &str,
    ) -> Result<RealmId> {
        let idx = self.vault_index().await?;
        if let Some(existing) = idx.read().await.team.team_realm_id {
            return Ok(existing);
        }

        let new_realm = network.create_realm(team_realm_name).await?;
        let new_id = new_realm.id();

        idx.update(|doc| {
            if doc.team.team_realm_id.is_none() {
                doc.team.team_realm_id = Some(new_id);
            }
        })
        .await?;

        Ok(idx.read().await.team.team_realm_id.unwrap_or(new_id))
    }
}
