//! Shamir K-of-N secret sharing over a 32-byte secret.
//!
//! Thin wrapper over the `sharks` crate. Used by the steward-recovery
//! flow in `indras-sync-engine` to split the 32-byte `encryption`
//! subkey (from `pass_story::expand_subkeys`) into N shares, any K of
//! which can recombine to recover the original. Each share is further
//! encrypted to a steward's ML-KEM-768 public key before distribution.
//!
//! # Example
//!
//! ```
//! use indras_crypto::shamir;
//!
//! let secret = [7u8; 32];
//! let shares = shamir::split_secret(&secret, 3, 5).unwrap();
//! assert_eq!(shares.len(), 5);
//!
//! let recovered = shamir::combine_shares(&shares[..3], 3).unwrap();
//! assert_eq!(recovered, secret);
//! ```

use serde::{Deserialize, Serialize};
use sharks::{Share as SharksShare, Sharks};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{CryptoError, CryptoResult};

/// Size of the secret a share protects.
pub const SHAMIR_SECRET_SIZE: usize = 32;

/// A single Shamir share over a 32-byte secret.
///
/// Internally holds the `sharks`-encoded bytes (first byte =
/// x-coordinate, remaining bytes = per-byte polynomial evaluations).
/// Serialize via `to_bytes` / `from_bytes` for storage or wire
/// transmission. The struct zeroizes its contents on drop.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct ShamirShare {
    bytes: Vec<u8>,
}

impl ShamirShare {
    /// Return the share's x-coordinate (1..=255; x=0 is the secret itself).
    pub fn index(&self) -> u8 {
        self.bytes.first().copied().unwrap_or(0)
    }

    /// Serialize to a byte buffer for persistence or wire transport.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Reconstruct a share from its byte form.
    ///
    /// Validates that the bytes parse as a valid `sharks` share before
    /// returning.
    pub fn from_bytes(bytes: &[u8]) -> CryptoResult<Self> {
        let share = SharksShare::try_from(bytes).map_err(|_| CryptoError::InvalidShare)?;
        Ok(Self {
            bytes: Vec::from(&share),
        })
    }

    fn as_sharks_share(&self) -> CryptoResult<SharksShare> {
        SharksShare::try_from(self.bytes.as_slice()).map_err(|_| CryptoError::InvalidShare)
    }
}

