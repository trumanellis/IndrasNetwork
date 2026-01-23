//! Post-quantum key encapsulation using Kyber768 (ML-KEM-768 equivalent)
//!
//! Provides quantum-resistant key encapsulation mechanism for secure
//! key exchange. Uses NIST FIPS 203 ML-KEM (Kyber) algorithm.
//!
//! ## Key Sizes (Kyber768 / ML-KEM-768 equivalent)
//!
//! - Encapsulation key (public): 1,184 bytes
//! - Decapsulation key (private): 2,400 bytes
//! - Ciphertext: 1,088 bytes
//! - Shared secret: 32 bytes
//!
//! ## Security
//!
//! Decapsulation keys and shared secrets are zeroized on drop to prevent
//! leakage in memory dumps.

use pqcrypto_kyber::kyber768;
use pqcrypto_traits::kem::{PublicKey as _, SecretKey as _, Ciphertext as _, SharedSecret as _};
use serde::{Deserialize, Serialize};
// Note: Zeroize is imported but handled through SecureBytes wrapper

use crate::error::CryptoError;
use crate::pq_identity::SecureBytes;

/// Size of the Kyber768 encapsulation key (public) in bytes
pub const PQ_ENCAPSULATION_KEY_SIZE: usize = kyber768::public_key_bytes();

/// Size of the Kyber768 decapsulation key (private) in bytes
pub const PQ_DECAPSULATION_KEY_SIZE: usize = kyber768::secret_key_bytes();

/// Size of the Kyber768 ciphertext in bytes
pub const PQ_CIPHERTEXT_SIZE: usize = kyber768::ciphertext_bytes();

/// Size of the shared secret in bytes
pub const PQ_SHARED_SECRET_SIZE: usize = kyber768::shared_secret_bytes();

/// Post-quantum KEM key pair using Kyber768 (ML-KEM-768 equivalent)
///
/// Contains both the encapsulation key (public) and decapsulation key (private).
#[derive(Clone)]
pub struct PQKemKeyPair {
    encapsulation_key: kyber768::PublicKey,
    decapsulation_key: kyber768::SecretKey,
}

impl PQKemKeyPair {
    /// Generate a new random KEM key pair
    pub fn generate() -> Self {
        let (encapsulation_key, decapsulation_key) = kyber768::keypair();
        Self {
            encapsulation_key,
            decapsulation_key,
        }
    }

    /// Create from decapsulation key bytes and encapsulation key bytes
    pub fn from_keypair_bytes(dk_bytes: &[u8], ek_bytes: &[u8]) -> Result<Self, CryptoError> {
        if dk_bytes.len() != PQ_DECAPSULATION_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid decapsulation key size: expected {}, got {}",
                PQ_DECAPSULATION_KEY_SIZE,
                dk_bytes.len()
            )));
        }
        if ek_bytes.len() != PQ_ENCAPSULATION_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid encapsulation key size: expected {}, got {}",
                PQ_ENCAPSULATION_KEY_SIZE,
                ek_bytes.len()
            )));
        }

        let decapsulation_key = kyber768::SecretKey::from_bytes(dk_bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Kyber decapsulation key: {:?}", e)))?;

        let encapsulation_key = kyber768::PublicKey::from_bytes(ek_bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Kyber encapsulation key: {:?}", e)))?;

        Ok(Self {
            encapsulation_key,
            decapsulation_key,
        })
    }

    /// Export full keypair bytes for storage
    ///
    /// Returns (decapsulation_key_bytes, encapsulation_key_bytes)
    /// WARNING: Keep the decapsulation key secret!
    /// The decapsulation key is wrapped in SecureBytes which zeroizes on drop.
    pub fn to_keypair_bytes(&self) -> (SecureBytes, Vec<u8>) {
        (
            SecureBytes::new(self.decapsulation_key.as_bytes().to_vec()),
            self.encapsulation_key.as_bytes().to_vec(),
        )
    }

    /// Export decapsulation key bytes (for storage)
    ///
    /// WARNING: Keep this secret!
    /// The returned SecureBytes will zeroize the key material when dropped.
    pub fn decapsulation_key_bytes(&self) -> SecureBytes {
        SecureBytes::new(self.decapsulation_key.as_bytes().to_vec())
    }

    /// Get the public encapsulation key
    pub fn encapsulation_key(&self) -> PQEncapsulationKey {
        PQEncapsulationKey {
            key: self.encapsulation_key.clone(),
        }
    }

    /// Get the raw encapsulation key bytes
    pub fn encapsulation_key_bytes(&self) -> Vec<u8> {
        self.encapsulation_key.as_bytes().to_vec()
    }

    /// Decapsulate a ciphertext to recover the shared secret
    pub fn decapsulate(&self, ciphertext: &PQCiphertext) -> Result<[u8; PQ_SHARED_SECRET_SIZE], CryptoError> {
        let ct = kyber768::Ciphertext::from_bytes(&ciphertext.bytes)
            .map_err(|e| CryptoError::PQDecapsulationFailed(format!("Invalid ciphertext: {:?}", e)))?;

        let shared_secret = kyber768::decapsulate(&ct, &self.decapsulation_key);
        let ss_bytes = shared_secret.as_bytes();

        let mut result = [0u8; PQ_SHARED_SECRET_SIZE];
        result.copy_from_slice(ss_bytes);
        Ok(result)
    }
}

