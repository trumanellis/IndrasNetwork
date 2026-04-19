//! Encrypted Shamir shares for steward-based key recovery.
//!
//! Wraps a `ShamirShare` for delivery to one steward by encapsulating
//! a fresh shared secret to the steward's ML-KEM-768 public key
//! ([`PQEncapsulationKey`]) and using that secret to encrypt the
//! share's bytes with ChaCha20-Poly1305 — the same envelope shape used
//! by [`crate::interface_key::InterfaceKey::encapsulate_for`].
//!
//! On the recovery side, the steward decapsulates with their
//! [`PQKemKeyPair`] to recover the same shared secret, then decrypts
//! the share. Once the threshold of decrypted shares is collected, the
//! original 32-byte secret is reassembled with [`crate::shamir::combine_shares`].
//!
//! # Wire format
//!
//! Use [`EncryptedStewardShare::to_bytes`] / [`EncryptedStewardShare::from_bytes`]
//! for postcard-encoded transport. Each encrypted share carries the
//! metadata a steward needs to (a) route to the right recipient by KEM
//! id, (b) know which split version it belongs to, and (c) know the
//! threshold and share index without decrypting first.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::{CryptoError, CryptoResult};
use crate::interface_key::NONCE_SIZE;
use crate::pq_kem::{PQCiphertext, PQEncapsulationKey, PQKemKeyPair};
use crate::shamir::ShamirShare;

/// Length of the short routing identifier derived from a recipient's
/// encapsulation key (first 8 bytes).
pub const RECIPIENT_KEM_ID_LEN: usize = 8;

/// A single Shamir share encrypted to one steward's ML-KEM-768 public key.
///
/// Carries enough cleartext metadata for routing and reassembly without
/// exposing the underlying share. The share itself is recoverable only
/// by the holder of the matching [`PQKemKeyPair`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedStewardShare {
    /// X-coordinate (1..=255) of the underlying Shamir share. Allows
    /// the recovery coordinator to detect duplicate shares before
    /// attempting decryption.
    pub share_index: u8,

    /// Threshold (K) required to recombine. Steward UIs can show this
    /// to the user (e.g. "3 of 5 shares needed").
    pub threshold: u8,

    /// Monotonic version of the underlying secret split. Increments
    /// when the user rotates the master secret and re-issues shares.
    /// Stewards holding old versions can be pruned.
    pub secret_version: u64,

    /// First [`RECIPIENT_KEM_ID_LEN`] bytes of the steward's
    /// encapsulation key, for cleartext routing.
    pub recipient_kem_id: [u8; RECIPIENT_KEM_ID_LEN],

    /// ML-KEM-768 ciphertext (~1,088 bytes) carrying the encapsulated
    /// shared secret used to encrypt `encrypted_share`.
    pub kem_ciphertext: Vec<u8>,

    /// ChaCha20-Poly1305 nonce.
    pub nonce: [u8; NONCE_SIZE],

    /// ChaCha20-Poly1305 ciphertext of the postcard-encoded share.
    pub encrypted_share: Vec<u8>,
}

