//! Recovery request / share release CRDT docs.
//!
//! When a user on a fresh device wants to recover their account, the
//! device publishes a [`RecoveryRequest`] into each selected
//! steward's DM realm under [`recovery_request_doc_key`]. The
//! steward's device surfaces the request in its inbox UI; after
//! out-of-band verification the steward approves, which re-wraps
//! their held Shamir share against the new device's KEM ek and
//! publishes a [`ShareRelease`] under
//! [`share_release_doc_key`]. The new device polls the release doc
//! across every DM realm and, once K arrive, recombines the subkey.
//!
//! Both doc keys are suffixed with the *new device's* `UserId` so
//! concurrent recoveries across unrelated peers don't collide. Only
//! one active recovery per new device is expected; re-initiating
//! overwrites the prior request via last-writer-wins.

use serde::{Deserialize, Serialize};

use indras_network::document::DocumentSchema;

/// Key prefix for a recovery request doc.
pub const RECOVERY_REQUEST_KEY_PREFIX: &str = "_recovery_request:";

/// Key prefix for a share release doc.
pub const SHARE_RELEASE_KEY_PREFIX: &str = "_share_release:";

/// Doc key for the recovery request the new device publishes into
/// each selected steward's DM realm. Suffixed with the new device's
/// UID so stewards can identify the requester across realms.
pub fn recovery_request_doc_key(new_device_uid: &[u8; 32]) -> String {
    format!("{}{}", RECOVERY_REQUEST_KEY_PREFIX, hex::encode(new_device_uid))
}

/// Doc key a steward writes to release their (re-wrapped) share
/// back to the new device. Lives in the same DM realm as the
/// request.
pub fn share_release_doc_key(new_device_uid: &[u8; 32]) -> String {
    format!("{}{}", SHARE_RELEASE_KEY_PREFIX, hex::encode(new_device_uid))
}

/// Request issued by a fresh device asking a steward to release
/// their share of the requester's old account's encryption subkey.
///
/// The request carries the new device's PQ identity + fresh KEM ek
/// so the steward can encrypt the released share directly without
/// a separate directory lookup.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoveryRequest {
    /// The new device's `UserId` (blake3 of its PQ verifying key).
    pub new_device_uid: [u8; 32],
    /// Human label the new device is presenting. Steward UIs
    /// render this verbatim in the approval dialog.
    pub new_device_display_name: String,
    /// Fresh ML-KEM-768 encapsulation key the steward will wrap
    /// their share to.
    pub new_device_kem_ek: Vec<u8>,
    /// New device's DSA verifying key, echoed so the steward can
    /// cross-reference identity before approving.
    pub new_device_vk: Vec<u8>,
    /// Wall-clock millis when the request was issued.
    pub issued_at_millis: i64,
    /// `true` once the new device has retracted the request.
    /// Stewards stop surfacing withdrawn requests.
    pub withdrawn: bool,
}

impl DocumentSchema for RecoveryRequest {
    fn merge(&mut self, remote: Self) {
        if remote.issued_at_millis > self.issued_at_millis {
            *self = remote;
        }
    }
}

/// A steward's approval: their Shamir share, re-wrapped to the new
/// device's KEM ek, plus metadata identifying which source account
/// the share belongs to.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShareRelease {
    /// Responding steward's `UserId`.
    pub steward_uid: [u8; 32],
    /// `UserId` of the account whose subkey is being recovered.
    /// Collected by the new device across K releases to confirm
    /// they all target the same source identity.
    pub source_account_uid: [u8; 32],
    /// Fresh `EncryptedStewardShare::to_bytes` with the Shamir
    /// share encrypted to the new device's KEM ek.
    pub encrypted_share_bytes: Vec<u8>,
    /// Wall-clock millis of approval.
    pub approved_at_millis: i64,
}

impl DocumentSchema for ShareRelease {
    fn merge(&mut self, remote: Self) {
        if remote.approved_at_millis > self.approved_at_millis {
            *self = remote;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_keys_stable_and_distinct() {
        let uid = [0x5au8; 32];
        let r1 = recovery_request_doc_key(&uid);
        let r2 = recovery_request_doc_key(&uid);
        let s1 = share_release_doc_key(&uid);
        assert_eq!(r1, r2);
        assert_ne!(r1, s1);
        assert!(r1.starts_with(RECOVERY_REQUEST_KEY_PREFIX));
        assert!(s1.starts_with(SHARE_RELEASE_KEY_PREFIX));
    }

    #[test]
    fn recovery_request_merge_prefers_newer() {
        let older = RecoveryRequest {
            new_device_uid: [1u8; 32],
            new_device_display_name: "old".into(),
            new_device_kem_ek: vec![1],
            new_device_vk: vec![2],
            issued_at_millis: 100,
            withdrawn: false,
        };
        let newer = RecoveryRequest {
            new_device_display_name: "new".into(),
            issued_at_millis: 500,
            ..older.clone()
        };
        let mut a = older.clone();
        a.merge(newer.clone());
        assert_eq!(a.new_device_display_name, "new");
        let mut b = newer.clone();
        b.merge(older);
        assert_eq!(b.issued_at_millis, 500);
    }

    #[test]
    fn share_release_merge_prefers_newer() {
        let a = ShareRelease {
            steward_uid: [9u8; 32],
            source_account_uid: [8u8; 32],
            encrypted_share_bytes: vec![1, 2, 3],
            approved_at_millis: 10,
        };
        let b = ShareRelease {
            encrypted_share_bytes: vec![4, 5, 6],
            approved_at_millis: 20,
            ..a.clone()
        };
        let mut m = a.clone();
        m.merge(b.clone());
        assert_eq!(m.approved_at_millis, 20);
        assert_eq!(m.encrypted_share_bytes, vec![4, 5, 6]);
    }
}
