//! Per-artifact encryption primitives for revocable sharing.
//!
//! Contains the encrypted key blob type used when artifacts are shared
//! with revocation support. The artifact content is encrypted with a
//! per-artifact ChaCha20-Poly1305 key, and the key itself is encrypted
//! with the realm's interface key.

use serde::{Deserialize, Serialize};

/// Size of artifact encryption keys (ChaCha20-Poly1305).
pub const ARTIFACT_KEY_SIZE: usize = 32;

/// Per-artifact encryption key (ChaCha20-Poly1305).
pub type ArtifactKey = [u8; ARTIFACT_KEY_SIZE];

/// Encrypted artifact key blob.
///
/// The artifact key is encrypted with the realm's interface key
/// so only realm members can decrypt artifacts.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EncryptedArtifactKey {
    /// Nonce used for encryption (12 bytes for ChaCha20-Poly1305).
    pub nonce: [u8; 12],
    /// Encrypted key data (32 bytes + 16 bytes auth tag).
    pub ciphertext: Vec<u8>,
}
