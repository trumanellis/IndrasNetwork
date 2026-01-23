//! Error types for indras-storage
//!
//! This module defines the error types used throughout the storage crate.

use thiserror::Error;

/// Errors that can occur in storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    /// I/O error during storage operations
    #[error("I/O error: {0}")]
    Io(String),

    /// Requested item was not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Packet not found (for backwards compatibility)
    #[error("Packet not found: {0}")]
    PacketNotFound(String),

    /// Storage capacity has been exceeded
    #[error("Storage capacity exceeded")]
    CapacityExceeded,

    /// Error during serialization
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Error during deserialization
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Peer identity conversion error
    #[error("Identity error: {0}")]
    Identity(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::Io(err.to_string())
    }
}

impl StorageError {
    /// Create a new NotFound error
    pub fn not_found(item: impl Into<String>) -> Self {
        Self::NotFound(item.into())
    }

    /// Create a new Serialization error
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization(message.into())
    }

    /// Create a new Deserialization error
    pub fn deserialization(message: impl Into<String>) -> Self {
        Self::Deserialization(message.into())
    }

    /// Create a new Identity error
    pub fn identity(message: impl Into<String>) -> Self {
        Self::Identity(message.into())
    }

    /// Create a new I/O error
    pub fn io(message: impl Into<String>) -> Self {
        Self::Io(message.into())
    }
}

/// Convert from postcard Error to StorageError
impl From<postcard::Error> for StorageError {
    fn from(err: postcard::Error) -> Self {
        StorageError::Deserialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error() {
        let err = StorageError::not_found("test-item");
        assert!(matches!(err, StorageError::NotFound(_)));
        assert!(err.to_string().contains("test-item"));
    }

    #[test]
    fn test_capacity_exceeded_error() {
        let err = StorageError::CapacityExceeded;
        assert!(matches!(err, StorageError::CapacityExceeded));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let storage_err: StorageError = io_err.into();
        assert!(matches!(storage_err, StorageError::Io(_)));
    }

    #[test]
    fn test_serialization_error() {
        let err = StorageError::serialization("invalid format");
        assert!(matches!(err, StorageError::Serialization(_)));
    }

    #[test]
    fn test_deserialization_error() {
        let err = StorageError::deserialization("corrupted data");
        assert!(matches!(err, StorageError::Deserialization(_)));
    }
}