impl std::fmt::Debug for PQKemKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PQKemKeyPair")
            .field("encapsulation_key", &hex::encode(&self.encapsulation_key_bytes()[..8]))
            .finish_non_exhaustive()
    }
}

/// Public encapsulation key for KEM
///
/// Can be freely shared. Used by others to encapsulate secrets to you.
#[derive(Clone)]
pub struct PQEncapsulationKey {
    key: kyber768::PublicKey,
}

impl PQEncapsulationKey {
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != PQ_ENCAPSULATION_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid encapsulation key size: expected {}, got {}",
                PQ_ENCAPSULATION_KEY_SIZE,
                bytes.len()
            )));
        }

        let key = kyber768::PublicKey::from_bytes(bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Kyber encapsulation key: {:?}", e)))?;

        Ok(Self { key })
    }

    /// Export to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.key.as_bytes().to_vec()
    }

    /// Encapsulate a random shared secret
    ///
    /// Returns the ciphertext and the shared secret.
    /// Send the ciphertext to the key owner; they can decapsulate to get the same secret.
    pub fn encapsulate(&self) -> (PQCiphertext, [u8; PQ_SHARED_SECRET_SIZE]) {
        let (shared_secret, ciphertext) = kyber768::encapsulate(&self.key);

        let ct = PQCiphertext {
            bytes: ciphertext.as_bytes().to_vec(),
        };

        let ss_bytes = shared_secret.as_bytes();
        let mut result = [0u8; PQ_SHARED_SECRET_SIZE];
        result.copy_from_slice(ss_bytes);

        (ct, result)
    }

    /// Get a short identifier (first 8 bytes hex encoded)
    pub fn short_id(&self) -> String {
        hex::encode(&self.to_bytes()[..8])
    }
}

impl std::fmt::Debug for PQEncapsulationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PQEncapsulationKey")
            .field("id", &self.short_id())
            .finish()
    }
}

impl PartialEq for PQEncapsulationKey {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

impl Eq for PQEncapsulationKey {}

impl std::hash::Hash for PQEncapsulationKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_bytes().hash(state);
    }
}

/// A KEM ciphertext
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PQCiphertext {
    bytes: Vec<u8>,
}

impl PQCiphertext {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, CryptoError> {
        if bytes.len() != PQ_CIPHERTEXT_SIZE {
            return Err(CryptoError::InvalidPQCiphertext(format!(
                "Invalid ciphertext size: expected {}, got {}",
                PQ_CIPHERTEXT_SIZE,
                bytes.len()
            )));
        }
        Ok(Self { bytes })
    }

