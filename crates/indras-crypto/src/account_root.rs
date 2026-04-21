//! Account root — the long-lived PQ keypair that certifies devices.
//!
//! Conceptually distinct from a `PQIdentity` (which represents a
//! single *device's* identity), the `AccountRoot` is the signing
//! authority for an entire logical account. Its `vk` is published
//! once at account creation and verified by peers forever after.
//! Its `sk` is split across stewards at enrollment and then
//! zeroized on-device — it only materializes briefly on a new
//! device during recovery long enough to sign one
//! [`DeviceCertificate`](crate::device_cert::DeviceCertificate),
//! then vanishes again.
//!
//! This module is a semantic wrapper around [`PQIdentity`]: same
//! Dilithium3 primitives, different lifecycle.

use serde::{Deserialize, Serialize};

use crate::error::CryptoError;
use crate::pq_identity::{PQIdentity, PQPublicIdentity, PQSignature, SecureBytes};

/// The account's root signing authority. Generated once and
/// eventually distributed among stewards as Shamir shares.
#[derive(Clone)]
pub struct AccountRoot {
    inner: PQIdentity,
}

impl AccountRoot {
    /// Generate a fresh root. Called exactly once per account at
    /// creation time; the `sk` is split across stewards soon after
    /// and then dropped.
    pub fn generate() -> Self {
        Self {
            inner: PQIdentity::generate(),
        }
    }

    /// Rebuild a root from already-known keypair bytes. Used on a
    /// recovering device after Shamir assembly reconstructs the
    /// signing-key bytes.
    pub fn from_keypair_bytes(sk: &[u8], vk: &[u8]) -> Result<Self, CryptoError> {
        Ok(Self {
            inner: PQIdentity::from_keypair_bytes(sk, vk)?,
        })
    }

    /// Sign a message under the root authority. Intended to be
    /// called exactly once per recovery — typically to stamp a
    /// fresh `DeviceCertificate` — before the root is zeroized.
    pub fn sign(&self, message: &[u8]) -> PQSignature {
        self.inner.sign(message)
    }

    /// Export the root's verifying key. Public information —
    /// broadcast to peers so they can verify signatures produced
    /// with this root.
    pub fn verifying_key(&self) -> PQPublicIdentity {
        self.inner.verifying_key()
    }

    /// Raw public-key bytes, for embedding in profile documents
    /// and device certificates.
    pub fn verifying_key_bytes(&self) -> Vec<u8> {
        self.inner.verifying_key_bytes()
    }

    /// Zeroizing signing-key bytes. The caller is expected to
    /// Shamir-split these and immediately drop the resulting
    /// [`SecureBytes`] instance.
    pub fn signing_key_bytes(&self) -> SecureBytes {
        self.inner.signing_key_bytes()
    }

    /// Export full keypair bytes. Used transiently during recovery
    /// assembly to reconstruct the root from Shamir shares +
    /// separately-broadcast `vk`.
    pub fn to_keypair_bytes(&self) -> (SecureBytes, Vec<u8>) {
        self.inner.to_keypair_bytes()
    }

    /// Blake3 digest of the verifying key — a 32-byte identifier
    /// that peers can reference without carrying the full 1,952-
    /// byte `vk`.
    pub fn root_id(&self) -> [u8; 32] {
        self.inner.user_id()
    }
}

impl std::fmt::Debug for AccountRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountRoot")
            .field("root_id_prefix", &hex::encode(&self.root_id()[..8]))
            .finish_non_exhaustive()
    }
}

// Note: signing-key material is zeroized on drop via the wrapped
// `PQIdentity`, which itself carries zeroize-on-drop semantics.
// We intentionally don't implement `Zeroize` because dilithium3
// keygen is slow enough that forcing callers into a regenerate-
// -then-drop pattern is costlier than letting the natural drop
// path run. Narrow the exposure window by dropping `AccountRoot`
// values as soon as the sign-once operation completes.

/// A lightweight reference to an account's root, safe to embed in
/// CRDT docs and profile entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRootRef {
    /// Blake3 of the `vk` — stable 32-byte identifier.
    pub root_id: [u8; 32],
    /// Full verifying-key bytes for peer-side signature checks.
    pub verifying_key_bytes: Vec<u8>,
}

impl AccountRootRef {
    /// Take a snapshot of an `AccountRoot`'s public state.
    pub fn from_root(root: &AccountRoot) -> Self {
        Self {
            root_id: root.root_id(),
            verifying_key_bytes: root.verifying_key_bytes(),
        }
    }

    /// Rehydrate a `PQPublicIdentity` for verification. Returns
    /// `None` when the stored bytes are malformed (e.g. a corrupted
    /// profile document).
    pub fn public(&self) -> Option<PQPublicIdentity> {
        PQPublicIdentity::from_bytes(&self.verifying_key_bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_unique_roots() {
        let a = AccountRoot::generate();
        let b = AccountRoot::generate();
        assert_ne!(a.root_id(), b.root_id());
    }

    #[test]
    fn sign_then_verify_with_own_vk() {
        let root = AccountRoot::generate();
        let msg = b"authorize device cert";
        let sig = root.sign(msg);
        assert!(root.verifying_key().verify(msg, &sig));
    }

    #[test]
    fn wrong_message_fails_verification() {
        let root = AccountRoot::generate();
        let sig = root.sign(b"one");
        assert!(!root.verifying_key().verify(b"two", &sig));
    }

    #[test]
    fn from_keypair_bytes_roundtrips() {
        let original = AccountRoot::generate();
        let (sk, vk) = original.to_keypair_bytes();
        let rebuilt = AccountRoot::from_keypair_bytes(sk.as_slice(), &vk).unwrap();
        assert_eq!(original.root_id(), rebuilt.root_id());
        let msg = b"across the divide";
        let sig = rebuilt.sign(msg);
        assert!(original.verifying_key().verify(msg, &sig));
    }

    #[test]
    fn account_root_ref_rehydrates_to_verifier() {
        let root = AccountRoot::generate();
        let r = AccountRootRef::from_root(&root);
        assert_eq!(r.root_id, root.root_id());
        let pk = r.public().unwrap();
        let sig = root.sign(b"sync");
        assert!(pk.verify(b"sync", &sig));
    }
}
