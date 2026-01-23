//! Key distribution for N-peer interface member onboarding
//!
//! Provides secure key sharing when inviting new members to an interface.
//! Uses ML-KEM-768 (post-quantum secure) for key encapsulation.

use serde::{Deserialize, Serialize};

use indras_core::InterfaceId;

use crate::error::CryptoError;
use crate::interface_key::{InterfaceKey, NONCE_SIZE};
use crate::pq_kem::{PQEncapsulationKey, PQKemKeyPair, PQ_CIPHERTEXT_SIZE};

/// Key distribution utilities for member onboarding
///
/// Uses ML-KEM-768 for post-quantum secure key encapsulation.
pub struct KeyDistribution;

impl KeyDistribution {
    /// Create an invite for a new member using ML-KEM
    ///
    /// Encapsulates the interface key to the invitee's public encapsulation key
    /// so only they can decapsulate it.
    pub fn create_invite(
        interface_key: &InterfaceKey,
        invitee_encapsulation_key: &PQEncapsulationKey,
    ) -> Result<KeyInvite, CryptoError> {
        let encapsulated = interface_key.encapsulate_for(invitee_encapsulation_key)?;

        Ok(KeyInvite {
            interface_id: interface_key.interface_id(),
            kem_ciphertext: encapsulated.kem_ciphertext,
            encrypted_key: encapsulated.encrypted_key,
            nonce: encapsulated.nonce,
        })
    }

    /// Accept an invite and extract the interface key using ML-KEM
    pub fn accept_invite(
        invite: &KeyInvite,
        our_kem_keypair: &PQKemKeyPair,
    ) -> Result<InterfaceKey, CryptoError> {
        // Reconstruct encapsulated key format
        let encapsulated = crate::interface_key::EncapsulatedKey {
            interface_id: invite.interface_id,
            kem_ciphertext: invite.kem_ciphertext.clone(),
            encrypted_key: invite.encrypted_key.clone(),
            nonce: invite.nonce,
        };

        InterfaceKey::decapsulate(&encapsulated, our_kem_keypair)
    }

    /// Create an invite from raw key bytes (for manual key sharing)
    pub fn create_invite_from_bytes(
        key_bytes: &[u8; 32],
        interface_id: InterfaceId,
        invitee_encapsulation_key: &PQEncapsulationKey,
    ) -> Result<KeyInvite, CryptoError> {
        let key = InterfaceKey::from_bytes(*key_bytes, interface_id);
        Self::create_invite(&key, invitee_encapsulation_key)
    }
}

/// An invitation to join an interface (post-quantum secure)
///
/// Contains the ML-KEM encapsulated interface key that only the invitee can decapsulate.
///
/// ## Size
///
/// - KEM ciphertext: ~1,088 bytes
/// - Encrypted key: ~48 bytes (32-byte key + 16-byte auth tag)
/// - Total: ~1,200 bytes per invite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInvite {
    /// The interface this invite is for
    pub interface_id: InterfaceId,
    /// ML-KEM-768 ciphertext (~1,088 bytes)
    pub kem_ciphertext: Vec<u8>,
    /// The encrypted interface key (encrypted with shared secret)
    pub encrypted_key: Vec<u8>,
    /// Nonce used for symmetric encryption
    pub nonce: [u8; NONCE_SIZE],
}

impl KeyInvite {
    /// Serialize the invite for transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        postcard::to_allocvec(self).map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Deserialize an invite from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, CryptoError> {
        postcard::from_bytes(data).map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }

    /// Check if the KEM ciphertext has valid length
    pub fn is_valid_ciphertext_length(&self) -> bool {
        self.kem_ciphertext.len() == PQ_CIPHERTEXT_SIZE
    }
}

/// Metadata that can be attached to an invite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteMetadata {
    /// Human-readable interface name
    pub interface_name: Option<String>,
    /// Description of the interface
    pub description: Option<String>,
    /// Inviter's display name
    pub inviter_name: Option<String>,
    /// When the invite was created (Unix millis)
    pub created_at_millis: i64,
    /// When the invite expires (Unix millis, optional)
    pub expires_at_millis: Option<i64>,
}

impl Default for InviteMetadata {
    fn default() -> Self {
        Self {
            interface_name: None,
            description: None,
            inviter_name: None,
            created_at_millis: chrono::Utc::now().timestamp_millis(),
            expires_at_millis: None,
        }
    }
}

impl InviteMetadata {
    /// Create new metadata with current timestamp
    pub fn new() -> Self {
        Self::default()
    }

    /// Set interface name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.interface_name = Some(name.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set inviter name
    pub fn with_inviter(mut self, name: impl Into<String>) -> Self {
        self.inviter_name = Some(name.into());
        self
    }

