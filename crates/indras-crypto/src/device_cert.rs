//! Device certificate — a root-signed attestation that a given
//! device belongs to a logical account.
//!
//! Every trusted device for an account has a `DeviceCertificate`
//! on its behalf. At account creation the root signs the original
//! device's cert before scattering itself across stewards. On
//! recovery, the reassembled root signs a *new* cert for the
//! recovering device, which the existing roster then merges. Peers
//! verify new arrivals by checking their cert's signature against
//! the already-known account root `vk`.
//!
//! Certificates are immutable once issued. Revocation is modeled
//! as a separately-signed revocation *record* (see the `revoked`
//! field and [`DeviceCertificate::revoke`]) so merging a roster
//! after a revocation doesn't destabilize prior verifications.

use serde::{Deserialize, Serialize};

use crate::error::CryptoError;
use crate::account_root::AccountRoot;
use crate::pq_identity::{PQPublicIdentity, PQSignature};

/// Root-signed device certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCertificate {
    /// Raw PQ verifying-key bytes of the device this cert speaks for.
    pub device_vk_bytes: Vec<u8>,
    /// Human label — "Truman's MacBook Pro 2026", "Phone", etc.
    pub device_name: String,
    /// Wall-clock millis when this cert was issued.
    pub added_at_millis: i64,
    /// `true` once the root has issued a revocation record for this
    /// cert. The revocation signature overrides the addition signature.
    pub revoked: bool,
    /// Root signature over the canonical byte encoding of
    /// `(device_vk_bytes, device_name, added_at_millis, revoked)`.
    pub signature: Vec<u8>,
}

impl DeviceCertificate {
    /// Produce a fresh certificate signed by `root`.
    pub fn sign(
        device_vk_bytes: Vec<u8>,
        device_name: impl Into<String>,
        added_at_millis: i64,
        root: &AccountRoot,
    ) -> Self {
        let device_name = device_name.into();
        let msg = canonical_message(
            &device_vk_bytes,
            &device_name,
            added_at_millis,
            false,
        );
        let sig = root.sign(&msg);
        Self {
            device_vk_bytes,
            device_name,
            added_at_millis,
            revoked: false,
            signature: sig.to_bytes().to_vec(),
        }
    }

    /// Turn this cert into a root-signed revocation record.
    ///
    /// `revoked_at_millis` supersedes `added_at_millis` for merge
    /// purposes; the canonical message always includes the current
    /// `revoked` flag, so the signature binds to the updated state.
    pub fn revoke(&self, revoked_at_millis: i64, root: &AccountRoot) -> Self {
        let msg = canonical_message(
            &self.device_vk_bytes,
            &self.device_name,
            revoked_at_millis,
            true,
        );
        let sig = root.sign(&msg);
        Self {
            device_vk_bytes: self.device_vk_bytes.clone(),
            device_name: self.device_name.clone(),
            added_at_millis: revoked_at_millis,
            revoked: true,
            signature: sig.to_bytes().to_vec(),
        }
    }

    /// Verify this certificate against the account's root vk.
    ///
    /// Returns `false` on any shape / signature mismatch; intended
    /// to gate peer trust decisions so malformed input fails safely.
    pub fn verify(&self, root_vk: &PQPublicIdentity) -> bool {
        let msg = canonical_message(
            &self.device_vk_bytes,
            &self.device_name,
            self.added_at_millis,
            self.revoked,
        );
        match PQSignature::from_bytes(self.signature.clone()) {
            Ok(sig) => root_vk.verify(&msg, &sig),
            Err(_) => false,
        }
    }

    /// Rehydrate the certified device's public key so peers can
    /// verify signatures *that device* subsequently produces.
    pub fn device_public_key(&self) -> Result<PQPublicIdentity, CryptoError> {
        PQPublicIdentity::from_bytes(&self.device_vk_bytes)
    }

    /// Blake3 digest of the device's verifying key. Matches the
    /// value returned by `PQIdentity::user_id` — handy for looking
    /// the device up in CRDT-indexed stores.
    pub fn device_user_id(&self) -> [u8; 32] {
        *blake3::hash(&self.device_vk_bytes).as_bytes()
    }
}

/// Canonical byte encoding the certificate signature binds to.
/// Stable across the wire + across platforms — fields joined with
/// explicit length prefixes + a domain-separation tag so two
/// fields can't be concatenated into a spoofed message.
fn canonical_message(
    device_vk_bytes: &[u8],
    device_name: &str,
    ts_millis: i64,
    revoked: bool,
) -> Vec<u8> {
    const DOMAIN: &[u8] = b"indras:device-cert:v1";
    let mut out = Vec::with_capacity(
        DOMAIN.len() + 4 + device_vk_bytes.len() + 4 + device_name.len() + 8 + 1,
    );
    out.extend_from_slice(DOMAIN);
    out.extend_from_slice(&(device_vk_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(device_vk_bytes);
    let name_bytes = device_name.as_bytes();
    out.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(name_bytes);
    out.extend_from_slice(&ts_millis.to_le_bytes());
    out.push(if revoked { 1 } else { 0 });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pq_identity::PQIdentity;

    #[test]
    fn root_signed_cert_verifies() {
        let root = AccountRoot::generate();
        let device = PQIdentity::generate();
        let cert = DeviceCertificate::sign(
            device.verifying_key_bytes(),
            "MacBook Pro",
            1_700_000_000_000,
            &root,
        );
        assert!(cert.verify(&root.verifying_key()));
    }

    #[test]
    fn wrong_root_vk_rejects_cert() {
        let root = AccountRoot::generate();
        let other_root = AccountRoot::generate();
        let device = PQIdentity::generate();
        let cert = DeviceCertificate::sign(
            device.verifying_key_bytes(),
            "Phone",
            1_700_000_000_000,
            &root,
        );
        assert!(!cert.verify(&other_root.verifying_key()));
    }

    #[test]
    fn tampered_cert_rejects() {
        let root = AccountRoot::generate();
        let device = PQIdentity::generate();
        let cert = DeviceCertificate::sign(
            device.verifying_key_bytes(),
            "Phone",
            1_700_000_000_000,
            &root,
        );
        let mut forged = cert.clone();
        forged.device_name = "Attacker".into();
        assert!(!forged.verify(&root.verifying_key()));
    }

    #[test]
    fn revocation_self_verifies_and_supersedes() {
        let root = AccountRoot::generate();
        let device = PQIdentity::generate();
        let cert = DeviceCertificate::sign(
            device.verifying_key_bytes(),
            "Lost phone",
            1_700_000_000_000,
            &root,
        );
        let revoked = cert.revoke(1_700_000_100_000, &root);
        assert!(revoked.verify(&root.verifying_key()));
        assert!(revoked.revoked);
        assert_eq!(revoked.added_at_millis, 1_700_000_100_000);
        // Original cert still self-verifies (no mutation).
        assert!(cert.verify(&root.verifying_key()));
        // But a mix-and-match forgery (revoked=true with old signature) does not.
        let mut forged = cert.clone();
        forged.revoked = true;
        assert!(!forged.verify(&root.verifying_key()));
    }

    #[test]
    fn device_user_id_matches_underlying_identity() {
        let root = AccountRoot::generate();
        let device = PQIdentity::generate();
        let cert = DeviceCertificate::sign(
            device.verifying_key_bytes(),
            "x",
            1,
            &root,
        );
        assert_eq!(cert.device_user_id(), device.user_id());
    }
}
