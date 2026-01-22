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
