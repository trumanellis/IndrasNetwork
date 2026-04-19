//! Steward-based recovery for the pass-story keystore.
//!
//! Splits the 32-byte `encryption` subkey produced by
//! [`indras_crypto::pass_story::expand_subkeys`] into K-of-N Shamir
//! shares, encrypts each to a steward's ML-KEM-768 public key, and
//! persists a local manifest recording the assignments. On recovery,
//! K decrypted shares are recombined into the original subkey, which
//! the caller then feeds back into [`indras_node::StoryKeystore`] to
//! re-initialize at-rest decryption.
//!
//! This module is pure offline orchestration plus disk persistence —
//! how the encrypted shares actually reach stewards (over iroh, via
//! braid sync, or out-of-band) is layered separately so the
//! cryptographic flow can be validated in isolation.
//!
//! # Lifecycle
//!
//! 1. **Setup** — after `StoryAuth::create_account`, call
//!    [`prepare_recovery`] with the user's chosen stewards and K. Save
//!    the returned manifest with [`save_manifest`]; ship the encrypted
//!    shares out to each steward.
//! 2. **Steward storage** — each steward holds one
//!    [`indras_crypto::steward_share::EncryptedStewardShare`].
//! 3. **Recovery** — collect K shares from stewards, each calls
//!    `EncryptedStewardShare::decrypt` to produce a
//!    [`indras_crypto::shamir::ShamirShare`], pass them to
//!    [`recover_encryption_subkey`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use indras_crypto::error::CryptoError;
use indras_crypto::pq_kem::PQEncapsulationKey;
use indras_crypto::shamir::{self, ShamirShare, SHAMIR_SECRET_SIZE};
use indras_crypto::steward_share::{
    encrypt_share_for_steward, EncryptedStewardShare, RECIPIENT_KEM_ID_LEN,
};

/// Filename for the on-disk steward-recovery manifest.
const MANIFEST_FILENAME: &str = "steward_recovery.json";

/// Opaque steward identifier.
///
/// Bytes are chosen by the caller; typical content is a hash of the
/// steward's PQ verifying key or their `MemberId`. Stored in plaintext
/// alongside the assignment so the user can see who holds which share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StewardId(pub Vec<u8>);

impl StewardId {
    /// Construct a steward id from a byte slice.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Borrow the underlying bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// One steward's assignment within a recovery split.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardAssignment {
    /// Caller-chosen identifier for this steward.
    pub steward_id: StewardId,
    /// X-coordinate of the Shamir share they hold.
    pub share_index: u8,
    /// First [`RECIPIENT_KEM_ID_LEN`] bytes of the steward's KEM
    /// encapsulation key. Lets the user double-check assignments
    /// against the steward's identity without reaching out.
    pub recipient_kem_id: [u8; RECIPIENT_KEM_ID_LEN],
    /// Wall-clock millis when the share was generated and encrypted.
    pub assigned_at_millis: i64,
}

/// Local manifest of a steward-recovery split.
///
/// Records the threshold, version, and per-steward assignments. Saved
/// to the user's data dir as JSON for inspectability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardManifest {
    /// Threshold (K) needed to recombine the secret.
    pub threshold: u8,
    /// Total shares issued (N).
    pub total_shares: u8,
    /// Monotonic split version. Increments when the user rotates the
    /// underlying secret and re-issues shares.
    pub secret_version: u64,
    /// One entry per steward.
    pub assignments: Vec<StewardAssignment>,
    /// Wall-clock millis when the manifest was created.
    pub created_at_millis: i64,
}

/// Output of [`prepare_recovery`]: the manifest to persist locally and
/// the encrypted shares to ship to stewards.
pub struct PreparedRecovery {
    /// Manifest meant for local storage on the user's device.
    pub manifest: StewardManifest,
    /// Per-steward encrypted shares; index `i` corresponds to the
    /// `i`th entry in `manifest.assignments`.
    pub encrypted_shares: Vec<EncryptedStewardShare>,
}

