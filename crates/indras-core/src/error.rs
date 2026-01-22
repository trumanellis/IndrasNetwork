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
