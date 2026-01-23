//! Iroh-based peer identity implementation
//!
//! Wraps iroh's `PublicKey` to implement `PeerIdentity` trait.

use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};

use iroh::PublicKey;
use serde::{Deserialize, Serialize};

use indras_core::error::IdentityError;
use indras_core::identity::PeerIdentity;

/// Peer identity based on iroh's Ed25519 public key
///
/// This wraps the 32-byte public key used by iroh for endpoint identification.
/// Implements all traits required by `PeerIdentity`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IrohIdentity(pub(crate) PublicKey);

impl IrohIdentity {
    /// Create a new identity from an iroh public key
    pub fn new(public_key: PublicKey) -> Self {
        Self(public_key)
    }

    /// Get the underlying iroh public key
    pub fn public_key(&self) -> &PublicKey {
        &self.0
    }

    /// Create from a 32-byte array
    pub fn from_array(bytes: [u8; 32]) -> Result<Self, IdentityError> {
        PublicKey::from_bytes(&bytes)
            .map(Self)
            .map_err(|e| IdentityError::InvalidFormat(e.to_string()))
    }
}

impl PeerIdentity for IrohIdentity {
    fn as_bytes(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() != 32 {
            return Err(IdentityError::InvalidKeyLength {
                expected: 32,
                actual: bytes.len(),
            });
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Self::from_array(arr)
    }

    fn short_id(&self) -> String {
        // Use first 8 characters of the base32 representation
        format!("{}", self.0.fmt_short())
    }
}

impl Debug for IrohIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IrohIdentity({})", self.0.fmt_short())
    }
}

impl Display for IrohIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.fmt_short())
    }
}

impl Hash for IrohIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state);
    }
}

impl Serialize for IrohIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize as bytes for compactness
        serializer.serialize_bytes(self.0.as_bytes())
    }
}

impl<'de> Deserialize<'de> for IrohIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        Self::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }
}

impl From<PublicKey> for IrohIdentity {
    fn from(key: PublicKey) -> Self {
        Self(key)
    }
}

impl From<IrohIdentity> for PublicKey {
    fn from(id: IrohIdentity) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iroh_identity_roundtrip() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let public = secret.public();
        let id = IrohIdentity::new(public);

        let bytes = id.as_bytes();
        let recovered = IrohIdentity::from_bytes(&bytes).unwrap();
        assert_eq!(id, recovered);
    }

    #[test]
    fn test_iroh_identity_display() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let public = secret.public();
        let id = IrohIdentity::new(public);

        let display = format!("{}", id);
        assert!(!display.is_empty());
        // Short ID should be 8 chars
        assert!(id.short_id().len() >= 6);
    }

    #[test]
    fn test_iroh_identity_serde() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let id = IrohIdentity::new(secret.public());

        // Round-trip through postcard
        let bytes = postcard::to_allocvec(&id).unwrap();
        let recovered: IrohIdentity = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(id, recovered);
    }

    #[test]
    fn test_invalid_key_length() {
        let result = IrohIdentity::from_bytes(&[0u8; 16]);
        assert!(result.is_err());
        match result {
            Err(IdentityError::InvalidKeyLength { expected, actual }) => {
                assert_eq!(expected, 32);
                assert_eq!(actual, 16);
            }
            _ => panic!("Expected InvalidKeyLength error"),
        }
    }

    #[test]
    fn test_identity_equality() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let public = secret.public();

        let id1 = IrohIdentity::new(public);
        let id2 = IrohIdentity::new(public);

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_identity_inequality() {
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());

        let id1 = IrohIdentity::new(secret1.public());
        let id2 = IrohIdentity::new(secret2.public());

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_identity_hash() {
        use std::collections::HashSet;
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());

        let id1 = IrohIdentity::new(secret1.public());
        let id2 = IrohIdentity::new(secret2.public());
        let id1_clone = IrohIdentity::new(secret1.public());

        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2);
        set.insert(id1_clone); // Should not add new entry

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_identity_bytes_comparison() {
        use iroh::SecretKey;

        let secret1 = SecretKey::generate(&mut rand::rng());
        let secret2 = SecretKey::generate(&mut rand::rng());

        let id1 = IrohIdentity::new(secret1.public());
        let id2 = IrohIdentity::new(secret2.public());

        // Different identities should have different bytes
        assert_ne!(id1.as_bytes(), id2.as_bytes());

        // Same identity should have same bytes
        let id1_clone = IrohIdentity::new(secret1.public());
        assert_eq!(id1.as_bytes(), id1_clone.as_bytes());
    }

    #[test]
    fn test_identity_empty_bytes_error() {
        let result = IrohIdentity::from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_too_many_bytes_error() {
        let result = IrohIdentity::from_bytes(&[0u8; 64]);
        assert!(result.is_err());
    }

    #[test]
    fn test_public_key_accessor() {
        use iroh::SecretKey;

        let secret = SecretKey::generate(&mut rand::rng());
        let public = secret.public();
        let id = IrohIdentity::new(public);

        assert_eq!(*id.public_key(), public);
    }
}
