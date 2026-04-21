//! Encrypted envelope for the account root's signing key.
//!
//! The root `sk` is ~4000 bytes (Dilithium3), too large for a direct
//! Shamir split (our Shamir primitive operates on a fixed 32-byte
//! secret). Instead we use classic key escrow:
//!
//! 1. Generate a 32-byte random wrapping key `W`.
//! 2. Encrypt root `sk` with `W` → ciphertext.
//! 3. Publish ciphertext + root `vk` as an [`AccountRootEnvelope`]
//!    CRDT doc in the home realm.
//! 4. Shamir-split `W` across stewards (reusing the existing
//!    K-of-N pipeline).
//!
//! On recovery, K stewards release their share of `W`. The new
//! device reassembles `W`, fetches the envelope doc from the home
//! realm (via standard CRDT sync), and decrypts root `sk`. The root
//! is used exactly once to sign the new device's certificate, then
//! dropped.
//!
//! The envelope is a public doc — anyone who can see the home realm
//! can read it. That's fine because `W` is the actual secret; the
//! envelope without `W` is no easier to break than Dilithium itself.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305,
};
use serde::{Deserialize, Serialize};

use indras_crypto::account_root::AccountRoot;
use indras_network::document::DocumentSchema;

/// CRDT doc key for the envelope in the home realm.
pub const ACCOUNT_ROOT_ENVELOPE_DOC_KEY: &str = "_account_root_envelope";

/// Published envelope — ciphertext + root vk + a monotonic version
/// so re-splits cleanly supersede earlier envelopes on merge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountRootEnvelope {
    /// ChaCha20-Poly1305 ciphertext of the root's signing-key bytes.
    /// Empty before first publish.
    pub encrypted_sk: Vec<u8>,
    /// 12-byte nonce used for the encryption.
    pub nonce: Vec<u8>,
    /// Public verifying-key bytes — the same value in every
    /// envelope version for a given root.
    pub verifying_key_bytes: Vec<u8>,
    /// Version bumped on every re-split. Merge picks the newest.
    pub version: u64,
}

impl DocumentSchema for AccountRootEnvelope {
    fn merge(&mut self, remote: Self) {
        if remote.version > self.version {
            *self = remote;
        }
    }
}

/// Errors from envelope sealing / unsealing.
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    #[error("wrapping key must be exactly 32 bytes")]
    InvalidKeyLength,
    #[error("encryption failed: {0}")]
    Encrypt(String),
    #[error("decryption failed — wrong key or corrupted envelope")]
    Decrypt,
    #[error("nonce must be exactly 12 bytes")]
    InvalidNonce,
    #[error("crypto error: {0}")]
    Crypto(String),
}

/// Seal an account root under a 32-byte wrapping key.
pub fn seal_account_root(
    root: &AccountRoot,
    wrapping_key: &[u8; 32],
    version: u64,
) -> Result<AccountRootEnvelope, EnvelopeError> {
    let cipher = ChaCha20Poly1305::new_from_slice(wrapping_key)
        .map_err(|_| EnvelopeError::InvalidKeyLength)?;
    // Deterministic nonce derived from version keeps re-seals with
    // the same key safely distinct (the wrapping key rotates on
    // every re-split anyway, so reuse risk is theoretical).
    let mut nonce = [0u8; 12];
    nonce[..8].copy_from_slice(&version.to_le_bytes());
    nonce[8..].copy_from_slice(&[0xa5; 4]);
    let (sk, vk) = root.to_keypair_bytes();
    let ciphertext = cipher
        .encrypt(nonce.as_ref().into(), sk.as_slice())
        .map_err(|e| EnvelopeError::Encrypt(format!("{e}")))?;
    Ok(AccountRootEnvelope {
        encrypted_sk: ciphertext,
        nonce: nonce.to_vec(),
        verifying_key_bytes: vk,
        version,
    })
}

/// Unseal an envelope back into an `AccountRoot`. The `vk` in the
/// envelope anchors the returned root, so a corrupt envelope can't
/// silently substitute a different key.
pub fn unseal_account_root(
    envelope: &AccountRootEnvelope,
    wrapping_key: &[u8; 32],
) -> Result<AccountRoot, EnvelopeError> {
    if envelope.nonce.len() != 12 {
        return Err(EnvelopeError::InvalidNonce);
    }
    let cipher = ChaCha20Poly1305::new_from_slice(wrapping_key)
        .map_err(|_| EnvelopeError::InvalidKeyLength)?;
    let sk = cipher
        .decrypt(envelope.nonce.as_slice().into(), envelope.encrypted_sk.as_slice())
        .map_err(|_| EnvelopeError::Decrypt)?;
    AccountRoot::from_keypair_bytes(&sk, &envelope.verifying_key_bytes)
        .map_err(|e| EnvelopeError::Crypto(format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_seal_then_unseal() {
        let root = AccountRoot::generate();
        let key = [0x42u8; 32];
        let env = seal_account_root(&root, &key, 1).unwrap();
        let back = unseal_account_root(&env, &key).unwrap();
        assert_eq!(back.root_id(), root.root_id());
        let sig = back.sign(b"hello");
        assert!(root.verifying_key().verify(b"hello", &sig));
    }

    #[test]
    fn wrong_key_fails_unseal() {
        let root = AccountRoot::generate();
        let key = [0x11u8; 32];
        let wrong = [0x22u8; 32];
        let env = seal_account_root(&root, &key, 1).unwrap();
        let err = unseal_account_root(&env, &wrong).unwrap_err();
        assert!(matches!(err, EnvelopeError::Decrypt));
    }

    #[test]
    fn merge_prefers_newer_version() {
        let mut a = AccountRootEnvelope {
            encrypted_sk: vec![1],
            nonce: vec![0; 12],
            verifying_key_bytes: vec![9],
            version: 1,
        };
        let b = AccountRootEnvelope {
            encrypted_sk: vec![2],
            nonce: vec![0; 12],
            verifying_key_bytes: vec![9],
            version: 5,
        };
        a.merge(b);
        assert_eq!(a.version, 5);
        assert_eq!(a.encrypted_sk, vec![2]);
    }
}
