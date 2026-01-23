//! Interface key management for N-peer interfaces
//!
//! Provides shared symmetric key encryption using ChaCha20-Poly1305
//! for encrypting events within an interface.
//!
//! Key exchange uses ML-KEM-768 (post-quantum secure) for encapsulating
//! interface keys to new members.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use indras_core::InterfaceId;

use crate::error::CryptoError;
use crate::pq_kem::{PQEncapsulationKey, PQKemKeyPair, PQCiphertext};

/// Nonce size for ChaCha20-Poly1305 (12 bytes)
pub const NONCE_SIZE: usize = 12;

/// Key size (32 bytes)
pub const KEY_SIZE: usize = 32;

/// Shared symmetric key for an interface
///
/// All members of an interface share this key, enabling them to
/// encrypt and decrypt events within the interface.
#[derive(Clone)]
pub struct InterfaceKey {
    /// The raw key bytes
    key: [u8; KEY_SIZE],
    /// The interface this key belongs to
    interface_id: InterfaceId,
}

impl InterfaceKey {
    /// Generate a new random interface key
    pub fn generate(interface_id: InterfaceId) -> Self {
        let mut key = [0u8; KEY_SIZE];
        rand::rng().fill_bytes(&mut key);
        Self { key, interface_id }
    }

    /// Create from raw key bytes
    pub fn from_bytes(key: [u8; KEY_SIZE], interface_id: InterfaceId) -> Self {
        Self { key, interface_id }
    }

    /// Derive from a seed (for deterministic interfaces)
    ///
    /// Useful for creating test interfaces with known keys.
    pub fn from_seed(seed: &[u8; 32], interface_id: InterfaceId) -> Self {
        // Use HKDF-style derivation: XOR seed with interface_id
        let mut key = [0u8; KEY_SIZE];
        for i in 0..KEY_SIZE {
            key[i] = seed[i] ^ interface_id.as_bytes()[i];
        }
        Self { key, interface_id }
    }

    /// Get the interface ID this key is for
    pub fn interface_id(&self) -> InterfaceId {
        self.interface_id
    }

    /// Get the raw key bytes (use with caution)
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.key
    }

    /// Encrypt data for the interface
    ///
    /// Returns the nonce concatenated with the ciphertext.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData, CryptoError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        Ok(EncryptedData {
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt data from the interface
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>, CryptoError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        let nonce = Nonce::from_slice(&encrypted.nonce);

        cipher
            .decrypt(nonce, encrypted.ciphertext.as_slice())
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }

    /// Decrypt from raw bytes (nonce + ciphertext)
    pub fn decrypt_bytes(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if data.len() < NONCE_SIZE {
            return Err(CryptoError::DecryptionFailed(
                "Data too short for nonce".to_string(),
            ));
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&data[..NONCE_SIZE]);

        let encrypted = EncryptedData {
            nonce,
            ciphertext: data[NONCE_SIZE..].to_vec(),
        };

        self.decrypt(&encrypted)
    }

    /// Encapsulate this interface key for a recipient using ML-KEM-768
    ///
    /// Uses post-quantum key encapsulation to securely share the interface key.
    /// The recipient needs their decapsulation key to recover the interface key.
    pub fn encapsulate_for(
        &self,
        recipient_encapsulation_key: &PQEncapsulationKey,
    ) -> Result<EncapsulatedKey, CryptoError> {
        // Encapsulate to get shared secret and ciphertext
        let (ciphertext, shared_secret) = recipient_encapsulation_key.encapsulate();

        // Use shared secret to encrypt the interface key
        let cipher = ChaCha20Poly1305::new_from_slice(&shared_secret)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        // Generate nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the interface key
        let encrypted_key = cipher
            .encrypt(nonce, self.key.as_slice())
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        Ok(EncapsulatedKey {
            interface_id: self.interface_id,
            kem_ciphertext: ciphertext.into_bytes(),
            encrypted_key,
            nonce: nonce_bytes,
        })
    }

    /// Decapsulate to recover the interface key using ML-KEM-768
    ///
    /// Uses the recipient's KEM key pair to recover the shared secret,
    /// then decrypts the interface key.
    pub fn decapsulate(
        encapsulated: &EncapsulatedKey,
        our_kem_keypair: &PQKemKeyPair,
    ) -> Result<Self, CryptoError> {
        // Reconstruct ciphertext
        let ciphertext = PQCiphertext::from_bytes(encapsulated.kem_ciphertext.clone())
            .map_err(|e| CryptoError::PQDecapsulationFailed(e.to_string()))?;

        // Decapsulate to get shared secret
        let shared_secret = our_kem_keypair.decapsulate(&ciphertext)
            .map_err(|e| CryptoError::PQDecapsulationFailed(e.to_string()))?;

        // Decrypt the interface key
        let cipher = ChaCha20Poly1305::new_from_slice(&shared_secret)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        let nonce = Nonce::from_slice(&encapsulated.nonce);

        let key_bytes = cipher
            .decrypt(nonce, encapsulated.encrypted_key.as_slice())
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        if key_bytes.len() != KEY_SIZE {
            return Err(CryptoError::InvalidKey(
                "Decrypted key has wrong length".to_string(),
            ));
        }

        let mut key = [0u8; KEY_SIZE];
        key.copy_from_slice(&key_bytes);

        Ok(Self {
            key,
            interface_id: encapsulated.interface_id,
        })
    }
}

