//! Routing error types
//!
//! Re-exports core routing errors and adds routing-specific errors.

use thiserror::Error;

// Re-export core routing errors
pub use indras_core::RoutingError as CoreRoutingError;

/// Extended routing errors for the routing crate
#[derive(Debug, Error)]
pub enum RoutingError {
    /// Core routing error
    #[error("Core routing error: {0}")]
    Core(#[from] CoreRoutingError),

    /// Storage operation failed
    #[error("Storage operation failed")]
    StorageFailed,

    /// Back-propagation timeout
    #[error("Back-propagation timed out for packet")]
    BackPropTimeout,

    /// Invalid route state
    #[error("Invalid route state: {0}")]
    InvalidRouteState(String),

    /// Topology error
    #[error("Topology error: {0}")]
    TopologyError(String),

    /// Route cache miss
    #[error("Route not found in cache")]
    RouteCacheMiss,

    /// Route is stale
    #[error("Route is stale and needs refresh")]
    RouteStale,

    /// No relay candidates available
    #[error("No relay candidates available")]
    NoRelayCandidates,
}

impl From<indras_core::StorageError> for RoutingError {
    fn from(_: indras_core::StorageError) -> Self {
        Self::StorageFailed
    }
}

/// Result type for routing operations
pub type RoutingResult<T> = Result<T, RoutingError>;
