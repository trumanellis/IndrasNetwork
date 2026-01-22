//! Error types for Indras Network

use thiserror::Error;

/// Top-level error type for Indras Network
#[derive(Debug, Error)]
pub enum IndrasError {
    #[error("Identity error: {0}")]
    Identity(#[from] IdentityError),

    #[error("Routing error: {0}")]
    Routing(#[from] RoutingError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Interface error: {0}")]
    Interface(#[from] InterfaceError),
}

/// Errors related to peer identity
#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("Invalid identity format: {0}")]
    InvalidFormat(String),

    #[error("Identity not found: {0}")]
    NotFound(String),

    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },
}

/// Errors related to routing
#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("No route available to destination")]
    NoRoute,

    #[error("TTL expired for packet")]
    TtlExpired,

    #[error("Packet already visited this peer")]
    AlreadyVisited,

    #[error("Destination peer not found")]
    DestinationNotFound,

    #[error("Routing timeout after {0} ticks")]
    Timeout(u64),
}

/// Errors related to storage
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Storage I/O error: {0}")]
    Io(String),

    #[error("Packet not found: {0}")]
    PacketNotFound(String),

    #[error("Storage capacity exceeded")]
    CapacityExceeded,

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),
}

/// Errors related to cryptography
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

    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}

/// Errors related to transport
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Peer not connected: {0}")]
    PeerNotConnected(String),

    #[error("Address resolution failed: {0}")]
    AddressResolutionFailed(String),
}

/// Errors related to protocol handling
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Invalid message format: {0}")]
    InvalidMessageFormat(String),

    #[error("Unknown message type: {0}")]
    UnknownMessageType(u8),

    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },

    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),
}

/// Errors related to N-peer interfaces
#[derive(Debug, Error)]
pub enum InterfaceError {
    #[error("Not a member of this interface")]
    NotMember,

    #[error("Interface not found: {0}")]
    NotFound(String),

    #[error("Event append failed: {0}")]
    AppendFailed(String),

    #[error("Sync failed: {0}")]
    SyncFailed(String),

    #[error("Invalid event: {0}")]
    InvalidEvent(String),

    #[error("Member already exists")]
    MemberExists,

    #[error("Member not found")]
    MemberNotFound,

    #[error("Document error: {0}")]
    DocumentError(String),

    #[error("Encryption required but no key available")]
    NoKey,
}

/// Result type alias for Indras operations
pub type IndrasResult<T> = Result<T, IndrasError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_error_display() {
        let err = IdentityError::InvalidFormat("bad data".to_string());
        assert!(format!("{}", err).contains("Invalid identity format"));
        assert!(format!("{}", err).contains("bad data"));

        let err = IdentityError::NotFound("peer123".to_string());
        assert!(format!("{}", err).contains("Identity not found"));
        assert!(format!("{}", err).contains("peer123"));

        let err = IdentityError::InvalidKeyLength {
            expected: 32,
            actual: 16,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("32"));
        assert!(msg.contains("16"));
    }

    #[test]
    fn test_routing_error_display() {
        assert!(format!("{}", RoutingError::NoRoute).contains("No route"));
        assert!(format!("{}", RoutingError::TtlExpired).contains("TTL expired"));
        assert!(format!("{}", RoutingError::AlreadyVisited).contains("already visited"));
        assert!(format!("{}", RoutingError::DestinationNotFound).contains("not found"));

        let err = RoutingError::Timeout(100);
        assert!(format!("{}", err).contains("100"));
    }

    #[test]
    fn test_storage_error_display() {
        let err = StorageError::Io("disk full".to_string());
        assert!(format!("{}", err).contains("disk full"));

        let err = StorageError::PacketNotFound("abc123".to_string());
        assert!(format!("{}", err).contains("abc123"));

        assert!(
            format!("{}", StorageError::CapacityExceeded).contains("exceeded")
        );

        let err = StorageError::Serialization("json error".to_string());
        assert!(format!("{}", err).contains("json error"));
    }

    #[test]
    fn test_crypto_error_display() {
        let err = CryptoError::EncryptionFailed("bad key".to_string());
        assert!(format!("{}", err).contains("Encryption failed"));

        let err = CryptoError::DecryptionFailed("corrupt data".to_string());
        assert!(format!("{}", err).contains("Decryption failed"));

        assert!(
            format!("{}", CryptoError::SignatureVerificationFailed)
                .contains("Signature verification")
        );
    }

    #[test]
    fn test_transport_error_display() {
        let err = TransportError::ConnectionFailed("timeout".to_string());
        assert!(format!("{}", err).contains("Connection failed"));

        assert!(format!("{}", TransportError::ConnectionClosed).contains("closed"));

        let err = TransportError::PeerNotConnected("peer_a".to_string());
        assert!(format!("{}", err).contains("peer_a"));
    }

    #[test]
    fn test_protocol_error_display() {
        let err = ProtocolError::InvalidMessageFormat("missing field".to_string());
        assert!(format!("{}", err).contains("Invalid message format"));

        let err = ProtocolError::UnknownMessageType(42);
        assert!(format!("{}", err).contains("42"));

        let err = ProtocolError::VersionMismatch {
            expected: 2,
            actual: 1,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("2"));
        assert!(msg.contains("1"));
    }

    #[test]
    fn test_interface_error_display() {
        assert!(format!("{}", InterfaceError::NotMember).contains("Not a member"));
        assert!(format!("{}", InterfaceError::MemberExists).contains("already exists"));
        assert!(format!("{}", InterfaceError::MemberNotFound).contains("not found"));
        assert!(format!("{}", InterfaceError::NoKey).contains("no key"));

        let err = InterfaceError::NotFound("iface123".to_string());
        assert!(format!("{}", err).contains("iface123"));
    }

    #[test]
    fn test_error_conversions() {
        // Test that sub-errors convert to IndrasError
        let identity_err = IdentityError::NotFound("peer".to_string());
        let indras_err: IndrasError = identity_err.into();
        assert!(matches!(indras_err, IndrasError::Identity(_)));

        let routing_err = RoutingError::NoRoute;
        let indras_err: IndrasError = routing_err.into();
        assert!(matches!(indras_err, IndrasError::Routing(_)));

        let storage_err = StorageError::CapacityExceeded;
        let indras_err: IndrasError = storage_err.into();
        assert!(matches!(indras_err, IndrasError::Storage(_)));

        let crypto_err = CryptoError::SignatureVerificationFailed;
        let indras_err: IndrasError = crypto_err.into();
        assert!(matches!(indras_err, IndrasError::Crypto(_)));

        let transport_err = TransportError::ConnectionClosed;
        let indras_err: IndrasError = transport_err.into();
        assert!(matches!(indras_err, IndrasError::Transport(_)));

        let protocol_err = ProtocolError::UnknownMessageType(1);
        let indras_err: IndrasError = protocol_err.into();
        assert!(matches!(indras_err, IndrasError::Protocol(_)));

        let interface_err = InterfaceError::NotMember;
        let indras_err: IndrasError = interface_err.into();
        assert!(matches!(indras_err, IndrasError::Interface(_)));
    }

    #[test]
    fn test_indras_error_display() {
        let err: IndrasError = IdentityError::NotFound("test".to_string()).into();
        let msg = format!("{}", err);
        assert!(msg.contains("Identity error"));
        assert!(msg.contains("test"));
    }
}
