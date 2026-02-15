//! Network bridge — connects IndrasNetwork to Dioxus signals.
//! Currently a stub; will be wired to real P2P in Phase 7.


/// Stub network handle for future P2P integration.
#[derive(Clone)]
pub struct NetworkHandle {
    // Will hold IndrasNetwork instance
}

impl NetworkHandle {
    pub fn new_stub() -> Self {
        Self {}
    }
}
