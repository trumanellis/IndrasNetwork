//! Peer identity abstractions
//!
//! This module provides the [`PeerIdentity`] trait that abstracts over
//! different identity implementations:
//!
//! - `SimulationIdentity`: Simple char-based identity for testing ('A'..'Z')
//! - `IrohIdentity`: Real public key identity using iroh (in indras-transport)

use std::fmt::{Debug, Display};
use std::hash::Hash;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::error::IdentityError;

/// Trait for peer identity abstraction
///
/// This trait allows the same routing and messaging logic to work with
/// both simulation identities (simple chars) and real cryptographic
/// identities (public keys).
pub trait PeerIdentity:
    Clone + Eq + Hash + Send + Sync + Debug + Display + Serialize + DeserializeOwned + 'static
{
    /// Get the identity as bytes
    fn as_bytes(&self) -> Vec<u8>;

    /// Create an identity from bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, IdentityError>;

    /// Get a short display form (for logging)
    fn short_id(&self) -> String {
        format!("{}", self)
    }
}

/// Simple character-based identity for simulation
///
/// Used for testing and development. Maps to characters 'A'..'Z'.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SimulationIdentity(pub char);

impl SimulationIdentity {
    /// Create a new simulation identity from a capital letter
    pub fn new(c: char) -> Option<Self> {
        if c.is_ascii_uppercase() {
            Some(Self(c))
        } else {
            None
        }
    }

    /// Generate all identities from 'A' to the given letter (inclusive)
    pub fn range_to(end: char) -> Vec<Self> {
        ('A'..=end).filter_map(Self::new).collect()
    }

    /// Get the underlying character
    pub fn as_char(&self) -> char {
        self.0
    }
}

impl Display for SimulationIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PeerIdentity for SimulationIdentity {
    fn as_bytes(&self) -> Vec<u8> {
        vec![self.0 as u8]
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() != 1 {
            return Err(IdentityError::InvalidKeyLength {
                expected: 1,
                actual: bytes.len(),
            });
        }
        let c = bytes[0] as char;
        Self::new(c).ok_or_else(|| {
            IdentityError::InvalidFormat(format!("Invalid simulation identity: {}", c))
        })
    }

    fn short_id(&self) -> String {
        self.0.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_identity_creation() {
        assert!(SimulationIdentity::new('A').is_some());
        assert!(SimulationIdentity::new('Z').is_some());
        assert!(SimulationIdentity::new('a').is_none());
        assert!(SimulationIdentity::new('1').is_none());
    }

    #[test]
    fn test_simulation_identity_range() {
        let ids = SimulationIdentity::range_to('C');
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0].0, 'A');
        assert_eq!(ids[1].0, 'B');
        assert_eq!(ids[2].0, 'C');
    }

    #[test]
    fn test_simulation_identity_bytes_roundtrip() {
        let id = SimulationIdentity::new('M').unwrap();
        let bytes = id.as_bytes();
        let recovered = SimulationIdentity::from_bytes(&bytes).unwrap();
        assert_eq!(id, recovered);
    }
}
