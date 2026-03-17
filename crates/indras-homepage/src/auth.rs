//! Bearer-token authentication for homepage viewers.
//!
//! A viewer proves their identity by signing `(steward_pubkey || timestamp)`
//! with their Ed25519 secret key and presenting it as a bearer token.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};

/// Maximum age of a token in seconds (5 minutes).
const TOKEN_MAX_AGE_SECS: i64 = 300;

/// Size of the timestamp field in bytes.
const TIMESTAMP_LEN: usize = 8;

/// Total raw token length: pubkey(32) + timestamp(8) + signature(64) = 104.
const TOKEN_RAW_LEN: usize = PUBLIC_KEY_LENGTH + TIMESTAMP_LEN + SIGNATURE_LENGTH;

/// A signed viewer identity token.
///
/// Format: `base64(pubkey[32] || timestamp_be[8] || signature[64])`
/// The signature covers: `steward_pubkey[32] || timestamp_be[8]`
#[derive(Debug, Clone)]
pub struct ViewerToken {
    /// The viewer's Ed25519 public key (32 bytes = MemberId).
    pub pubkey: [u8; 32],
    /// Unix timestamp (seconds) when the token was created.
    pub timestamp: i64,
    /// Ed25519 signature over `steward || timestamp`.
    pub signature: [u8; 64],
}

/// Errors from token operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Token is not valid base64.
    #[error("invalid base64 encoding")]
    InvalidBase64,
    /// Token has wrong length.
    #[error("invalid token length: expected {TOKEN_RAW_LEN}, got {0}")]
    InvalidLength(usize),
    /// Signature verification failed.
    #[error("signature verification failed")]
    BadSignature,
    /// Token timestamp is too old or too far in the future.
    #[error("token expired or too far in future")]
    Expired,
    /// Could not parse the public key.
    #[error("invalid public key")]
    InvalidKey,
}

impl ViewerToken {
    /// Create a new token (used by the viewer/client side).
    pub fn new(pubkey: [u8; 32], timestamp: i64, signature: [u8; 64]) -> Self {
        Self { pubkey, timestamp, signature }
    }

    /// Encode the token as a base64 string.
    pub fn encode(&self) -> String {
        let mut raw = [0u8; TOKEN_RAW_LEN];
        raw[..32].copy_from_slice(&self.pubkey);
        raw[32..40].copy_from_slice(&self.timestamp.to_be_bytes());
        raw[40..104].copy_from_slice(&self.signature);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
    }

    /// Decode a token from a base64 string.
    pub fn decode(s: &str) -> Result<Self, AuthError> {
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(s)
            .map_err(|_| AuthError::InvalidBase64)?;
        if raw.len() != TOKEN_RAW_LEN {
            return Err(AuthError::InvalidLength(raw.len()));
        }
        let mut pubkey = [0u8; 32];
        pubkey.copy_from_slice(&raw[..32]);
        let timestamp = i64::from_be_bytes(raw[32..40].try_into().unwrap());
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&raw[40..104]);
        Ok(Self { pubkey, timestamp, signature })
    }

    /// Verify the token against the steward's public key and current time.
    ///
    /// Returns the viewer's MemberId (pubkey bytes) on success.
    pub fn verify(&self, steward: &[u8; 32], now: i64) -> Result<[u8; 32], AuthError> {
        // Check timestamp freshness
        let age = (now - self.timestamp).abs();
        if age > TOKEN_MAX_AGE_SECS {
            return Err(AuthError::Expired);
        }

        // Build the signed message: steward[32] || timestamp[8]
        let mut msg = [0u8; 40];
        msg[..32].copy_from_slice(steward);
        msg[32..40].copy_from_slice(&self.timestamp.to_be_bytes());

        // Verify signature
        let verifying_key = VerifyingKey::from_bytes(&self.pubkey)
            .map_err(|_| AuthError::InvalidKey)?;
        let signature = Signature::from_bytes(&self.signature);
        verifying_key.verify_strict(&msg, &signature)
            .map_err(|_| AuthError::BadSignature)?;

        Ok(self.pubkey)
    }
}

