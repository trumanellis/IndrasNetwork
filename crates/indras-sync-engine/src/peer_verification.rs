//! Verify a claimed device against an account's `DeviceRoster`.
//!
//! Two-step verification:
//! 1. Locate the account's home-realm `DeviceRoster` doc (by realm id
//!    or via a DM peer reference).
//! 2. Call [`DeviceRoster::device_is_trusted`] which itself checks
//!    the cert's signature against the embedded `AccountRootRef`.
//!
//! Higher-level DM- / shared-realm-admission logic (elsewhere) can
//! call these helpers once it knows which account is claiming to be
//! on the other end of the handshake. This module intentionally
//! stays small and side-effect-free so it can be dropped into
//! whichever layer eventually makes the trust decision.

use std::sync::Arc;

use indras_network::home_realm::home_realm_id;
use indras_network::{IndrasNetwork, MemberId, RealmId};

use crate::device_roster::{DeviceRoster, DEVICE_ROSTER_DOC_KEY};
use crate::peer_key_directory::PeerKeyDirectory;

/// Check whether a device's PQ verifying-key bytes are trusted by
/// the account whose home realm is `account_home_realm_id`.
///
/// Returns `false` on any failure: realm unavailable, roster
/// missing, cert absent, signature invalid, or cert revoked.
/// Callers gating peer admission should fail closed.
pub async fn verify_peer_device(
    network: &Arc<IndrasNetwork>,
    account_home_realm_id: &RealmId,
    device_vk_bytes: &[u8],
) -> bool {
    let Some(realm) = network.get_realm_by_id(account_home_realm_id) else {
        return false;
    };
    let Ok(doc) = realm.document::<DeviceRoster>(DEVICE_ROSTER_DOC_KEY).await else {
        return false;
    };
    let roster = doc.read().await.clone();
    roster.device_is_trusted(device_vk_bytes)
}

/// Load the full `DeviceRoster` for an account. Handy for UI
/// surfaces ("here's the list of devices on this account") or for
/// inspection. Returns `None` when the roster doc is missing or
/// the realm is unreachable.
pub async fn load_device_roster(
    network: &Arc<IndrasNetwork>,
    account_home_realm_id: &RealmId,
) -> Option<DeviceRoster> {
    let realm = network.get_realm_by_id(account_home_realm_id)?;
    let doc = realm
        .document::<DeviceRoster>(DEVICE_ROSTER_DOC_KEY)
        .await
        .ok()?;
    Some(doc.read().await.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_crypto::account_root::{AccountRoot, AccountRootRef};
    use indras_crypto::device_cert::DeviceCertificate;
    use indras_crypto::pq_identity::PQIdentity;

    /// Pure-function test that the trust check on a locally-
    /// constructed roster behaves consistently with the Realm-
    /// fetched path. Covers the primitives we built in B.3/B.2
    /// without needing a real IndrasNetwork.
    #[test]
    fn trusts_valid_cert_rejects_revoked_and_unknown() {
        let root = AccountRoot::generate();
        let device = PQIdentity::generate();
        let cert = DeviceCertificate::sign(
            device.verifying_key_bytes(),
            "trusted",
            1_700_000_000_000,
            &root,
        );
        let mut roster = DeviceRoster {
            account_root_ref: Some(AccountRootRef::from_root(&root)),
            devices: vec![cert.clone()],
        };
        assert!(roster.device_is_trusted(&device.verifying_key_bytes()));

        let stranger = PQIdentity::generate();
        assert!(!roster.device_is_trusted(&stranger.verifying_key_bytes()));

        let revoked = cert.revoke(1_700_000_100_000, &root);
        roster.upsert(revoked);
        assert!(!roster.device_is_trusted(&device.verifying_key_bytes()));
    }
}

/// Given a peer we share a DM realm with, verify that their claimed
/// PQ identity (UserId) has a trusted, non-revoked device cert in
/// the peer's own account-home `DeviceRoster`.
///
/// Returns `false` whenever any link in the chain is missing:
/// - peer hasn't published their PQ verifying key into the DM
///   realm's peer-keys directory,
/// - peer's home realm hasn't been reached by this node yet,
/// - the roster doc isn't present or has an empty `account_root_ref`,
/// - the cert is revoked or missing.
///
/// Caller semantics are "fail closed" — skip the claim on `false`
/// and retry the next time state updates.
pub async fn gate_peer_via_home_roster(
    network: &Arc<IndrasNetwork>,
    dm_realm_id: &RealmId,
    peer_member_id: &MemberId,
    peer_user_id: &[u8; 32],
) -> bool {
    // 1. Fetch the peer's PQ verifying-key bytes from the DM realm's
    //    peer-keys directory (published at vault setup).
    let Some(dm_realm) = network.get_realm_by_id(dm_realm_id) else {
        return false;
    };
    let Ok(key_dir) = dm_realm.document::<PeerKeyDirectory>("peer-keys").await else {
        return false;
    };
    let vk_bytes = {
        let snap = key_dir.read().await;
        match snap.get(peer_user_id) {
            Some(pk) => pk.to_bytes(),
            None => return false,
        }
    };

    // 2. Compute the peer's home-realm id and pull their roster.
    let home_id = home_realm_id(*peer_member_id);
    verify_peer_device(network, &home_id, &vk_bytes).await
}
