//! Error types for indras-crypto

use thiserror::Error;

/// Errors that can occur during cryptographic operations
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Key generation failed: {0}")]
    KeyGenerationFailed(String),

    #[error("Key exchange failed: {0}")]
    KeyExchangeFailed(String),

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Invalid nonce")]
    InvalidNonce,

    #[error("Data too short: expected at least {expected} bytes, got {actual}")]
    DataTooShort { expected: usize, actual: usize },
}

/// Result type for crypto operations
pub type CryptoResult<T> = Result<T, CryptoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_error_display() {
        let err = CryptoError::EncryptionFailed("bad key".to_string());
        assert!(format!("{}", err).contains("Encryption failed"));
        assert!(format!("{}", err).contains("bad key"));

        let err = CryptoError::DecryptionFailed("corrupt ciphertext".to_string());
        assert!(format!("{}", err).contains("Decryption failed"));

        let err = CryptoError::InvalidKey("wrong length".to_string());
        assert!(format!("{}", err).contains("Invalid key"));

        let err = CryptoError::KeyGenerationFailed("rng error".to_string());
        assert!(format!("{}", err).contains("Key generation failed"));

        let err = CryptoError::KeyExchangeFailed("peer rejected".to_string());
        assert!(format!("{}", err).contains("Key exchange failed"));

        let err = CryptoError::SignatureVerificationFailed;
        assert!(format!("{}", err).contains("Signature verification failed"));

        let err = CryptoError::InvalidNonce;
        assert!(format!("{}", err).contains("Invalid nonce"));

        let err = CryptoError::DataTooShort {
            expected: 32,
            actual: 16,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Data too short"));
        assert!(msg.contains("32"));
        assert!(msg.contains("16"));
    }

    #[test]
    fn test_crypto_error_debug() {
        let err = CryptoError::InvalidNonce;
        let debug_str = format!("{:?}", err);
        assert!(!debug_str.is_empty());
    }
}