impl EncryptedStewardShare {
    /// Decrypt this share using the recipient's KEM keypair.
    ///
    /// Returns an error if the KEM ciphertext is malformed, the
    /// recipient is not the intended one (Kyber's implicit rejection
    /// produces a different shared secret, which causes ChaCha20-Poly1305
    /// authentication to fail), or the inner share bytes are corrupt.
    pub fn decrypt(&self, recipient: &PQKemKeyPair) -> CryptoResult<ShamirShare> {
        let kem_ct = PQCiphertext::from_bytes(self.kem_ciphertext.clone())?;
        let shared_secret = recipient.decapsulate(&kem_ct)?;

        let cipher = ChaCha20Poly1305::new_from_slice(&shared_secret)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;
        let nonce = Nonce::from_slice(&self.nonce);

        let plaintext = cipher
            .decrypt(nonce, self.encrypted_share.as_slice())
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        let share: ShamirShare = postcard::from_bytes(&plaintext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        Ok(share)
    }

    /// Serialize for storage or wire transport.
    pub fn to_bytes(&self) -> CryptoResult<Vec<u8>> {
        postcard::to_allocvec(self).map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> CryptoResult<Self> {
        postcard::from_bytes(bytes).map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

/// Encrypt a single Shamir share to a steward's encapsulation key.
///
/// `secret_version` should be incremented by the caller whenever the
/// underlying master secret is rotated, so old shares can be safely
/// discarded. `threshold` is recorded for the steward's awareness.
pub fn encrypt_share_for_steward(
    share: &ShamirShare,
    threshold: u8,
    secret_version: u64,
    recipient: &PQEncapsulationKey,
) -> CryptoResult<EncryptedStewardShare> {
    let (kem_ciphertext, shared_secret) = recipient.encapsulate();

    let cipher = ChaCha20Poly1305::new_from_slice(&shared_secret)
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = postcard::to_allocvec(share)
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    let encrypted_share = cipher
        .encrypt(nonce, plaintext.as_slice())
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    let recipient_bytes = recipient.to_bytes();
    let mut recipient_kem_id = [0u8; RECIPIENT_KEM_ID_LEN];
    let copy_len = recipient_kem_id.len().min(recipient_bytes.len());
    recipient_kem_id[..copy_len].copy_from_slice(&recipient_bytes[..copy_len]);

    Ok(EncryptedStewardShare {
        share_index: share.index(),
        threshold,
        secret_version,
        recipient_kem_id,
        kem_ciphertext: kem_ciphertext.into_bytes(),
        nonce: nonce_bytes,
        encrypted_share,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shamir;

    fn sample_secret() -> [u8; shamir::SHAMIR_SECRET_SIZE] {
        let mut s = [0u8; shamir::SHAMIR_SECRET_SIZE];
        for (i, b) in s.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(11).wrapping_add(5);
        }
        s
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret = sample_secret();
        let shares = shamir::split_secret(&secret, 3, 5).unwrap();

        let steward = PQKemKeyPair::generate();
        let encrypted =
            encrypt_share_for_steward(&shares[0], 3, 1, &steward.encapsulation_key()).unwrap();

        assert_eq!(encrypted.share_index, shares[0].index());
        assert_eq!(encrypted.threshold, 3);
        assert_eq!(encrypted.secret_version, 1);

        let recovered = encrypted.decrypt(&steward).unwrap();
        // Share comparison: serialize both sides and compare bytes.
        assert_eq!(recovered.to_bytes(), shares[0].to_bytes());
    }

    #[test]
    fn test_wrong_recipient_cannot_decrypt() {
        let secret = sample_secret();
        let shares = shamir::split_secret(&secret, 3, 5).unwrap();

        let intended = PQKemKeyPair::generate();
        let attacker = PQKemKeyPair::generate();
        let encrypted =
            encrypt_share_for_steward(&shares[0], 3, 1, &intended.encapsulation_key()).unwrap();

        let result = encrypted.decrypt(&attacker);
        assert!(
            matches!(result, Err(CryptoError::DecryptionFailed(_))),
            "expected decryption failure, got {:?}",
            result
        );
    }

    #[test]
    fn test_full_split_distribute_recover_flow() {
        let secret = sample_secret();
        let shares = shamir::split_secret(&secret, 3, 5).unwrap();

        let stewards: Vec<PQKemKeyPair> = (0..5).map(|_| PQKemKeyPair::generate()).collect();

        // Distribute each share encrypted to its assigned steward.
        let encrypted: Vec<EncryptedStewardShare> = shares
            .iter()
            .zip(stewards.iter())
            .map(|(share, steward)| {
                encrypt_share_for_steward(share, 3, 1, &steward.encapsulation_key()).unwrap()
            })
            .collect();

        // Recovery: steward 0, 2, 4 release their shares.
        let releasing = [0usize, 2, 4];
        let collected: Vec<ShamirShare> = releasing
            .iter()
            .map(|&i| encrypted[i].decrypt(&stewards[i]).unwrap())
            .collect();

        let recovered = shamir::combine_shares(&collected, 3).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let secret = sample_secret();
        let shares = shamir::split_secret(&secret, 3, 5).unwrap();

        let steward = PQKemKeyPair::generate();
        let encrypted =
            encrypt_share_for_steward(&shares[0], 3, 7, &steward.encapsulation_key()).unwrap();

        let bytes = encrypted.to_bytes().unwrap();
        let restored = EncryptedStewardShare::from_bytes(&bytes).unwrap();

        assert_eq!(restored.share_index, encrypted.share_index);
        assert_eq!(restored.threshold, encrypted.threshold);
        assert_eq!(restored.secret_version, encrypted.secret_version);
        assert_eq!(restored.recipient_kem_id, encrypted.recipient_kem_id);
        assert_eq!(restored.kem_ciphertext, encrypted.kem_ciphertext);
        assert_eq!(restored.nonce, encrypted.nonce);
        assert_eq!(restored.encrypted_share, encrypted.encrypted_share);

        let recovered = restored.decrypt(&steward).unwrap();
        assert_eq!(recovered.to_bytes(), shares[0].to_bytes());
    }

    #[test]
    fn test_recipient_id_matches_pubkey_prefix() {
        let secret = sample_secret();
        let shares = shamir::split_secret(&secret, 3, 5).unwrap();

        let steward = PQKemKeyPair::generate();
        let ek_bytes = steward.encapsulation_key().to_bytes();

        let encrypted =
            encrypt_share_for_steward(&shares[0], 3, 1, &steward.encapsulation_key()).unwrap();

        assert_eq!(
            &encrypted.recipient_kem_id[..],
            &ek_bytes[..RECIPIENT_KEM_ID_LEN]
        );
    }
}