/// Axum extractor that optionally extracts a verified viewer identity.
///
/// Reads the `Authorization: Bearer <token>` header. If present and valid,
/// returns `Some(member_id)`. If absent, returns `None`. If present but
/// invalid, returns a 401 error.
pub struct OptionalViewer(pub Option<[u8; 32]>);

/// Axum extractor requiring the viewer to be the steward (page owner).
///
/// Returns 401 if no token is present, 403 if the viewer is not the
/// steward. On success yields the steward's public key bytes.
pub struct RequiredSteward(pub [u8; 32]);

/// Returns the current Unix timestamp in seconds.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl<S> FromRequestParts<S> for OptionalViewer
where
    S: Send + Sync + AsRef<[u8; 32]>,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get("authorization");
        let Some(header_value) = header else {
            return Ok(OptionalViewer(None));
        };

        let header_str = header_value.to_str()
            .map_err(|_| (StatusCode::BAD_REQUEST, "invalid authorization header"))?;

        let token_str = header_str.strip_prefix("Bearer ")
            .ok_or((StatusCode::BAD_REQUEST, "expected Bearer token"))?;

        let token = ViewerToken::decode(token_str)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token"))?;

        let steward = state.as_ref();
        let viewer_id = token.verify(steward, unix_now())
            .map_err(|_| (StatusCode::UNAUTHORIZED, "token verification failed"))?;

        Ok(OptionalViewer(Some(viewer_id)))
    }
}

impl<S> FromRequestParts<S> for RequiredSteward
where
    S: Send + Sync + AsRef<[u8; 32]>,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let viewer = OptionalViewer::from_request_parts(parts, state).await?;
        let steward = state.as_ref();
        match viewer.0 {
            None => Err((StatusCode::UNAUTHORIZED, "authentication required")),
            Some(id) if id == *steward => Ok(RequiredSteward(id)),
            Some(_) => Err((StatusCode::FORBIDDEN, "steward access required")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn make_token(steward: &[u8; 32], timestamp: i64) -> (ViewerToken, SigningKey) {
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        let pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();

        let mut msg = [0u8; 40];
        msg[..32].copy_from_slice(steward);
        msg[32..40].copy_from_slice(&timestamp.to_be_bytes());

        let sig = signing_key.sign(&msg);
        let token = ViewerToken::new(pubkey, timestamp, sig.to_bytes());
        (token, signing_key)
    }

    #[test]
    fn encode_decode_roundtrip() {
        let steward = [0x01; 32];
        let (token, _) = make_token(&steward, 1000);
        let encoded = token.encode();
        let decoded = ViewerToken::decode(&encoded).unwrap();
        assert_eq!(decoded.pubkey, token.pubkey);
        assert_eq!(decoded.timestamp, token.timestamp);
        assert_eq!(decoded.signature, token.signature);
    }

    #[test]
    fn verify_valid_token() {
        let steward = [0x01; 32];
        let now = 1000i64;
        let (token, _) = make_token(&steward, now);
        let result = token.verify(&steward, now);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), token.pubkey);
    }

    #[test]
    fn reject_expired_token() {
        let steward = [0x01; 32];
        let (token, _) = make_token(&steward, 1000);
        // now is 6 minutes later
        let result = token.verify(&steward, 1000 + 360);
        assert!(matches!(result, Err(AuthError::Expired)));
    }

    #[test]
    fn reject_wrong_steward() {
        let steward = [0x01; 32];
        let wrong_steward = [0x02; 32];
        let (token, _) = make_token(&steward, 1000);
        let result = token.verify(&wrong_steward, 1000);
        assert!(matches!(result, Err(AuthError::BadSignature)));
    }

    #[test]
    fn reject_invalid_base64() {
        let result = ViewerToken::decode("not-valid-base64!!!");
        assert!(matches!(result, Err(AuthError::InvalidBase64)));
    }

    #[test]
    fn reject_wrong_length() {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 50]);
        let result = ViewerToken::decode(&encoded);
        assert!(matches!(result, Err(AuthError::InvalidLength(50))));
    }
}
