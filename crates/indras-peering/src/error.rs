//! Peering error types.

/// Errors from the peering runtime.
#[derive(Debug, thiserror::Error)]
pub enum PeeringError {
    /// Propagated from the underlying IndrasNetwork.
    #[error(transparent)]
    Network(#[from] indras_network::IndraError),
    /// The runtime has already been shut down.
    #[error("peering runtime already shut down")]
    AlreadyShutDown,
    /// Catch-all for miscellaneous errors.
    #[error("{0}")]
    Other(String),
}
