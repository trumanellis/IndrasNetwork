//! Extension trait adding vault sync methods to Realm.
//!
//! With the fork-rights architecture, the vault file index is local-only
//! (not a shared CRDT). This trait retains the `vault_index()` method
//! only for backward compatibility with `snapshot_patch` in `RealmBraid`;
//! it will be removed once `snapshot_patch` reads from a local index.

use indras_network::Realm;

/// Vault file sync extension trait for Realm.
///
/// Most methods have been removed — the vault file index is now local-only.
/// See `Vault` for the primary API.
#[allow(async_fn_in_trait)]
pub trait RealmVault {
    // Intentionally empty — vault file operations are now on `Vault` directly.
    // This trait is retained for import compatibility and will be removed.
}

impl RealmVault for Realm {}
