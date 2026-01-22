//! Peer context injection for multi-instance logging
//!
//! This module provides thread-local storage for peer identity context,
//! allowing automatic injection of peer_id into all log entries within a scope.

use std::cell::RefCell;

use indras_core::PeerIdentity;
use uuid::Uuid;

/// Peer context data stored in thread-local storage
#[derive(Debug, Clone)]
pub struct PeerContextData {
    /// The peer's identity as a string
    pub peer_id: String,
    /// The type of peer (simulation or production)
    pub peer_type: PeerType,
    /// Unique instance ID for this peer session
    pub instance_id: Uuid,
}

/// Type of peer identity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerType {
    /// Simulation identity (char-based)
    Simulation,
    /// Production identity (cryptographic key)
    Production,
}

impl std::fmt::Display for PeerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerType::Simulation => write!(f, "simulation"),
            PeerType::Production => write!(f, "production"),
        }
    }
}

thread_local! {
    static PEER_CONTEXT: RefCell<Option<PeerContextData>> = const { RefCell::new(None) };
}

/// RAII guard for peer context
///
/// When this guard is created, it sets the peer context for the current thread.
/// When it's dropped, it restores the previous context (if any).
///
/// # Example
///
/// ```ignore
/// use indras_logging::context::PeerContextGuard;
/// use indras_core::SimulationIdentity;
///
/// let peer = SimulationIdentity::new('A').unwrap();
/// let _guard = PeerContextGuard::new(&peer);
///
/// // All tracing events in this scope will include peer_id = "A"
/// tracing::info!("Processing packet");
/// ```
pub struct PeerContextGuard {
    previous: Option<PeerContextData>,
}

impl PeerContextGuard {
    /// Create a new peer context guard
    ///
    /// This sets the peer identity for all log entries in the current scope.
    pub fn new<I: PeerIdentity>(identity: &I) -> Self {
        let previous = PEER_CONTEXT.with(|ctx| ctx.borrow().clone());

        let peer_type = if std::any::type_name::<I>().contains("Simulation") {
            PeerType::Simulation
        } else {
            PeerType::Production
        };

        let new_ctx = PeerContextData {
            peer_id: identity.short_id(),
            peer_type,
            instance_id: Uuid::new_v4(),
        };

        PEER_CONTEXT.with(|ctx| *ctx.borrow_mut() = Some(new_ctx));

        Self { previous }
    }

    /// Create a guard with a specific instance ID
    ///
    /// Useful when you want to maintain a consistent instance ID across restarts.
    pub fn with_instance_id<I: PeerIdentity>(identity: &I, instance_id: Uuid) -> Self {
        let previous = PEER_CONTEXT.with(|ctx| ctx.borrow().clone());

        let peer_type = if std::any::type_name::<I>().contains("Simulation") {
            PeerType::Simulation
        } else {
            PeerType::Production
        };

        let new_ctx = PeerContextData {
            peer_id: identity.short_id(),
            peer_type,
            instance_id,
        };

        PEER_CONTEXT.with(|ctx| *ctx.borrow_mut() = Some(new_ctx));

        Self { previous }
    }

    /// Get the current peer context (if any)
    pub fn current() -> Option<PeerContextData> {
        PEER_CONTEXT.with(|ctx| ctx.borrow().clone())
    }

    /// Get the current peer ID (if set)
    pub fn current_peer_id() -> Option<String> {
        Self::current().map(|ctx| ctx.peer_id)
    }

    /// Get the current instance ID (if set)
    pub fn current_instance_id() -> Option<Uuid> {
        Self::current().map(|ctx| ctx.instance_id)
    }
}

impl Drop for PeerContextGuard {
    fn drop(&mut self) {
        PEER_CONTEXT.with(|ctx| *ctx.borrow_mut() = self.previous.take());
    }
}

/// Convenience macro to create a peer context scope
///
/// # Example
///
/// ```ignore
/// with_peer_context!(peer, {
///     tracing::info!("Processing packet");
/// });
/// ```
#[macro_export]
macro_rules! with_peer_context {
    ($identity:expr, $body:block) => {{
        let _guard = $crate::context::PeerContextGuard::new($identity);
        $body
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    #[test]
    fn test_peer_context_guard() {
        // No context initially
        assert!(PeerContextGuard::current().is_none());

        let peer = SimulationIdentity::new('A').unwrap();
        {
            let _guard = PeerContextGuard::new(&peer);

            // Context should be set
            let ctx = PeerContextGuard::current().unwrap();
            assert_eq!(ctx.peer_id, "A");
            assert_eq!(ctx.peer_type, PeerType::Simulation);
        }

        // Context should be cleared after guard drops
        assert!(PeerContextGuard::current().is_none());
    }

    #[test]
    fn test_nested_contexts() {
        let peer_a = SimulationIdentity::new('A').unwrap();
        let peer_b = SimulationIdentity::new('B').unwrap();

        {
            let _guard_a = PeerContextGuard::new(&peer_a);
            assert_eq!(PeerContextGuard::current_peer_id(), Some("A".to_string()));

            {
                let _guard_b = PeerContextGuard::new(&peer_b);
                assert_eq!(PeerContextGuard::current_peer_id(), Some("B".to_string()));
            }

            // Should restore to A after B's guard drops
            assert_eq!(PeerContextGuard::current_peer_id(), Some("A".to_string()));
        }

        // Should be None after all guards drop
        assert!(PeerContextGuard::current_peer_id().is_none());
    }

    #[test]
    fn test_with_instance_id() {
        let peer = SimulationIdentity::new('X').unwrap();
        let instance_id = Uuid::new_v4();

        {
            let _guard = PeerContextGuard::with_instance_id(&peer, instance_id);

            let ctx = PeerContextGuard::current().unwrap();
            assert_eq!(ctx.instance_id, instance_id);
        }
    }
}
