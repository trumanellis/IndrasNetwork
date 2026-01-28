//! Per-artifact encryption for revocable sharing.
//!
//! This module provides encryption and decryption functions for artifacts
//! using ChaCha20-Poly1305 with per-artifact random keys. Each artifact
//! gets its own encryption key, enabling fine-grained revocation.
//!
//! ## Security Model
//!
//! - Each artifact is encrypted with a unique random 256-bit key
//! - The artifact key is then encrypted with the realm's interface key
//! - Revocation deletes the artifact key, making content permanently unreadable
//! - Uses AEAD (ChaCha20-Poly1305) for authenticated encryption
//!
//! ## Usage
//!
//! ```rust,ignore
//! use indras_crypto::{encrypt_artifact, decrypt_artifact, generate_artifact_key};
//!
//! // Encrypt an artifact
//! let content = b"Secret document content";
//! let key = generate_artifact_key();
//! let encrypted = encrypt_artifact(content, &key)?;
//!
//! // Later, decrypt with the same key
//! let decrypted = decrypt_artifact(&encrypted, &key)?;
//! assert_eq!(decrypted, content);
//! ```

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::CryptoError;
use crate::interface_key::{InterfaceKey, NONCE_SIZE};

/// Size of artifact encryption keys (256 bits for ChaCha20).
pub const ARTIFACT_KEY_SIZE: usize = 32;

/// Per-artifact encryption key.
pub type ArtifactKey = [u8; ARTIFACT_KEY_SIZE];

/// Generate a new random artifact encryption key.
///
/// Uses the system's cryptographically secure random number generator.
pub fn generate_artifact_key() -> ArtifactKey {
    let mut key = [0u8; ARTIFACT_KEY_SIZE];
    rand::rng().fill_bytes(&mut key);
    key
}

/// Encrypted artifact content with nonce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedArtifact {
    /// Nonce used for encryption (12 bytes).
    pub nonce: [u8; NONCE_SIZE],
    /// Encrypted content with authentication tag.
    pub ciphertext: Vec<u8>,
}

impl EncryptedArtifact {
    /// Convert to bytes (nonce || ciphertext).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(NONCE_SIZE + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Parse from bytes (nonce || ciphertext).
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

    /// Get the total size of the encrypted data.
    pub fn size(&self) -> usize {
        NONCE_SIZE + self.ciphertext.len()
    }
}

/// Encrypt artifact content with a per-artifact key.
///
/// Uses ChaCha20-Poly1305 AEAD for authenticated encryption.
/// Each call generates a random nonce for the encryption.
///
/// # Arguments
///
/// * `plaintext` - The artifact content to encrypt
/// * `key` - The per-artifact encryption key
///
/// # Returns
///
/// The encrypted artifact with nonce and ciphertext.
pub fn encrypt_artifact(plaintext: &[u8], key: &ArtifactKey) -> Result<EncryptedArtifact, CryptoError> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    // Generate random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    Ok(EncryptedArtifact {
        nonce: nonce_bytes,
        ciphertext,
    })
}

/// Decrypt artifact content with a per-artifact key.
///
/// # Arguments
///
/// * `encrypted` - The encrypted artifact data
/// * `key` - The per-artifact encryption key
///
/// # Returns
///
/// The decrypted plaintext content.
///
/// # Errors
///
/// Returns an error if:
/// - The key is incorrect
/// - The ciphertext has been tampered with
/// - The data format is invalid
pub fn decrypt_artifact(encrypted: &EncryptedArtifact, key: &ArtifactKey) -> Result<Vec<u8>, CryptoError> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

    let nonce = Nonce::from_slice(&encrypted.nonce);

    cipher
        .decrypt(nonce, encrypted.ciphertext.as_slice())
        .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
}

/// Decrypt artifact content from raw bytes.
///
/// Convenience function that parses the encrypted data and decrypts it.
pub fn decrypt_artifact_bytes(data: &[u8], key: &ArtifactKey) -> Result<Vec<u8>, CryptoError> {
    let encrypted = EncryptedArtifact::from_bytes(data)?;
    decrypt_artifact(&encrypted, key)
}

/// Encrypted artifact key for storage in the registry.
///
/// The artifact key is encrypted with the realm's interface key
/// so it can be safely stored and synchronized via CRDT.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptedArtifactKey {
    /// Nonce used for encryption (12 bytes).
    pub nonce: [u8; NONCE_SIZE],
    /// Encrypted key data (32 bytes key + 16 bytes auth tag = 48 bytes).
    pub ciphertext: Vec<u8>,
}

impl EncryptedArtifactKey {
    /// Encrypt an artifact key with an interface key.
    ///
    /// The artifact key is encrypted so it can be safely stored
    /// in the realm's key registry CRDT document.
    pub fn encrypt(artifact_key: &ArtifactKey, interface_key: &InterfaceKey) -> Result<Self, CryptoError> {
        let encrypted_data = interface_key.encrypt(artifact_key)?;
        Ok(Self {
            nonce: encrypted_data.nonce,
            ciphertext: encrypted_data.ciphertext,
        })
    }

