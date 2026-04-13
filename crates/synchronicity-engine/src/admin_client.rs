//! In-process admin client for the embedded relay service.
//!
//! Holds a direct `Arc<RelayService>` and invokes its config methods
//! synchronously-ish (async but no HTTP). The process boundary is the
//! trust boundary, so there's no auth.

use std::sync::Arc;

use indras_relay::{RelayConfigPatch, RelayConfigView, RelayService};

/// Thin wrapper exposing read/write access to the embedded relay config.
#[derive(Clone)]
pub struct AdminClient {
    service: Arc<RelayService>,
}

impl AdminClient {
    /// Wrap an existing `RelayService` handle.
    pub fn new(service: Arc<RelayService>) -> Self {
        Self { service }
    }

    /// Snapshot the current config.
    pub async fn get_config(&self) -> Result<RelayConfigView, String> {
        Ok(self.service.config_view().await)
    }

    /// Apply a patch; returns the updated view or a validation/persist error.
    pub async fn put_config(&self, patch: RelayConfigPatch) -> Result<RelayConfigView, String> {
        self.service.apply_config_patch(patch).await
    }
}
