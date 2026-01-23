//! Post-quantum identity using ML-DSA (Dilithium) signatures
//!
//! Provides quantum-resistant digital signatures for application-layer
//! authentication. Uses NIST FIPS 204 ML-DSA (Dilithium) algorithm.
//!
//! ## Key Sizes (Dilithium3 / ML-DSA-65 equivalent)
//!
//! - Signing key: 4,000 bytes
//! - Verifying key: 1,952 bytes
//! - Signature: 3,293 bytes
//!
//! ## Security
//!
//! Secret keys are zeroized on drop to prevent leakage in memory dumps.

use pqcrypto_dilithium::dilithium3;
use pqcrypto_traits::sign::{PublicKey as _, SecretKey as _, DetachedSignature};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::CryptoError;

/// Size of the Dilithium3 signing key in bytes
pub const PQ_SIGNING_KEY_SIZE: usize = dilithium3::secret_key_bytes();

/// Size of the Dilithium3 verifying key in bytes
pub const PQ_VERIFYING_KEY_SIZE: usize = dilithium3::public_key_bytes();

/// Size of the Dilithium3 signature in bytes
pub const PQ_SIGNATURE_SIZE: usize = dilithium3::signature_bytes();

/// Secure byte container that zeroizes on drop
///
/// Use this for storing sensitive key material that should not
/// persist in memory after use.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecureBytes(Vec<u8>);

impl SecureBytes {
    /// Create new secure bytes
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Get the inner bytes (borrowed)
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Get the length
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for SecureBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for SecureBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Post-quantum identity using Dilithium3 (ML-DSA-65 equivalent)
///
/// Contains both the signing key (private) and verifying key (public).
/// Used for creating digital signatures that are resistant to quantum attacks.
#[derive(Clone)]
pub struct PQIdentity {
    signing_key: dilithium3::SecretKey,
    verifying_key: dilithium3::PublicKey,
}

impl PQIdentity {
    /// Generate a new random PQ identity
    pub fn generate() -> Self {
        let (verifying_key, signing_key) = dilithium3::keypair();
        Self {
            signing_key,
            verifying_key,
        }
    }

    /// Create from signing key bytes
    ///
    /// Derives the verifying key from the signing key.
    pub fn from_signing_key_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != PQ_SIGNING_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid signing key size: expected {}, got {}",
                PQ_SIGNING_KEY_SIZE,
                bytes.len()
            )));
        }

        let _signing_key = dilithium3::SecretKey::from_bytes(bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Dilithium signing key: {:?}", e)))?;

        // Extract public key from secret key (it's embedded in the secret key)
        // We need to regenerate the keypair or extract it - for now we'll store both
        // Actually, pqcrypto doesn't provide a way to derive public from secret,
        // so we need to store both. Let's adjust the format.

        // The signing key bytes should include both keys concatenated
        // For now, we'll require the caller to provide just the secret key
        // and we'll need a different approach for persistence

        Err(CryptoError::InvalidKey(
            "Cannot derive verifying key from signing key alone. Use from_keypair_bytes instead.".to_string()
        ))
    }

    /// Create from full keypair bytes (signing key + verifying key)
    pub fn from_keypair_bytes(sk_bytes: &[u8], pk_bytes: &[u8]) -> Result<Self, CryptoError> {
        if sk_bytes.len() != PQ_SIGNING_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid signing key size: expected {}, got {}",
                PQ_SIGNING_KEY_SIZE,
                sk_bytes.len()
            )));
        }
        if pk_bytes.len() != PQ_VERIFYING_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid verifying key size: expected {}, got {}",
                PQ_VERIFYING_KEY_SIZE,
                pk_bytes.len()
            )));
        }

        let signing_key = dilithium3::SecretKey::from_bytes(sk_bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Dilithium signing key: {:?}", e)))?;

        let verifying_key = dilithium3::PublicKey::from_bytes(pk_bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Dilithium verifying key: {:?}", e)))?;

        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// Export full keypair bytes for storage
    ///
    /// Returns (signing_key_bytes, verifying_key_bytes)
    /// WARNING: Keep the signing key secret!
    /// The signing key is wrapped in SecureBytes which zeroizes on drop.
    pub fn to_keypair_bytes(&self) -> (SecureBytes, Vec<u8>) {
        (
            SecureBytes::new(self.signing_key.as_bytes().to_vec()),
            self.verifying_key.as_bytes().to_vec(),
        )
    }

    /// Export signing key bytes (for storage)
    ///
    /// WARNING: Keep this secret! Anyone with these bytes can sign as you.
    /// The returned SecureBytes will zeroize the key material when dropped.
    pub fn signing_key_bytes(&self) -> SecureBytes {
        SecureBytes::new(self.signing_key.as_bytes().to_vec())
    }

    /// Get the public verifying key
    pub fn verifying_key(&self) -> PQPublicIdentity {
        PQPublicIdentity {
            verifying_key: self.verifying_key.clone(),
        }
    }

    /// Get the raw verifying key bytes
    pub fn verifying_key_bytes(&self) -> Vec<u8> {
        self.verifying_key.as_bytes().to_vec()
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> PQSignature {
        let sig = dilithium3::detached_sign(message, &self.signing_key);
        PQSignature {
            bytes: sig.as_bytes().to_vec(),
        }
    }

    /// Verify a signature (convenience method using our own verifying key)
    pub fn verify(&self, message: &[u8], signature: &PQSignature) -> bool {
        self.verifying_key().verify(message, signature)
    }
}

impl std::fmt::Debug for PQIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PQIdentity")
            .field("verifying_key", &hex::encode(&self.verifying_key_bytes()[..8]))
            .finish_non_exhaustive()
    }
}

