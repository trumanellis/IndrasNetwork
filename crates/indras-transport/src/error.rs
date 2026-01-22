//! Transport layer error types

pub use crate::connection::ConnectionError;
pub use crate::discovery::DiscoveryError;
pub use crate::protocol::FramingError;

use thiserror::Error;

/// Unified transport error type
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection error: {0}")]
    Connection(#[from] ConnectionError),

    #[error("Discovery error: {0}")]
    Discovery(#[from] DiscoveryError),

    #[error("Protocol framing error: {0}")]
    Framing(#[from] FramingError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Stream closed")]
    StreamClosed,

    #[error("Invalid peer address")]
    InvalidAddress,
}