/// Encrypted data with nonce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// Nonce used for encryption
    pub nonce: [u8; NONCE_SIZE],
    /// The encrypted ciphertext
    pub ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// Convert to bytes (nonce || ciphertext)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(NONCE_SIZE + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Parse from bytes (nonce || ciphertext)
    pub fn from_bytes(data: &[u8]) -> Result<Self, CryptoError> {
        if data.len() < NONCE_SIZE {
            return Err(CryptoError::DecryptionFailed(
                "Data too short for nonce".to_string(),
            ));
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&data[..NONCE_SIZE]);

        Ok(Self {
            nonce,
            ciphertext: data[NONCE_SIZE..].to_vec(),
        })
    }
}

/// Encapsulated key for sharing with new members (post-quantum secure)
///
/// Uses ML-KEM-768 for key encapsulation, replacing the previous X25519-based
/// key exchange. The KEM ciphertext is ~1,088 bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncapsulatedKey {
    /// The interface this key is for
    pub interface_id: InterfaceId,
    /// ML-KEM-768 ciphertext (~1,088 bytes)
    pub kem_ciphertext: Vec<u8>,
    /// The encrypted interface key (encrypted with derived shared secret)
    pub encrypted_key: Vec<u8>,
    /// Nonce used for symmetric encryption
    pub nonce: [u8; NONCE_SIZE],
}

impl EncapsulatedKey {
    /// Serialize to bytes for transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        postcard::to_allocvec(self).map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, CryptoError> {
        postcard::from_bytes(data).map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

/// Legacy exported key struct (deprecated - use EncapsulatedKey instead)
///
/// Kept for backward compatibility during migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(since = "0.2.0", note = "Use EncapsulatedKey with ML-KEM instead")]
pub struct ExportedKey {
    /// The interface this key is for
    pub interface_id: InterfaceId,
    /// The encrypted interface key
    pub encrypted_key: Vec<u8>,
    /// Nonce used for encryption
    pub nonce: [u8; NONCE_SIZE],
    /// Public key of the sender (for key derivation)
    pub sender_public: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_interface_id() -> InterfaceId {
        InterfaceId::new([0x42; 32])
    }

    #[test]
    fn test_key_generation() {
        let id = test_interface_id();
        let key1 = InterfaceKey::generate(id);
        let key2 = InterfaceKey::generate(id);

        // Keys should be different
        assert_ne!(key1.as_bytes(), key2.as_bytes());
        assert_eq!(key1.interface_id(), id);
    }

    #[test]
    fn test_encrypt_decrypt() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let plaintext = b"Hello, encrypted world!";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_bytes() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let plaintext = b"Test message";
        let encrypted = key.encrypt(plaintext).unwrap();
        let bytes = encrypted.to_bytes();

        let decrypted = key.decrypt_bytes(&bytes).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let plaintext = b"Same plaintext";
        let encrypted1 = key.encrypt(plaintext).unwrap();
        let encrypted2 = key.encrypt(plaintext).unwrap();