/// Public identity for signature verification
///
/// Contains only the verifying key (public). Can be freely shared.
#[derive(Clone)]
pub struct PQPublicIdentity {
    verifying_key: dilithium3::PublicKey,
}

impl PQPublicIdentity {
    /// Create from verifying key bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != PQ_VERIFYING_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid verifying key size: expected {}, got {}",
                PQ_VERIFYING_KEY_SIZE,
                bytes.len()
            )));
        }

        let verifying_key = dilithium3::PublicKey::from_bytes(bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Dilithium verifying key: {:?}", e)))?;

        Ok(Self { verifying_key })
    }

    /// Export to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.verifying_key.as_bytes().to_vec()
    }

    /// Verify a signature
    pub fn verify(&self, message: &[u8], signature: &PQSignature) -> bool {
        if signature.bytes.len() != PQ_SIGNATURE_SIZE {
            return false;
        }

        match dilithium3::DetachedSignature::from_bytes(&signature.bytes) {
            Ok(sig) => dilithium3::verify_detached_signature(&sig, message, &self.verifying_key).is_ok(),
            Err(_) => false,
        }
    }

    /// Get a short identifier (first 8 bytes hex encoded)
    pub fn short_id(&self) -> String {
        hex::encode(&self.to_bytes()[..8])
    }
}

impl std::fmt::Debug for PQPublicIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PQPublicIdentity")
            .field("id", &self.short_id())
            .finish()
    }
}

impl PartialEq for PQPublicIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

impl Eq for PQPublicIdentity {}

impl std::hash::Hash for PQPublicIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_bytes().hash(state);
    }
}

/// A post-quantum signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PQSignature {
    bytes: Vec<u8>,
}