    /// Decrypt the artifact key using an interface key.
    ///
    /// Returns the per-artifact encryption key.
    pub fn decrypt(&self, interface_key: &InterfaceKey) -> Result<ArtifactKey, CryptoError> {
        let encrypted_data = crate::interface_key::EncryptedData {
            nonce: self.nonce,
            ciphertext: self.ciphertext.clone(),
        };

        let decrypted = interface_key.decrypt(&encrypted_data)?;

        if decrypted.len() != ARTIFACT_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Decrypted key has wrong length: {} (expected {})",
                decrypted.len(),
                ARTIFACT_KEY_SIZE
            )));
        }

        let mut key = [0u8; ARTIFACT_KEY_SIZE];
        key.copy_from_slice(&decrypted);
        Ok(key)
    }

    /// Get the total size of the encrypted key.
    pub fn size(&self) -> usize {
        NONCE_SIZE + self.ciphertext.len()
    }
}

/// Calculate the BLAKE3 hash of content.
///
/// Used for content-addressing encrypted artifacts.
pub fn hash_content(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::InterfaceId;

    #[test]
    fn test_generate_artifact_key() {
        let key1 = generate_artifact_key();
        let key2 = generate_artifact_key();

        // Keys should be different
        assert_ne!(key1, key2);

        // Keys should have correct length
        assert_eq!(key1.len(), ARTIFACT_KEY_SIZE);
    }

    #[test]
    fn test_encrypt_decrypt_artifact() {
        let key = generate_artifact_key();
        let plaintext = b"Hello, encrypted artifact!";

        let encrypted = encrypt_artifact(plaintext, &key).unwrap();
        let decrypted = decrypt_artifact(&encrypted, &key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_empty_content() {
        let key = generate_artifact_key();
        let plaintext = b"";

        let encrypted = encrypt_artifact(plaintext, &key).unwrap();
        let decrypted = decrypt_artifact(&encrypted, &key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_large_content() {
        let key = generate_artifact_key();
        let plaintext = vec![0xABu8; 1024 * 1024]; // 1 MB

        let encrypted = encrypt_artifact(&plaintext, &key).unwrap();
        let decrypted = decrypt_artifact(&encrypted, &key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_artifact_key();
        let key2 = generate_artifact_key();
        let plaintext = b"Secret content";

        let encrypted = encrypt_artifact(plaintext, &key1).unwrap();
        let result = decrypt_artifact(&encrypted, &key2);

        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = generate_artifact_key();
        let plaintext = b"Secret content";

        let mut encrypted = encrypt_artifact(plaintext, &key).unwrap();

        // Tamper with ciphertext
        if !encrypted.ciphertext.is_empty() {
            encrypted.ciphertext[0] ^= 0xFF;
        }

        let result = decrypt_artifact(&encrypted, &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_artifact_to_from_bytes() {
        let key = generate_artifact_key();
        let plaintext = b"Test content";

        let encrypted = encrypt_artifact(plaintext, &key).unwrap();
        let bytes = encrypted.to_bytes();
        let parsed = EncryptedArtifact::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.nonce, encrypted.nonce);
        assert_eq!(parsed.ciphertext, encrypted.ciphertext);

        // Should still decrypt correctly
        let decrypted = decrypt_artifact(&parsed, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypted_artifact_key() {
        let interface_id = InterfaceId::new([0x42; 32]);
        let interface_key = InterfaceKey::generate(interface_id);
        let artifact_key = generate_artifact_key();

        // Encrypt the artifact key
        let encrypted = EncryptedArtifactKey::encrypt(&artifact_key, &interface_key).unwrap();

        // Decrypt and verify
        let decrypted = encrypted.decrypt(&interface_key).unwrap();
        assert_eq!(decrypted, artifact_key);
    }

    #[test]
    fn test_encrypted_artifact_key_wrong_interface_key() {
        let interface_id = InterfaceId::new([0x42; 32]);
        let interface_key1 = InterfaceKey::generate(interface_id);
        let interface_key2 = InterfaceKey::generate(interface_id);
        let artifact_key = generate_artifact_key();

        // Encrypt with key1
        let encrypted = EncryptedArtifactKey::encrypt(&artifact_key, &interface_key1).unwrap();

        // Try to decrypt with key2
        let result = encrypted.decrypt(&interface_key2);
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_content() {
        let data = b"Hello, world!";
        let hash1 = hash_content(data);
        let hash2 = hash_content(data);

        // Same data should produce same hash
        assert_eq!(hash1, hash2);

        // Different data should produce different hash
        let hash3 = hash_content(b"Different content");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_decrypt_artifact_bytes() {
        let key = generate_artifact_key();
        let plaintext = b"Test decryption from bytes";

        let encrypted = encrypt_artifact(plaintext, &key).unwrap();
        let bytes = encrypted.to_bytes();

        let decrypted = decrypt_artifact_bytes(&bytes, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces_produce_different_ciphertext() {
        let key = generate_artifact_key();
        let plaintext = b"Same content";

        let encrypted1 = encrypt_artifact(plaintext, &key).unwrap();
        let encrypted2 = encrypt_artifact(plaintext, &key).unwrap();

        // Nonces should be different
        assert_ne!(encrypted1.nonce, encrypted2.nonce);

        // Ciphertexts should be different
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // Both should decrypt correctly
        assert_eq!(decrypt_artifact(&encrypted1, &key).unwrap(), plaintext);
        assert_eq!(decrypt_artifact(&encrypted2, &key).unwrap(), plaintext);
    }
}
