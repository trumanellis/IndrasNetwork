//! Key distribution for N-peer interface member onboarding
//!
//! Provides secure key sharing when inviting new members to an interface.

use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use indras_core::InterfaceId;

use crate::error::CryptoError;
use crate::interface_key::{InterfaceKey, NONCE_SIZE};

/// Key distribution utilities for member onboarding
pub struct KeyDistribution;

impl KeyDistribution {
    /// Create an invite for a new member
    ///
    /// Encrypts the interface key to the invitee's public key so only they
    /// can decrypt it.
    pub fn create_invite(
        interface_key: &InterfaceKey,
        inviter_secret: &StaticSecret,
        invitee_public: &PublicKey,
    ) -> Result<KeyInvite, CryptoError> {
        let exported = interface_key.export_for(invitee_public, inviter_secret)?;

        Ok(KeyInvite {
            interface_id: interface_key.interface_id(),
            encrypted_key: exported.encrypted_key,
            nonce: exported.nonce,
            inviter_public: PublicKey::from(inviter_secret).to_bytes(),
        })
    }

    /// Accept an invite and extract the interface key
    pub fn accept_invite(
        invite: &KeyInvite,
        our_secret: &StaticSecret,
    ) -> Result<InterfaceKey, CryptoError> {
        // Reconstruct exported key format
        let exported = crate::interface_key::ExportedKey {
            interface_id: invite.interface_id,
            encrypted_key: invite.encrypted_key.clone(),
            nonce: invite.nonce,
            sender_public: invite.inviter_public,
        };

        InterfaceKey::import_from(&exported, our_secret)
    }

    /// Create an invite from raw key bytes (for manual key sharing)
    pub fn create_invite_from_bytes(
        key_bytes: &[u8; 32],
        interface_id: InterfaceId,
        inviter_secret: &StaticSecret,
        invitee_public: &PublicKey,
    ) -> Result<KeyInvite, CryptoError> {
        let key = InterfaceKey::from_bytes(*key_bytes, interface_id);
        Self::create_invite(&key, inviter_secret, invitee_public)
    }
}

/// An invitation to join an interface
///
/// Contains the encrypted interface key that only the invitee can decrypt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInvite {
    /// The interface this invite is for
    pub interface_id: InterfaceId,
    /// The encrypted interface key (encrypted to invitee's public key)
    pub encrypted_key: Vec<u8>,
    /// Nonce used for encryption
    pub nonce: [u8; NONCE_SIZE],
    /// Public key of the inviter (needed for key derivation)
    pub inviter_public: [u8; 32],
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

    /// Get the inviter's public key
    pub fn inviter_public_key(&self) -> PublicKey {
        PublicKey::from(self.inviter_public)
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
    fn test_create_and_accept_invite() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        // Generate keypairs
        let alice_secret = random_secret();
        let bob_secret = random_secret();
        let bob_public = PublicKey::from(&bob_secret);

        // Alice creates invite for Bob
        let invite = KeyDistribution::create_invite(&key, &alice_secret, &bob_public).unwrap();

        // Bob accepts the invite
        let imported_key = KeyDistribution::accept_invite(&invite, &bob_secret).unwrap();

        // Keys should match
        assert_eq!(key.as_bytes(), imported_key.as_bytes());

        // Test encryption/decryption across the keys
        let plaintext = b"Secret message";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = imported_key.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_recipient_cannot_accept() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let alice_secret = random_secret();
        let bob_secret = random_secret();
        let bob_public = PublicKey::from(&bob_secret);
        let eve_secret = random_secret();

        // Alice creates invite for Bob
        let invite = KeyDistribution::create_invite(&key, &alice_secret, &bob_public).unwrap();

        // Eve tries to accept Bob's invite
        let result = KeyDistribution::accept_invite(&invite, &eve_secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_invite_serialization() {
        let id = test_interface_id();
        let key = InterfaceKey::generate(id);

        let alice_secret = random_secret();
        let bob_secret = random_secret();
        let bob_public = PublicKey::from(&bob_secret);

        let invite = KeyDistribution::create_invite(&key, &alice_secret, &bob_public).unwrap();

        // Serialize and deserialize
        let bytes = invite.to_bytes().unwrap();
        let parsed = KeyInvite::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.interface_id, invite.interface_id);
        assert_eq!(parsed.encrypted_key, invite.encrypted_key);
        assert_eq!(parsed.nonce, invite.nonce);

        // Should still work after serialization
        let imported_key = KeyDistribution::accept_invite(&parsed, &bob_secret).unwrap();
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

        let alice_secret = random_secret();
        let bob_secret = random_secret();
        let bob_public = PublicKey::from(&bob_secret);

        let key_invite = KeyDistribution::create_invite(&key, &alice_secret, &bob_public).unwrap();
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
}
