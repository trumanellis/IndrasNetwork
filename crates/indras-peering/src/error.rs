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
    /// The contacts realm has not been joined yet.
    #[error("contacts realm not joined — call start_tasks first")]
    ContactsRealmNotJoined,
    /// No remote peer found in the given realm.
    #[error("no remote peer found in realm")]
    NoPeerInRealm,
    /// Catch-all for miscellaneous errors.
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        assert_eq!(
            PeeringError::AlreadyShutDown.to_string(),
            "peering runtime already shut down"
        );
        assert_eq!(
            PeeringError::ContactsRealmNotJoined.to_string(),
            "contacts realm not joined — call start_tasks first"
        );
        assert_eq!(
            PeeringError::NoPeerInRealm.to_string(),
            "no remote peer found in realm"
        );
        assert_eq!(
            PeeringError::Other("custom".into()).to_string(),
            "custom"
        );
    }

    #[test]
    fn error_is_debug() {
        // Ensure Debug is derived (compile-time check + runtime format)
        let err = PeeringError::AlreadyShutDown;
        let dbg = format!("{:?}", err);
        assert!(dbg.contains("AlreadyShutDown"));
    }
}