        // Same plaintext should produce different ciphertext (different nonces)
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // But both should decrypt correctly
        assert_eq!(key.decrypt(&encrypted1).unwrap(), plaintext);
        assert_eq!(key.decrypt(&encrypted2).unwrap(), plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let id = test_interface_id();
        let key1 = InterfaceKey::generate(id);
        let key2 = InterfaceKey::generate(id);

        let plaintext = b"Secret message";
        let encrypted = key1.encrypt(plaintext).unwrap();

        // Decrypting with wrong key should fail
        let result = key2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_pq_key_encapsulation() {
        let id = test_interface_id();
        let original_key = InterfaceKey::generate(id);

        // Bob generates a KEM key pair
        let bob_kem = PQKemKeyPair::generate();
        let bob_encap_key = bob_kem.encapsulation_key();

        // Alice encapsulates the interface key for Bob
        let encapsulated = original_key.encapsulate_for(&bob_encap_key).unwrap();

        // Bob decapsulates to recover the key
        let recovered_key = InterfaceKey::decapsulate(&encapsulated, &bob_kem).unwrap();

        // Keys should match
        assert_eq!(original_key.as_bytes(), recovered_key.as_bytes());
        assert_eq!(original_key.interface_id(), recovered_key.interface_id());

        // Test encryption with original, decryption with recovered
        let plaintext = b"Quantum-secure secret message";
        let encrypted = original_key.encrypt(plaintext).unwrap();
        let decrypted = recovered_key.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_pq_wrong_keypair_fails() {
        let id = test_interface_id();
        let original_key = InterfaceKey::generate(id);

        // Bob and Eve generate KEM key pairs
        let bob_kem = PQKemKeyPair::generate();
        let eve_kem = PQKemKeyPair::generate();

        // Alice encapsulates the interface key for Bob
        let encapsulated = original_key.encapsulate_for(&bob_kem.encapsulation_key()).unwrap();

        // Eve tries to decapsulate (will get wrong shared secret due to ML-KEM implicit rejection)
        let result = InterfaceKey::decapsulate(&encapsulated, &eve_kem);

        // Should fail because Eve's derived shared secret is different
        assert!(result.is_err());
    }

    #[test]
    fn test_from_seed_deterministic() {
        let id = test_interface_id();
        let seed = [0xAB; 32];

        let key1 = InterfaceKey::from_seed(&seed, id);
        let key2 = InterfaceKey::from_seed(&seed, id);

        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_encrypted_data_serialization() {
        let encrypted = EncryptedData {
            nonce: [1; NONCE_SIZE],
            ciphertext: vec![10, 20, 30, 40],
        };

        let bytes = encrypted.to_bytes();
        let parsed = EncryptedData::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.nonce, encrypted.nonce);
        assert_eq!(parsed.ciphertext, encrypted.ciphertext);
    }

    #[test]
    fn test_encapsulated_key_serialization() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);
        let bob_kem = PQKemKeyPair::generate();

        let encapsulated = key.encapsulate_for(&bob_kem.encapsulation_key()).unwrap();

        // Serialize and deserialize
        let bytes = encapsulated.to_bytes().unwrap();
        let parsed = EncapsulatedKey::from_bytes(&bytes).unwrap();

        // Should be able to recover key
        let recovered = InterfaceKey::decapsulate(&parsed, &bob_kem).unwrap();
        assert_eq!(key.as_bytes(), recovered.as_bytes());
    }

    // ========== Additional Edge Case Tests ==========

    #[test]
    fn test_encrypt_empty_data() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let plaintext = b"";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_large_data() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        // 1 MB of data
        let plaintext = vec![0xABu8; 1024 * 1024];
        let encrypted = key.encrypt(&plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_invalid_data_too_short() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        // Data shorter than nonce (12 bytes)
        let result = key.decrypt_bytes(&[0u8; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_invalid_ciphertext() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        // Valid nonce but garbage ciphertext
        let mut data = vec![0u8; NONCE_SIZE + 20];
        data[..NONCE_SIZE].copy_from_slice(&[0u8; NONCE_SIZE]);

        let result = key.decrypt_bytes(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_tampered_ciphertext() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let plaintext = b"Secret message";
        let encrypted = key.encrypt(plaintext).unwrap();

        // Tamper with the ciphertext
        let mut tampered = encrypted.clone();
        if !tampered.ciphertext.is_empty() {
            tampered.ciphertext[0] ^= 0xFF;
        }

        let result = key.decrypt(&tampered);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_tampered_nonce() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let plaintext = b"Secret message";
        let encrypted = key.encrypt(plaintext).unwrap();

        // Tamper with the nonce
        let mut tampered = encrypted.clone();
        tampered.nonce[0] ^= 0xFF;

        let result = key.decrypt(&tampered);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_seed_different_interfaces() {
        let seed = [0xAB; 32];
        let id1 = InterfaceId::new([0x01; 32]);
        let id2 = InterfaceId::new([0x02; 32]);

        let key1 = InterfaceKey::from_seed(&seed, id1);
        let key2 = InterfaceKey::from_seed(&seed, id2);

        // Different interface IDs should produce different keys
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_encrypted_data_from_bytes_round_trip() {
        let data = EncryptedData {
            nonce: [0x42; NONCE_SIZE],
            ciphertext: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        };

        let bytes = data.to_bytes();
        let parsed = EncryptedData::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.nonce, data.nonce);
        assert_eq!(parsed.ciphertext, data.ciphertext);
    }

    #[test]
    fn test_encrypted_data_from_bytes_too_short() {
        // Less than NONCE_SIZE bytes
        let result = EncryptedData::from_bytes(&[0u8; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_recipients_same_key() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        // Multiple recipients
        let bob_kem = PQKemKeyPair::generate();
        let carol_kem = PQKemKeyPair::generate();

        let encap_bob = key.encapsulate_for(&bob_kem.encapsulation_key()).unwrap();
        let encap_carol = key.encapsulate_for(&carol_kem.encapsulation_key()).unwrap();

        // Both should recover the same key
        let key_bob = InterfaceKey::decapsulate(&encap_bob, &bob_kem).unwrap();
        let key_carol = InterfaceKey::decapsulate(&encap_carol, &carol_kem).unwrap();

        assert_eq!(key.as_bytes(), key_bob.as_bytes());
        assert_eq!(key.as_bytes(), key_carol.as_bytes());

        // Test cross-encryption/decryption
        let plaintext = b"Group message";
        let encrypted = key_bob.encrypt(plaintext).unwrap();
        let decrypted = key_carol.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