    /// Set expiration time
    pub fn expires_in_hours(mut self, hours: i64) -> Self {
        let millis = hours * 60 * 60 * 1000;
        self.expires_at_millis = Some(self.created_at_millis + millis);
        self
    }

    /// Check if the invite has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at_millis {
            chrono::Utc::now().timestamp_millis() > expires
        } else {
            false
        }
    }
}

/// Full invite with key and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullInvite {
    /// The key invite
    pub key_invite: KeyInvite,
    /// Optional metadata
    pub metadata: Option<InviteMetadata>,
}

impl FullInvite {
    /// Create a new full invite
    pub fn new(key_invite: KeyInvite) -> Self {
        Self {
            key_invite,
            metadata: None,
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: InviteMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Serialize for transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        postcard::to_allocvec(self).map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Deserialize
    pub fn from_bytes(data: &[u8]) -> Result<Self, CryptoError> {
        postcard::from_bytes(data).map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_interface_id() -> InterfaceId {
        InterfaceId::new([0x42; 32])
    }

    #[test]
    fn test_create_and_accept_invite() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        // Bob generates a KEM key pair
        let bob_kem = PQKemKeyPair::generate();
        let bob_encap_key = bob_kem.encapsulation_key();

        // Alice creates invite for Bob
        let invite = KeyDistribution::create_invite(&key, &bob_encap_key).unwrap();

        // Verify ciphertext length
        assert!(invite.is_valid_ciphertext_length());

        // Bob accepts the invite
        let imported_key = KeyDistribution::accept_invite(&invite, &bob_kem).unwrap();

        // Keys should match
        assert_eq!(key.as_bytes(), imported_key.as_bytes());

        // Test encryption/decryption across the keys
        let plaintext = b"Quantum-secure secret message";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = imported_key.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_recipient_cannot_accept() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        // Bob and Eve generate KEM key pairs
        let bob_kem = PQKemKeyPair::generate();
        let eve_kem = PQKemKeyPair::generate();

        // Alice creates invite for Bob
        let invite = KeyDistribution::create_invite(&key, &bob_kem.encapsulation_key()).unwrap();

        // Eve tries to accept Bob's invite
        let result = KeyDistribution::accept_invite(&invite, &eve_kem);
        assert!(result.is_err());
    }

    #[test]
    fn test_invite_serialization() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let bob_kem = PQKemKeyPair::generate();
        let invite = KeyDistribution::create_invite(&key, &bob_kem.encapsulation_key()).unwrap();

        // Serialize and deserialize
        let bytes = invite.to_bytes().unwrap();
        let parsed = KeyInvite::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.interface_id, invite.interface_id);
        assert_eq!(parsed.kem_ciphertext, invite.kem_ciphertext);
        assert_eq!(parsed.encrypted_key, invite.encrypted_key);
        assert_eq!(parsed.nonce, invite.nonce);

        // Should still work after serialization
        let imported_key = KeyDistribution::accept_invite(&parsed, &bob_kem).unwrap();
        assert_eq!(key.as_bytes(), imported_key.as_bytes());
    }

    #[test]
    fn test_invite_metadata() {
        let metadata = InviteMetadata::new()
            .with_name("Test Interface")
            .with_description("A test interface")
            .with_inviter("Alice")
            .expires_in_hours(24);

        assert_eq!(metadata.interface_name, Some("Test Interface".to_string()));
        assert!(!metadata.is_expired());
        assert!(metadata.expires_at_millis.is_some());
    }

    #[test]
    fn test_full_invite() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let bob_kem = PQKemKeyPair::generate();
        let key_invite = KeyDistribution::create_invite(&key, &bob_kem.encapsulation_key()).unwrap();
        let metadata = InviteMetadata::new().with_name("My Interface");

        let full_invite = FullInvite::new(key_invite).with_metadata(metadata);

        let bytes = full_invite.to_bytes().unwrap();
        let parsed = FullInvite::from_bytes(&bytes).unwrap();

        assert!(parsed.metadata.is_some());
        assert_eq!(
            parsed.metadata.unwrap().interface_name,
            Some("My Interface".to_string())
        );
    }

    #[test]
    fn test_create_invite_from_bytes() {
        let id = test_interface_id();
        let key_bytes = [0x42u8; 32];

        let bob_kem = PQKemKeyPair::generate();
        let invite = KeyDistribution::create_invite_from_bytes(
            &key_bytes,
            id,
            &bob_kem.encapsulation_key(),
        ).unwrap();

        let recovered_key = KeyDistribution::accept_invite(&invite, &bob_kem).unwrap();
        assert_eq!(recovered_key.as_bytes(), &key_bytes);
    }
}
