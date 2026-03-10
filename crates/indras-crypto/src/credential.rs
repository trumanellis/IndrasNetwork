//! Relay credential types and operations.
//!
//! Provides shared credential creation, parsing, and verification so both
//! clients and relays use the same code. The credential format (v1) is a
//! postcard-serialized `CredentialV1` followed by a 64-byte Ed25519 signature.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::{CryptoError, CryptoResult};

/// A credential payload (v1 format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialV1 {
    /// The player's identity (32-byte Ed25519 public key)
    pub player_id: [u8; 32],
    /// The transport public key this credential authorizes
    pub transport_pubkey: [u8; 32],
    /// When this credential expires (Unix millis)
    pub expires_at_millis: i64,
}

/// Parsed credential with its signature.
#[derive(Debug, Clone)]
pub struct SignedCredential {
    /// The credential payload
    pub credential: CredentialV1,
    /// Ed25519 signature over the serialized credential
    pub signature: [u8; 64],
}

/// Create a signed credential blob.
///
/// Returns postcard-serialized `CredentialV1` concatenated with a 64-byte
/// Ed25519 signature. The `signing_key`'s public key becomes `player_id`.
pub fn create_credential(
    signing_key: &SigningKey,
    transport_pubkey: [u8; 32],
    expires_at_millis: i64,
) -> Vec<u8> {
    let credential = CredentialV1 {
        player_id: signing_key.verifying_key().to_bytes(),
        transport_pubkey,
        expires_at_millis,
    };

    let payload = postcard::to_allocvec(&credential).expect("credential serialization");
    let signature: Signature = signing_key.sign(&payload);

    let mut blob = payload;
    blob.extend_from_slice(&signature.to_bytes());
    blob
}

/// Parse a credential blob into a `SignedCredential`.
///
/// Format: postcard-serialized `CredentialV1` ++ 64-byte Ed25519 signature.
pub fn parse_credential(bytes: &[u8]) -> CryptoResult<SignedCredential> {
    if bytes.len() < 64 {
        return Err(CryptoError::DataTooShort {
            expected: 65, // at least 1 byte payload + 64 signature
            actual: bytes.len(),
        });
    }

    let sig_offset = bytes.len() - 64;
    let credential_bytes = &bytes[..sig_offset];
    let sig_bytes = &bytes[sig_offset..];

    let credential: CredentialV1 = postcard::from_bytes(credential_bytes).map_err(|e| {
        CryptoError::InvalidKey(format!("failed to parse credential: {e}"))
    })?;

    let mut signature = [0u8; 64];
    signature.copy_from_slice(sig_bytes);

    Ok(SignedCredential {
        credential,
        signature,
    })
}

/// Verify the Ed25519 signature on a credential.
///
/// The signing key is the `player_id` itself (their Ed25519 public key).
pub fn verify_credential(signed: &SignedCredential) -> CryptoResult<()> {
    let verifying_key = VerifyingKey::from_bytes(&signed.credential.player_id)
        .map_err(|e| CryptoError::InvalidKey(format!("invalid player_id key: {e}")))?;

    let payload = postcard::to_allocvec(&signed.credential).map_err(|e| {
        CryptoError::InvalidKey(format!("failed to re-serialize credential: {e}"))
    })?;

    let signature = Signature::from_bytes(&signed.signature);

    verifying_key.verify(&payload, &signature).map_err(|_| {
        CryptoError::SignatureVerificationFailed
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    #[test]
    fn test_credential_roundtrip() {
        let signing_key = random_signing_key();
        let transport_pubkey = [0xABu8; 32];
        let expires = 1_700_000_000_000i64;

        let blob = create_credential(&signing_key, transport_pubkey, expires);
        let signed = parse_credential(&blob).unwrap();

        assert_eq!(signed.credential.player_id, signing_key.verifying_key().to_bytes());
        assert_eq!(signed.credential.transport_pubkey, transport_pubkey);
        assert_eq!(signed.credential.expires_at_millis, expires);

        verify_credential(&signed).unwrap();
    }

    #[test]
    fn test_credential_too_short() {
        let result = parse_credential(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_credential_bad_signature() {
        let signing_key = random_signing_key();
        let mut blob = create_credential(&signing_key, [1u8; 32], 1_700_000_000_000);
        let len = blob.len();
        blob[len - 1] ^= 0xFF; // corrupt signature

        let signed = parse_credential(&blob).unwrap();
        assert!(verify_credential(&signed).is_err());
    }
}
