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

use indras_network::{IndrasNetwork, RealmId};

use crate::device_roster::{DeviceRoster, DEVICE_ROSTER_DOC_KEY};

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
