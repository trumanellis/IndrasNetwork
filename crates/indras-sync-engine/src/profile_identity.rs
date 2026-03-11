//! CRDT document for user-set identity fields that sync across devices.
//!
//! Uses last-writer-wins merge strategy based on `updated_at` timestamp.
//! Accessed via `home.document::<ProfileIdentityDocument>("_profile_identity")`.
//! Prefixed with `_` to indicate it's a system document.

use serde::{Deserialize, Serialize};

/// CRDT document for user-set identity fields that sync across devices.
///
/// Uses last-writer-wins merge strategy based on `updated_at` timestamp.
/// Accessed via `home.document::<ProfileIdentityDocument>("_profile_identity")`.
/// Prefixed with `_` to indicate it's a system document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileIdentityDocument {
    /// User's chosen display name.
    pub display_name: String,
    /// User's chosen username/handle.
    pub username: String,
    /// Optional biographical text.
    pub bio: Option<String>,
    /// Hex-encoded public key.
    pub public_key: String,
    /// Timestamp of last update (epoch seconds) for last-writer-wins merge.
    pub updated_at: i64,
}

impl indras_network::document::DocumentSchema for ProfileIdentityDocument {
    fn merge(&mut self, remote: Self) {
        // Last-writer-wins: keep whichever version has the higher `updated_at`.
        if remote.updated_at > self.updated_at {
            *self = remote;
        }
    }
}
