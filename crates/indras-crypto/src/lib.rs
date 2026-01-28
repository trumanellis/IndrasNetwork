//! # Indras Crypto
//!
//! Cryptographic primitives for Indras Network.
//!
//! Provides shared symmetric key encryption for N-peer interfaces
//! and key distribution utilities for member onboarding.
//!
//! ## Features
//!
//! - ChaCha20-Poly1305 authenticated encryption for interface events
//! - ML-KEM-768 (post-quantum) key encapsulation for secure key distribution
//! - ML-DSA-65 (post-quantum) digital signatures for message authentication
//! - Interface key management and sharing
//! - Key invite system for member onboarding
//!
//! ## Key Types
//!
//! - [`InterfaceKey`]: Shared symmetric key for encrypting interface events
//! - [`KeyInvite`]: Encrypted key for inviting new members (uses ML-KEM)
//! - [`KeyDistribution`]: Utilities for creating and accepting invites
//! - [`PQIdentity`]: Post-quantum identity for signing messages
//! - [`PQKemKeyPair`]: Post-quantum key encapsulation for key exchange
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_crypto::{InterfaceKey, KeyDistribution, PQKemKeyPair};
//! use indras_core::InterfaceId;
//!
//! // Create a new interface with a random key
//! let interface_id = InterfaceId::generate();
//! let key = InterfaceKey::generate(interface_id);
//!
//! // Encrypt an event
//! let plaintext = b"Hello, world!";
//! let encrypted = key.encrypt(plaintext).unwrap();
//! let decrypted = key.decrypt(&encrypted).unwrap();
//! assert_eq!(decrypted, plaintext);
//!
//! // Invite a new member using post-quantum KEM
//! let bob_kem = PQKemKeyPair::generate();
//! let bob_public = bob_kem.encapsulation_key();
//!
//! // Alice creates an invite for Bob
//! let invite = KeyDistribution::create_invite(&key, &bob_public).unwrap();
//!
//! // Bob accepts the invite
//! let bob_key = KeyDistribution::accept_invite(&invite, &bob_kem).unwrap();
//! ```

pub mod artifact_encryption;
pub mod error;
pub mod interface_key;
pub mod key_distribution;
pub mod pq_identity;
pub mod pq_kem;

// Re-exports
pub use artifact_encryption::{
    decrypt_artifact, decrypt_artifact_bytes, encrypt_artifact, generate_artifact_key,
    hash_content, ArtifactKey, EncryptedArtifact, EncryptedArtifactKey, ARTIFACT_KEY_SIZE,
};
pub use error::{CryptoError, CryptoResult};
#[allow(deprecated)]
pub use interface_key::ExportedKey;
pub use interface_key::{EncapsulatedKey, EncryptedData, InterfaceKey, KEY_SIZE, NONCE_SIZE};
pub use key_distribution::{FullInvite, InviteMetadata, KeyDistribution, KeyInvite};

// Post-quantum re-exports
pub use pq_identity::{
    PQ_SIGNATURE_SIZE, PQ_SIGNING_KEY_SIZE, PQ_VERIFYING_KEY_SIZE, PQIdentity, PQPublicIdentity,
    PQSignature, SecureBytes,
};
pub use pq_kem::{
    PQ_CIPHERTEXT_SIZE, PQ_DECAPSULATION_KEY_SIZE, PQ_ENCAPSULATION_KEY_SIZE,
    PQ_SHARED_SECRET_SIZE, PQCiphertext, PQEncapsulationKey, PQKemKeyPair,
};

// Re-export x25519 types for legacy code (deprecated)
#[deprecated(since = "0.2.0", note = "Use ML-KEM via PQKemKeyPair instead")]
pub use x25519_dalek::{PublicKey, StaticSecret};
