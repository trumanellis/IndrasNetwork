//! Interface key management for N-peer interfaces
//!
//! Provides shared symmetric key encryption using ChaCha20-Poly1305
//! for encrypting events within an interface.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use indras_core::InterfaceId;

use crate::error::CryptoError;

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

    /// Export key for sharing with a new member
    ///
    /// Uses X25519 ECDH to create a shared secret, then encrypts the
    /// interface key with that shared secret.
    pub fn export_for(
        &self,
        recipient_public: &PublicKey,
        our_secret: &StaticSecret,
    ) -> Result<ExportedKey, CryptoError> {
        // Derive shared secret using X25519
        let shared_secret = our_secret.diffie_hellman(recipient_public);

        // Use shared secret as encryption key
        let cipher = ChaCha20Poly1305::new_from_slice(shared_secret.as_bytes())
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        // Generate nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the interface key
        let encrypted_key = cipher
            .encrypt(nonce, self.key.as_slice())
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        Ok(ExportedKey {
            interface_id: self.interface_id,
            encrypted_key,
            nonce: nonce_bytes,
            sender_public: PublicKey::from(our_secret).to_bytes(),
        })
    }

    /// Import a key shared by an existing member
    pub fn import_from(
        exported: &ExportedKey,
        our_secret: &StaticSecret,
    ) -> Result<Self, CryptoError> {
        // Reconstruct sender's public key
        let sender_public = PublicKey::from(exported.sender_public);

        // Derive shared secret
        let shared_secret = our_secret.diffie_hellman(&sender_public);

        // Decrypt the interface key
        let cipher = ChaCha20Poly1305::new_from_slice(shared_secret.as_bytes())
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        let nonce = Nonce::from_slice(&exported.nonce);

        let key_bytes = cipher
            .decrypt(nonce, exported.encrypted_key.as_slice())
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
            interface_id: exported.interface_id,
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

/// Exported key for sharing with new members
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    use rand::RngCore;

    fn test_interface_id() -> InterfaceId {
        InterfaceId::new([0x42; 32])
    }

    /// Generate a random StaticSecret (compatible with x25519-dalek's rand_core version)
    fn random_secret() -> StaticSecret {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        StaticSecret::from(bytes)
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
    fn test_key_export_import() {
        let id = test_interface_id();
        let original_key = InterfaceKey::generate(id);

        // Generate keypairs for Alice (exporter) and Bob (recipient)
        let alice_secret = random_secret();
        let bob_secret = random_secret();
        let bob_public = PublicKey::from(&bob_secret);

        // Alice exports the key for Bob
        let exported = original_key.export_for(&bob_public, &alice_secret).unwrap();

        // Bob imports the key
        let imported_key = InterfaceKey::import_from(&exported, &bob_secret).unwrap();

        // Keys should match
        assert_eq!(original_key.as_bytes(), imported_key.as_bytes());
        assert_eq!(original_key.interface_id(), imported_key.interface_id());

        // Test encryption with original, decryption with imported
        let plaintext = b"Shared secret message";
        let encrypted = original_key.encrypt(plaintext).unwrap();
        let decrypted = imported_key.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
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
}
