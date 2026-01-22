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
//! - X25519 key exchange for secure key distribution
//! - Interface key management and sharing
//! - Key invite system for member onboarding
//!
//! ## Key Types
//!
//! - [`InterfaceKey`]: Shared symmetric key for encrypting interface events
//! - [`KeyInvite`]: Encrypted key for inviting new members
//! - [`KeyDistribution`]: Utilities for creating and accepting invites
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_crypto::{InterfaceKey, KeyDistribution};
//! use indras_core::InterfaceId;
//! use x25519_dalek::{PublicKey, StaticSecret};
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
//! // Invite a new member
//! let alice_secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
//! let bob_secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
//! let bob_public = PublicKey::from(&bob_secret);
//!
//! // Alice creates an invite for Bob
//! let invite = KeyDistribution::create_invite(&key, &alice_secret, &bob_public).unwrap();
//!
//! // Bob accepts the invite
//! let bob_key = KeyDistribution::accept_invite(&invite, &bob_secret).unwrap();
//! ```

pub mod error;
pub mod interface_key;
pub mod key_distribution;

// Re-exports
pub use error::{CryptoError, CryptoResult};
pub use interface_key::{EncryptedData, ExportedKey, InterfaceKey, KEY_SIZE, NONCE_SIZE};
pub use key_distribution::{FullInvite, InviteMetadata, KeyDistribution, KeyInvite};

// Re-export x25519 types for convenience
pub use x25519_dalek::{PublicKey, StaticSecret};