/// Errors specific to steward recovery.
#[derive(Debug, Error)]
pub enum StewardRecoveryError {
    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("manifest serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("manifest not found at {0}")]
    ManifestNotFound(PathBuf),

    #[error("threshold k must be >= 2 (got {k})")]
    ThresholdTooSmall { k: u8 },

    #[error("steward count {n} cannot satisfy threshold {k} (need k <= n <= 255)")]
    InvalidStewardCount { n: usize, k: u8 },
}

/// Result alias for steward-recovery operations.
pub type StewardRecoveryResult<T> = Result<T, StewardRecoveryError>;

/// Split the encryption subkey into K-of-N shares and encrypt each to
/// the corresponding steward.
///
/// `stewards` is taken in order — share index `i+1` is encrypted to
/// `stewards[i]`. The slice length is N; `k` must satisfy `2 <= k <= N`
/// and `N <= 255`.
pub fn prepare_recovery(
    encryption_subkey: &[u8; SHAMIR_SECRET_SIZE],
    stewards: &[(StewardId, PQEncapsulationKey)],
    k: u8,
    secret_version: u64,
) -> StewardRecoveryResult<PreparedRecovery> {
    if k < 2 {
        return Err(StewardRecoveryError::ThresholdTooSmall { k });
    }
    if stewards.len() < k as usize || stewards.len() > 255 {
        return Err(StewardRecoveryError::InvalidStewardCount {
            n: stewards.len(),
            k,
        });
    }

    let n = stewards.len() as u8;
    let shares = shamir::split_secret(encryption_subkey, k, n)?;
    let now = chrono::Utc::now().timestamp_millis();

    let mut encrypted_shares = Vec::with_capacity(n as usize);
    let mut assignments = Vec::with_capacity(n as usize);

    for (share, (steward_id, ek)) in shares.iter().zip(stewards.iter()) {
        let enc = encrypt_share_for_steward(share, k, secret_version, ek)?;
        assignments.push(StewardAssignment {
            steward_id: steward_id.clone(),
            share_index: enc.share_index,
            recipient_kem_id: enc.recipient_kem_id,
            assigned_at_millis: now,
        });
        encrypted_shares.push(enc);
    }

    let manifest = StewardManifest {
        threshold: k,
        total_shares: n,
        secret_version,
        assignments,
        created_at_millis: now,
    };

    Ok(PreparedRecovery {
        manifest,
        encrypted_shares,
    })
}

/// Persist the manifest under `<data_dir>/steward_recovery.json`.
pub fn save_manifest(data_dir: &Path, manifest: &StewardManifest) -> StewardRecoveryResult<()> {
    let path = manifest_path(data_dir);
    let bytes = serde_json::to_vec_pretty(manifest)?;
    std::fs::write(&path, bytes)?;
    Ok(())
}

/// Load the manifest from `<data_dir>/steward_recovery.json`.
pub fn load_manifest(data_dir: &Path) -> StewardRecoveryResult<StewardManifest> {
    let path = manifest_path(data_dir);
    if !path.exists() {
        return Err(StewardRecoveryError::ManifestNotFound(path));
    }
    let bytes = std::fs::read(&path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Recombine K decrypted shares into the original 32-byte encryption
/// subkey.
///
/// The caller is responsible for collecting decrypted shares from
/// stewards (each invokes
/// [`indras_crypto::steward_share::EncryptedStewardShare::decrypt`] to
/// turn their stored ciphertext into a [`ShamirShare`]).
pub fn recover_encryption_subkey(
    decrypted_shares: &[ShamirShare],
    k: u8,
) -> StewardRecoveryResult<[u8; SHAMIR_SECRET_SIZE]> {
    Ok(shamir::combine_shares(decrypted_shares, k)?)
}

fn manifest_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MANIFEST_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_crypto::pq_kem::PQKemKeyPair;
    use tempfile::TempDir;

    fn sample_subkey() -> [u8; SHAMIR_SECRET_SIZE] {
        let mut s = [0u8; SHAMIR_SECRET_SIZE];
        for (i, b) in s.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(13).wrapping_add(2);
        }
        s
    }

    fn sample_stewards(n: usize) -> Vec<(StewardId, PQKemKeyPair)> {
        (0..n)
            .map(|i| {
                let id = StewardId::new(format!("steward-{}", i).into_bytes());
                (id, PQKemKeyPair::generate())
            })
            .collect()
    }

    fn into_eks(
        stewards: &[(StewardId, PQKemKeyPair)],
    ) -> Vec<(StewardId, PQEncapsulationKey)> {
        stewards
            .iter()
            .map(|(id, kp)| (id.clone(), kp.encapsulation_key()))
            .collect()
    }

    #[test]
    fn test_prepare_recovery_full_roundtrip() {
        let subkey = sample_subkey();
        let stewards = sample_stewards(5);
        let prepared = prepare_recovery(&subkey, &into_eks(&stewards), 3, 1).unwrap();

        assert_eq!(prepared.manifest.threshold, 3);
        assert_eq!(prepared.manifest.total_shares, 5);
        assert_eq!(prepared.manifest.secret_version, 1);
        assert_eq!(prepared.manifest.assignments.len(), 5);
        assert_eq!(prepared.encrypted_shares.len(), 5);

        // Stewards 0, 2, 4 release.
        let releasing = [0usize, 2, 4];
        let decrypted: Vec<ShamirShare> = releasing
            .iter()
            .map(|&i| prepared.encrypted_shares[i].decrypt(&stewards[i].1).unwrap())
            .collect();

        let recovered = recover_encryption_subkey(&decrypted, 3).unwrap();
        assert_eq!(recovered, subkey);
    }

    #[test]
    fn test_assignments_carry_steward_id_and_kem_id() {
        let subkey = sample_subkey();
        let stewards = sample_stewards(3);
        let eks = into_eks(&stewards);
        let prepared = prepare_recovery(&subkey, &eks, 2, 1).unwrap();

        for (i, assignment) in prepared.manifest.assignments.iter().enumerate() {
            assert_eq!(assignment.steward_id, eks[i].0);
            // x-coordinates are 1..=N (sharks reserves 0 for the secret).
            assert_eq!(assignment.share_index, prepared.encrypted_shares[i].share_index);
            let ek_bytes = eks[i].1.to_bytes();
            assert_eq!(
                &assignment.recipient_kem_id[..],
                &ek_bytes[..RECIPIENT_KEM_ID_LEN]
            );
        }
    }

    #[test]
    fn test_threshold_too_small_rejected() {
        let subkey = sample_subkey();
        let stewards = sample_stewards(5);
        let err = prepare_recovery(&subkey, &into_eks(&stewards), 1, 1);
        assert!(matches!(err, Err(StewardRecoveryError::ThresholdTooSmall { k: 1 })));
    }

    #[test]
    fn test_too_few_stewards_rejected() {
        let subkey = sample_subkey();
        let stewards = sample_stewards(2);
        let err = prepare_recovery(&subkey, &into_eks(&stewards), 3, 1);
        assert!(matches!(
            err,
            Err(StewardRecoveryError::InvalidStewardCount { n: 2, k: 3 })
        ));
    }

    #[test]
    fn test_manifest_save_load_roundtrip() {
        let subkey = sample_subkey();
        let stewards = sample_stewards(4);
        let prepared = prepare_recovery(&subkey, &into_eks(&stewards), 2, 7).unwrap();

        let dir = TempDir::new().unwrap();
        save_manifest(dir.path(), &prepared.manifest).unwrap();

        let loaded = load_manifest(dir.path()).unwrap();
        assert_eq!(loaded.threshold, prepared.manifest.threshold);
        assert_eq!(loaded.total_shares, prepared.manifest.total_shares);
        assert_eq!(loaded.secret_version, prepared.manifest.secret_version);
        assert_eq!(loaded.assignments.len(), prepared.manifest.assignments.len());
    }

    #[test]
    fn test_manifest_missing_errors() {
        let dir = TempDir::new().unwrap();
        let err = load_manifest(dir.path());
        assert!(matches!(err, Err(StewardRecoveryError::ManifestNotFound(_))));
    }

    #[test]
    fn test_recovery_with_too_few_shares_errors() {
        let subkey = sample_subkey();
        let stewards = sample_stewards(5);
        let prepared = prepare_recovery(&subkey, &into_eks(&stewards), 3, 1).unwrap();

        let only_two: Vec<ShamirShare> = prepared.encrypted_shares[..2]
            .iter()
            .zip(stewards.iter())
            .map(|(enc, (_, kp))| enc.decrypt(kp).unwrap())
            .collect();

        let err = recover_encryption_subkey(&only_two, 3);
        assert!(err.is_err());
    }
}
