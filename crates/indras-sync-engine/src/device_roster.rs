//! Device roster — the CRDT doc listing every device trusted by an
//! account.
//!
//! Lives in the account's *home* realm under the well-known key
//! [`DEVICE_ROSTER_DOC_KEY`]. Every member of a shared realm who
//! knows the account's root `vk` can verify a device by looking it
//! up in this doc and checking the cert's signature.
//!
//! Merge strategy: per-device entries compose by "later timestamp
//! wins" so a revocation signed later supersedes the original
//! addition. The root reference is sticky — once set, it persists
//! (a fresh root would invalidate every cert, so rotation is a
//! breaking account change, not a merge-time one).

use serde::{Deserialize, Serialize};

use indras_crypto::account_root::AccountRootRef;
use indras_crypto::device_cert::DeviceCertificate;
use indras_network::document::DocumentSchema;

/// Document name every device uses to write / read the roster in
/// the home realm.
pub const DEVICE_ROSTER_DOC_KEY: &str = "_device_roster";

/// Account-wide device roster.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceRoster {
    /// Snapshot of the root `vk` + id. Readers use this to verify
    /// each cert's signature. `None` before account creation
    /// finalizes the root (shouldn't normally be observed).
    pub account_root_ref: Option<AccountRootRef>,
    /// One entry per device. Revoked entries stay in the list so a
    /// lagging peer can't "re-admit" a revoked device just by
    /// gossiping its old cert back in.
    pub devices: Vec<DeviceCertificate>,
}

impl DeviceRoster {
    /// Upsert a cert into the roster, preserving the newer
    /// timestamp on conflict. Public so the sender can pre-compose
    /// the CRDT update locally before calling `doc.update`.
    pub fn upsert(&mut self, cert: DeviceCertificate) {
        if let Some(existing) = self
            .devices
            .iter_mut()
            .find(|c| c.device_vk_bytes == cert.device_vk_bytes)
        {
            if cert.added_at_millis > existing.added_at_millis {
                *existing = cert;
            }
        } else {
            self.devices.push(cert);
        }
    }

    /// Find the (possibly revoked) cert for the device with the
    /// given verifying-key bytes.
    pub fn cert_for(&self, device_vk_bytes: &[u8]) -> Option<&DeviceCertificate> {
        self.devices
            .iter()
            .find(|c| c.device_vk_bytes == device_vk_bytes)
    }

    /// `true` when the roster contains a non-revoked, root-signed
    /// cert for the specified device. Verifies the signature
    /// against the embedded `account_root_ref`.
    pub fn device_is_trusted(&self, device_vk_bytes: &[u8]) -> bool {
        let Some(cert) = self.cert_for(device_vk_bytes) else {
            return false;
        };
        if cert.revoked {
            return false;
        }
        let Some(ref root_ref) = self.account_root_ref else {
            return false;
        };
        let Some(pk) = root_ref.public() else {
            return false;
        };
        cert.verify(&pk)
    }

    /// Non-revoked devices — the active roster.
    pub fn active_devices(&self) -> impl Iterator<Item = &DeviceCertificate> {
        self.devices.iter().filter(|c| !c.revoked)
    }
}

impl DocumentSchema for DeviceRoster {
    fn merge(&mut self, remote: Self) {
        // Root ref is sticky — prefer the first-seen; a conflict
        // means two different roots claim the same account, which
        // should surface as a verification failure downstream, not
        // a silent overwrite.
        if self.account_root_ref.is_none() {
            self.account_root_ref = remote.account_root_ref.clone();
        }
        for cert in remote.devices {
            self.upsert(cert);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_crypto::account_root::AccountRoot;
    use indras_crypto::pq_identity::PQIdentity;

    fn make_cert(root: &AccountRoot, name: &str, ts: i64) -> (PQIdentity, DeviceCertificate) {
        let device = PQIdentity::generate();
        let cert = DeviceCertificate::sign(
            device.verifying_key_bytes(),
            name,
            ts,
            root,
        );
        (device, cert)
    }

    #[test]
    fn upsert_adds_then_updates_on_later_timestamp() {
        let root = AccountRoot::generate();
        let (device, cert_v1) = make_cert(&root, "Laptop", 100);
        let cert_v2 = cert_v1.revoke(200, &root);

        let mut roster = DeviceRoster {
            account_root_ref: Some(AccountRootRef::from_root(&root)),
            devices: Vec::new(),
        };
        roster.upsert(cert_v1.clone());
        assert_eq!(roster.devices.len(), 1);
        assert!(!roster.devices[0].revoked);

        roster.upsert(cert_v2.clone());
        assert_eq!(roster.devices.len(), 1);
        assert!(roster.devices[0].revoked);

        // Older cert loses to newer state.
        roster.upsert(cert_v1);
        assert!(roster.devices[0].revoked);
        assert_eq!(
            roster.cert_for(&device.verifying_key_bytes()).unwrap().added_at_millis,
            200
        );
    }

    #[test]
    fn merge_unions_per_device_and_carries_root_ref() {
        let root = AccountRoot::generate();
        let (_d1, cert1) = make_cert(&root, "A", 100);
        let (_d2, cert2) = make_cert(&root, "B", 150);
        let root_ref = AccountRootRef::from_root(&root);

        let mut a = DeviceRoster {
            account_root_ref: Some(root_ref.clone()),
            devices: vec![cert1.clone()],
        };
        let b = DeviceRoster {
            account_root_ref: Some(root_ref),
            devices: vec![cert2.clone()],
        };
        a.merge(b);
        assert_eq!(a.devices.len(), 2);
        assert!(a.account_root_ref.is_some());
    }

    #[test]
    fn device_is_trusted_checks_signature_and_revocation() {
        let root = AccountRoot::generate();
        let (device, cert) = make_cert(&root, "Laptop", 100);
        let mut roster = DeviceRoster {
            account_root_ref: Some(AccountRootRef::from_root(&root)),
            devices: vec![cert.clone()],
        };
        assert!(roster.device_is_trusted(&device.verifying_key_bytes()));

        // Revoke and re-assert false.
        let revoked = cert.revoke(200, &root);
        roster.upsert(revoked);
        assert!(!roster.device_is_trusted(&device.verifying_key_bytes()));

        // Unknown device is not trusted.
        let stranger = PQIdentity::generate();
        assert!(!roster.device_is_trusted(&stranger.verifying_key_bytes()));
    }

    #[test]
    fn trusted_check_fails_when_signature_does_not_match_root() {
        let real_root = AccountRoot::generate();
        let fake_root = AccountRoot::generate();
        let (device, cert_by_fake_root) = make_cert(&fake_root, "Impostor", 100);

        let roster = DeviceRoster {
            account_root_ref: Some(AccountRootRef::from_root(&real_root)),
            devices: vec![cert_by_fake_root],
        };
        assert!(!roster.device_is_trusted(&device.verifying_key_bytes()));
    }
}