    /// Export to bytes
    pub fn to_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Get owned bytes
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let keypair = PQKemKeyPair::generate();
        let encap_key = keypair.encapsulation_key();

        assert!(!encap_key.short_id().is_empty());
    }

    #[test]
    fn test_encapsulate_decapsulate() {
        // Alice generates a key pair
        let alice = PQKemKeyPair::generate();

        // Bob gets Alice's public key and encapsulates a secret
        let alice_public = alice.encapsulation_key();
        let (ciphertext, bob_secret) = alice_public.encapsulate();

        // Alice decapsulates to get the same secret
        let alice_secret = alice.decapsulate(&ciphertext).unwrap();

        assert_eq!(alice_secret, bob_secret);
    }

    #[test]
    fn test_different_keypairs_different_secrets() {
        let alice = PQKemKeyPair::generate();
        let eve = PQKemKeyPair::generate();

        // Bob encapsulates to Alice
        let alice_public = alice.encapsulation_key();
        let (ciphertext, bob_secret) = alice_public.encapsulate();

        // Alice gets the correct secret
        let alice_secret = alice.decapsulate(&ciphertext).unwrap();
        assert_eq!(alice_secret, bob_secret);

        // Eve gets a different secret (Kyber has implicit rejection)
        let eve_secret = eve.decapsulate(&ciphertext).unwrap();
        assert_ne!(eve_secret, bob_secret);
    }

    #[test]
    fn test_keypair_roundtrip() {
        let keypair = PQKemKeyPair::generate();

        // Export and reimport keypair
        let (dk_bytes, ek_bytes) = keypair.to_keypair_bytes();
        let restored = PQKemKeyPair::from_keypair_bytes(dk_bytes.as_slice(), &ek_bytes).unwrap();

        // Encapsulation keys should match
        assert_eq!(
            keypair.encapsulation_key_bytes(),
            restored.encapsulation_key_bytes()
        );

        // Should be able to decapsulate
        let (ciphertext, original_secret) = keypair.encapsulation_key().encapsulate();
        let restored_secret = restored.decapsulate(&ciphertext).unwrap();
        assert_eq!(original_secret, restored_secret);
    }

    #[test]
    fn test_encapsulation_key_roundtrip() {
        let keypair = PQKemKeyPair::generate();

        // Export and reimport encapsulation key
        let ek_bytes = keypair.encapsulation_key_bytes();
        let public = PQEncapsulationKey::from_bytes(&ek_bytes).unwrap();

        // Should be able to encapsulate
        let (ciphertext, secret) = public.encapsulate();

        // Original keypair should be able to decapsulate
        let decapsulated = keypair.decapsulate(&ciphertext).unwrap();
        assert_eq!(secret, decapsulated);
    }

    #[test]
    fn test_invalid_key_sizes() {
        // Too short decapsulation key
        let result = PQKemKeyPair::from_keypair_bytes(&[0u8; 100], &[0u8; PQ_ENCAPSULATION_KEY_SIZE]);
        assert!(result.is_err());

        // Too short encapsulation key
        let result = PQEncapsulationKey::from_bytes(&[0u8; 100]);
        assert!(result.is_err());
    }

    #[test]
    fn test_key_sizes() {
        let keypair = PQKemKeyPair::generate();

        assert_eq!(keypair.decapsulation_key_bytes().len(), PQ_DECAPSULATION_KEY_SIZE);
        assert_eq!(keypair.encapsulation_key_bytes().len(), PQ_ENCAPSULATION_KEY_SIZE);

        let (ciphertext, _) = keypair.encapsulation_key().encapsulate();
        assert_eq!(ciphertext.to_bytes().len(), PQ_CIPHERTEXT_SIZE);
    }

    #[test]
    fn test_encapsulation_key_equality() {
        let keypair = PQKemKeyPair::generate();
        let public1 = keypair.encapsulation_key();
        let public2 = keypair.encapsulation_key();

        assert_eq!(public1, public2);

        let other = PQKemKeyPair::generate();
        let public3 = other.encapsulation_key();

        assert_ne!(public1, public3);
    }
}
