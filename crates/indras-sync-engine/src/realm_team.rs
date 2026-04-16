//! `RealmTeam`: extension trait for the team-realm lifecycle on a synced vault.
//!
//! A synced vault's [`VaultFileDocument`](crate::VaultFileDocument) carries a
//! [`Team`](crate::team::Team). When a device hosts one or more logical agents
//! for the team, it must participate in the **team realm** — a separate realm
//! that hosts the braid DAG for this team.
//!
//! # Deterministic derivation
//!
//! Rather than create a realm with a random id on one device and broadcast
//! an invite, the team realm's artifact id is deterministically derived from
//! the synced-vault's id. Every device hosting a team agent independently
//! calls [`RealmTeam::ensure_team_realm`], which hashes the vault id into a
//! stable [`ArtifactId`] and materializes the same interface via
//! [`IndrasNetwork::create_realm_with_artifact`]. Peers converge on that
//! interface through the normal iroh gossip discovery — no invite codes
//! need to flow through the synced document. The same pattern backs
//! `home_realm_id` for cross-device home-realm convergence.

use indras_network::artifact::ArtifactId;
use indras_network::{IndrasNetwork, Realm, RealmId, error::Result};

use crate::realm_vault::RealmVault;

/// Derive the team realm's artifact id from a synced vault's id.
///
/// Deterministic and symmetric across devices — any device that knows the
/// vault id computes the same team artifact id, giving every participant a
/// shared rendezvous on the iroh gossip layer.
pub fn derive_team_artifact_id(vault_realm_id: &RealmId) -> ArtifactId {
    let hex: String = vault_realm_id
        .0
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let input = format!("team-realm-v1:{hex}");
    let hash = blake3::hash(input.as_bytes());
    ArtifactId::Doc(*hash.as_bytes())
}

/// Realm extension trait for team-realm creation and lookup.
#[allow(async_fn_in_trait)]
pub trait RealmTeam {
    /// Ensure the team realm exists on this device, returning its id.
    ///
    /// Derives the team realm's artifact id from this vault's id and
    /// materializes the interface via
    /// [`IndrasNetwork::create_realm_with_artifact`]. Idempotent: calling
    /// twice on the same device is a no-op at the interface layer. Safe
    /// across devices: every device running this with the same vault
    /// converges on the same team realm without invite exchange.
    ///
    /// On the first call (per vault), the returned id is also cached into
    /// the vault document's `team.team_realm_id` for UI convenience.
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
        let artifact_id = derive_team_artifact_id(&self.id());
        let team_realm = network
            .create_realm_with_artifact(artifact_id, team_realm_name)
            .await?;
        let team_realm_id = team_realm.id();

        let idx = self.vault_index().await?;
        if idx.read().await.team.team_realm_id != Some(team_realm_id) {
            idx.update(|doc| {
                doc.team.team_realm_id = Some(team_realm_id);
            })
            .await?;
        }

        Ok(team_realm_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::InterfaceId;

    #[test]
    fn derived_artifact_id_is_stable() {
        let vault = InterfaceId::new([7u8; 32]);
        let a = derive_team_artifact_id(&vault);
        let b = derive_team_artifact_id(&vault);
        assert_eq!(a, b, "same vault id must produce the same team artifact id");
    }

    #[test]
    fn derived_artifact_id_differs_per_vault() {
        let vault1 = InterfaceId::new([1u8; 32]);
        let vault2 = InterfaceId::new([2u8; 32]);
        assert_ne!(
            derive_team_artifact_id(&vault1),
            derive_team_artifact_id(&vault2),
            "different vault ids must produce different team artifact ids"
        );
    }
}