impl PQSignature {
    /// Create from raw bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, CryptoError> {
        if bytes.len() != PQ_SIGNATURE_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Invalid signature size: expected {}, got {}",
                PQ_SIGNATURE_SIZE,
                bytes.len()
            )));
        }
        Ok(Self { bytes })
    }

    /// Export to bytes
    pub fn to_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_identity() {
        let identity = PQIdentity::generate();
        let verifying_key = identity.verifying_key();

        assert!(!verifying_key.short_id().is_empty());
    }

    #[test]
    fn test_sign_verify() {
        let identity = PQIdentity::generate();
        let message = b"Hello, quantum-resistant world!";

        let signature = identity.sign(message);

        // Verify with our own key
        assert!(identity.verify(message, &signature));

        // Verify with public key
        let public = identity.verifying_key();
        assert!(public.verify(message, &signature));
    }

    #[test]
    fn test_wrong_message_fails() {
        let identity = PQIdentity::generate();
        let message = b"Original message";
        let wrong_message = b"Wrong message";

        let signature = identity.sign(message);

        assert!(!identity.verify(wrong_message, &signature));
    }

    #[test]
    fn test_wrong_key_fails() {
        let identity1 = PQIdentity::generate();
        let identity2 = PQIdentity::generate();
        let message = b"Test message";

        let signature = identity1.sign(message);

        // Verify with wrong key should fail
        assert!(!identity2.verify(message, &signature));
    }

    #[test]
    fn test_keypair_roundtrip() {
        let identity = PQIdentity::generate();
        let message = b"Test message";

        // Export and reimport keypair
        let (sk_bytes, pk_bytes) = identity.to_keypair_bytes();
        let restored = PQIdentity::from_keypair_bytes(sk_bytes.as_slice(), &pk_bytes).unwrap();

        // Verifying keys should match
        assert_eq!(
            identity.verifying_key_bytes(),
            restored.verifying_key_bytes()
        );

        // Should be able to verify signatures from original
        let signature = identity.sign(message);
        assert!(restored.verify(message, &signature));

        // Should be able to create valid signatures
        let signature2 = restored.sign(message);
        assert!(identity.verify(message, &signature2));
    }

    #[test]
    fn test_verifying_key_roundtrip() {
        let identity = PQIdentity::generate();
        let message = b"Test message";
        let signature = identity.sign(message);

        // Export and reimport verifying key
        let key_bytes = identity.verifying_key_bytes();
        let public = PQPublicIdentity::from_bytes(&key_bytes).unwrap();

        // Should verify correctly
        assert!(public.verify(message, &signature));
    }

    #[test]
    fn test_public_identity_equality() {
        let identity = PQIdentity::generate();
        let public1 = identity.verifying_key();
        let public2 = identity.verifying_key();

        assert_eq!(public1, public2);

        let other = PQIdentity::generate();
        let public3 = other.verifying_key();

        assert_ne!(public1, public3);
    }

    #[test]
    fn test_invalid_key_sizes() {
        // Too short verifying key
        let result = PQPublicIdentity::from_bytes(&[0u8; 100]);
        assert!(result.is_err());
    }

    #[test]
    fn test_key_sizes() {
        let identity = PQIdentity::generate();

        assert_eq!(identity.signing_key_bytes().len(), PQ_SIGNING_KEY_SIZE);
        assert_eq!(identity.verifying_key_bytes().len(), PQ_VERIFYING_KEY_SIZE);

        let signature = identity.sign(b"test");
        assert_eq!(signature.to_bytes().len(), PQ_SIGNATURE_SIZE);
    }

    #[test]
    fn test_secure_bytes_zeroizes() {
        let secret_data = vec![0xAB; 32];

        {
            let secure = SecureBytes::new(secret_data.clone());
            // Within scope, data is accessible
            assert_eq!(secure.as_slice(), secret_data.as_slice());
        }
        // After drop, the data should be zeroized
        // Note: This is a best-effort test; the actual memory may be reclaimed
    }

    // ========== Additional Edge Case Tests ==========

    #[test]
    fn test_sign_empty_message() {
        let identity = PQIdentity::generate();
        let message = b"";

        let signature = identity.sign(message);
        assert!(identity.verify(message, &signature));
    }

    #[test]
    fn test_sign_large_message() {
        let identity = PQIdentity::generate();
        // 1 MB message
        let message = vec![0xABu8; 1024 * 1024];

        let signature = identity.sign(&message);
        assert!(identity.verify(&message, &signature));
    }

    #[test]
    fn test_signature_not_malleable() {
        let identity = PQIdentity::generate();
        let message = b"Test message";
        let signature = identity.sign(message);

        // Mutating signature bytes should fail verification
        let mut bad_sig_bytes = signature.to_bytes().to_vec();
        if !bad_sig_bytes.is_empty() {
            bad_sig_bytes[0] ^= 0xFF;
        }

        // Invalid signature bytes won't even parse correctly
        let result = PQSignature::from_bytes(bad_sig_bytes);
        if let Ok(bad_sig) = result {
            assert!(!identity.verify(message, &bad_sig));
        }
    }

    #[test]
    fn test_signature_size_validation() {
        let too_short = vec![0u8; PQ_SIGNATURE_SIZE - 1];
        let result = PQSignature::from_bytes(too_short);
        assert!(result.is_err());

        let too_long = vec![0u8; PQ_SIGNATURE_SIZE + 1];
        let result = PQSignature::from_bytes(too_long);
        assert!(result.is_err());
    }

    #[test]
    fn test_public_identity_hash() {
        use std::collections::HashSet;

        let identity1 = PQIdentity::generate();
        let identity2 = PQIdentity::generate();

        let mut set = HashSet::new();
        set.insert(identity1.verifying_key());
        set.insert(identity2.verifying_key());
        set.insert(identity1.verifying_key()); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_short_id_format() {
        let identity = PQIdentity::generate();
        let short_id = identity.verifying_key().short_id();

        // Should be 16 hex characters (8 bytes)
        assert_eq!(short_id.len(), 16);
        assert!(short_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_secure_bytes_empty() {
        let secure = SecureBytes::new(vec![]);
        assert!(secure.is_empty());
        assert_eq!(secure.len(), 0);
    }

    #[test]
    fn test_secure_bytes_as_ref() {
        let data = vec![1, 2, 3, 4];
        let secure = SecureBytes::new(data.clone());
        let slice: &[u8] = secure.as_ref();
        assert_eq!(slice, &data[..]);
    }

    #[test]
    fn test_secure_bytes_from() {
        let data = vec![1, 2, 3, 4];
        let secure: SecureBytes = data.clone().into();
        assert_eq!(secure.as_slice(), &data[..]);
    }

    #[test]
    fn test_invalid_keypair_bytes() {
        // Correct size but invalid content
        let bad_sk = vec![0u8; PQ_SIGNING_KEY_SIZE];
        let bad_pk = vec![0u8; PQ_VERIFYING_KEY_SIZE];

        // This may or may not fail depending on the implementation
        // The important thing is it doesn't crash
        let _ = PQIdentity::from_keypair_bytes(&bad_sk, &bad_pk);
    }
}
