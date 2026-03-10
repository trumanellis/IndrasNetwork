//! # Indras Homepage
//!
//! HTTP homepage server for indras nodes.
//!
//! Each node can optionally serve a static profile page over HTTP,
//! accessible at the node's configured port. The page displays the
//! node's display name, username, bio, and public key.
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_homepage::{HomepageServer, Profile};
//!
//! let profile = Profile::new("Alice", "alice", "abcdef1234567890");
//! let server = HomepageServer::new(profile);
//! server.serve("127.0.0.1:3000".parse().unwrap()).await?;
//! ```

pub mod grants;
pub mod profile;
pub mod server;
pub mod templates;

pub use indras_profile::{IntentionSummary, Profile, ViewLevel, Visibility, Visible};

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::RwLock;
use indras_artifacts::AccessGrant;

/// Errors from the homepage server
#[derive(Debug, thiserror::Error)]
pub enum HomepageError {
    /// Failed to bind to address
    #[error("failed to bind: {0}")]
    Bind(String),
    /// Server error
    #[error("server error: {0}")]
    Serve(String),
}

/// HTTP homepage server for an indras node
pub struct HomepageServer {
    /// Shared profile data (can be updated live)
    profile: Arc<RwLock<Profile>>,
    /// Shared grant list for the profile artifact (updated by polling loop).
    grants: Arc<RwLock<Vec<AccessGrant>>>,
    /// The steward's member ID.
    steward: [u8; 32],
}

impl HomepageServer {
    /// Create a new homepage server with profile data
    pub fn new(profile: Profile, steward: [u8; 32]) -> Self {
        Self {
            profile: Arc::new(RwLock::new(profile)),
            grants: Arc::new(RwLock::new(Vec::new())),
            steward,
        }
    }

    /// Start serving on the given address
    pub async fn serve(self, addr: SocketAddr) -> Result<(), HomepageError> {
        server::serve(addr, self.profile, self.grants, self.steward).await
    }

    /// Update profile data (live reload — takes effect on next request)
    pub async fn update_profile(&self, profile: Profile) {
        *self.profile.write().await = profile;
    }

    /// Get a clone of the shared profile handle for external updates
    pub fn profile_handle(&self) -> Arc<RwLock<Profile>> {
        self.profile.clone()
    }

    /// Get a clone of the shared grants handle for external updates.
    pub fn grants_handle(&self) -> Arc<RwLock<Vec<AccessGrant>>> {
        self.grants.clone()
    }

    /// Get the steward's member ID.
    pub fn steward(&self) -> &[u8; 32] {
        &self.steward
    }
}