impl std::fmt::Debug for ShamirShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print share bytes — they recombine into the secret.
        f.debug_struct("ShamirShare")
            .field("index", &self.index())
            .field("len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// Split a 32-byte secret into `n` shares, any `k` of which recombine.
///
/// - `k` (threshold) must be between 2 and `n` inclusive.
/// - `n` must be between `k` and 255 inclusive.
///
/// Uses `sharks` with its internal CSPRNG.
pub fn split_secret(
    secret: &[u8; SHAMIR_SECRET_SIZE],
    k: u8,
    n: u8,
) -> CryptoResult<Vec<ShamirShare>> {
    if k < 2 {
        return Err(CryptoError::ShamirParams("k must be >= 2".into()));
    }
    if n < k {
        return Err(CryptoError::ShamirParams(format!(
            "n ({}) must be >= k ({})",
            n, k
        )));
    }

    let sharks = Sharks(k);
    let dealer = sharks.dealer(secret);
    let shares: Vec<ShamirShare> = dealer
        .take(n as usize)
        .map(|share| ShamirShare {
            bytes: Vec::from(&share),
        })
        .collect();

    if shares.len() != n as usize {
        return Err(CryptoError::ShamirParams(format!(
            "dealer produced {} shares, expected {}",
            shares.len(),
            n
        )));
    }

    Ok(shares)
}

/// Combine at least `k` shares to recover the original 32-byte secret.
///
/// Returns an error if fewer than `k` shares are supplied, any share
/// is malformed, or reconstruction yields a secret of the wrong size.
/// Supplying shares from *different* secrets will not reliably error —
/// callers must ensure shares belong to the same split.
pub fn combine_shares(
    shares: &[ShamirShare],
    k: u8,
) -> CryptoResult<[u8; SHAMIR_SECRET_SIZE]> {
    if k < 2 {
        return Err(CryptoError::ShamirParams("k must be >= 2".into()));
    }
    if shares.len() < k as usize {
        return Err(CryptoError::ShamirParams(format!(
            "need at least {} shares, got {}",
            k,
            shares.len()
        )));
    }

    let sharks_shares: Vec<SharksShare> = shares
        .iter()
        .map(|s| s.as_sharks_share())
        .collect::<CryptoResult<_>>()?;

    let sharks = Sharks(k);
    let recovered = sharks
        .recover(sharks_shares.as_slice())
        .map_err(|_| CryptoError::ShamirReconstruction)?;

    if recovered.len() != SHAMIR_SECRET_SIZE {
        return Err(CryptoError::ShamirReconstruction);
    }

    let mut out = [0u8; SHAMIR_SECRET_SIZE];
    out.copy_from_slice(&recovered);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_secret() -> [u8; SHAMIR_SECRET_SIZE] {
        let mut s = [0u8; SHAMIR_SECRET_SIZE];
        for (i, b) in s.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(3);
        }
        s
    }

    #[test]
    fn test_split_combine_roundtrip_threshold() {
        let secret = sample_secret();
        let shares = split_secret(&secret, 3, 5).unwrap();
        assert_eq!(shares.len(), 5);

        let recovered = combine_shares(&shares[..3], 3).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_split_combine_roundtrip_more_than_threshold() {
        let secret = sample_secret();
        let shares = split_secret(&secret, 3, 5).unwrap();
        let recovered = combine_shares(&shares, 3).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_below_threshold_does_not_recover_secret() {
        let secret = sample_secret();
        let shares = split_secret(&secret, 3, 5).unwrap();

        // sharks::recover with k=3 shares=2 may or may not error depending
        // on internal validation; what matters is that the result is not
        // the original secret.
        let maybe = combine_shares(&shares[..2], 3);
        if let Ok(recovered) = maybe {
            assert_ne!(
                recovered, secret,
                "k-1 shares should not reconstruct the secret"
            );
        }
    }

    #[test]
    fn test_below_threshold_count_errors() {
        let secret = sample_secret();
        let shares = split_secret(&secret, 3, 5).unwrap();
        // Call with k=3 requires at least 3 shares; supplying 2 should error.
        let err = combine_shares(&shares[..2], 3);
        assert!(matches!(err, Err(CryptoError::ShamirParams(_))));
    }

    #[test]
    fn test_share_serialize_roundtrip() {
        let secret = sample_secret();
        let shares = split_secret(&secret, 3, 5).unwrap();

        let wire: Vec<Vec<u8>> = shares.iter().map(|s| s.to_bytes()).collect();
        let restored: Vec<ShamirShare> = wire
            .iter()
            .map(|b| ShamirShare::from_bytes(b).unwrap())
            .collect();

        let recovered = combine_shares(&restored[..3], 3).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn test_share_indices_are_distinct() {
        let secret = sample_secret();
        let shares = split_secret(&secret, 3, 5).unwrap();
        let mut idxs: Vec<u8> = shares.iter().map(|s| s.index()).collect();
        idxs.sort();
        idxs.dedup();
        assert_eq!(idxs.len(), 5, "each share must have a distinct x-coordinate");
        assert!(idxs.iter().all(|&i| i != 0), "x=0 is reserved for the secret");
    }

    #[test]
    fn test_invalid_params_k_too_small() {
        let secret = sample_secret();
        let err = split_secret(&secret, 1, 5);
        assert!(matches!(err, Err(CryptoError::ShamirParams(_))));
    }

    #[test]
    fn test_invalid_params_n_less_than_k() {
        let secret = sample_secret();
        let err = split_secret(&secret, 5, 3);
        assert!(matches!(err, Err(CryptoError::ShamirParams(_))));
    }

    #[test]
    fn test_corrupt_share_bytes_rejected() {
        let err = ShamirShare::from_bytes(&[]);
        assert!(matches!(err, Err(CryptoError::InvalidShare)));
    }

    #[test]
    fn test_different_secrets_produce_different_shares() {
        let secret_a = [0xAAu8; SHAMIR_SECRET_SIZE];
        let secret_b = [0xBBu8; SHAMIR_SECRET_SIZE];
        let shares_a = split_secret(&secret_a, 3, 5).unwrap();
        let shares_b = split_secret(&secret_b, 3, 5).unwrap();
        // At least one pair differs (astronomical probability of collision
        // for 32-byte secrets, treated as deterministic up to sharks' RNG).
        let any_different = shares_a
            .iter()
            .zip(shares_b.iter())
            .any(|(a, b)| a.bytes != b.bytes);
        assert!(any_different);
    }
}
