//! Deterministic mapping from (workspace_seed, object_id) to InterfaceId
//!
//! Uses BLAKE3 keyed hashing so that any peer with the same workspace seed
//! independently derives the same InterfaceId for a given object.

use dashmap::DashMap;
use indras_core::InterfaceId;

/// Context string for InterfaceId derivation
const INTERFACE_ID_CONTEXT: &str = "indras-appflowy-bridge/interface-id/v1";

/// Context string for key seed derivation
const KEY_SEED_CONTEXT: &str = "indras-appflowy-bridge/key-seed/v1";

/// Deterministic mapping from AppFlowy object IDs to IndrasNetwork InterfaceIds.
///
/// All peers sharing the same `workspace_seed` will derive identical
/// InterfaceIds for the same `object_id`, enabling them to find each other
/// on the P2P network without a central registry.
pub struct WorkspaceMapping {
    workspace_seed: [u8; 32],
    cache: DashMap<String, InterfaceId>,
}

impl WorkspaceMapping {
    /// Create a new workspace mapping with the given seed.
    ///
    /// The seed is a 32-byte secret shared among workspace members via invite.
    pub fn new(workspace_seed: [u8; 32]) -> Self {
        Self {
            workspace_seed,
            cache: DashMap::new(),
        }
    }

    /// Map an AppFlowy object_id to an InterfaceId.
    ///
    /// Results are cached for repeated lookups.
    pub fn interface_id(&self, object_id: &str) -> InterfaceId {
        if let Some(cached) = self.cache.get(object_id) {
            return *cached;
        }

        let id = object_id_to_interface_id(&self.workspace_seed, object_id);
        self.cache.insert(object_id.to_string(), id);
        id
    }

    /// Derive a 32-byte key seed for an object.
    ///
    /// Used with `InterfaceKey::from_seed()` so that all peers sharing the
    /// workspace seed independently derive the same encryption key.
    pub fn key_seed(&self, object_id: &str) -> [u8; 32] {
        object_id_to_key_seed(&self.workspace_seed, object_id)
    }

    /// Get the workspace seed.
    pub fn workspace_seed(&self) -> &[u8; 32] {
        &self.workspace_seed
    }
}

/// Derive an InterfaceId from a workspace seed and object ID.
///
/// Uses BLAKE3 keyed hash: `BLAKE3(key=workspace_seed, CONTEXT || object_id)`.
pub fn object_id_to_interface_id(workspace_seed: &[u8; 32], object_id: &str) -> InterfaceId {
    let mut hasher = blake3::Hasher::new_keyed(workspace_seed);
    hasher.update(INTERFACE_ID_CONTEXT.as_bytes());
    hasher.update(object_id.as_bytes());
    let hash = hasher.finalize();
    InterfaceId::new(*hash.as_bytes())
}

/// Derive a 32-byte key seed from a workspace seed and object ID.
///
/// Uses a different context string than `object_id_to_interface_id` to ensure
/// domain separation.
pub fn object_id_to_key_seed(workspace_seed: &[u8; 32], object_id: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(workspace_seed);
    hasher.update(KEY_SEED_CONTEXT.as_bytes());
    hasher.update(object_id.as_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_mapping() {
        let seed = [0x42u8; 32];
        let id1 = object_id_to_interface_id(&seed, "doc-abc-123");
        let id2 = object_id_to_interface_id(&seed, "doc-abc-123");
        assert_eq!(id1, id2, "same seed + object_id must produce same InterfaceId");
    }

    #[test]
    fn test_different_objects_different_ids() {
        let seed = [0x42u8; 32];
        let id1 = object_id_to_interface_id(&seed, "document-1");
        let id2 = object_id_to_interface_id(&seed, "document-2");
        assert_ne!(id1, id2, "different object_ids must produce different InterfaceIds");
    }

    #[test]
    fn test_different_seeds_different_ids() {
        let seed_a = [0x01u8; 32];
        let seed_b = [0x02u8; 32];
        let id_a = object_id_to_interface_id(&seed_a, "same-doc");
        let id_b = object_id_to_interface_id(&seed_b, "same-doc");
        assert_ne!(id_a, id_b, "different seeds must produce different InterfaceIds");
    }

    #[test]
    fn test_key_seed_domain_separation() {
        let seed = [0x42u8; 32];
        let interface_id = object_id_to_interface_id(&seed, "doc-1");
        let key_seed = object_id_to_key_seed(&seed, "doc-1");
        assert_ne!(
            interface_id.as_bytes(),
            &key_seed,
            "interface ID and key seed must differ (domain separation)"
        );
    }

    #[test]
    fn test_key_seed_deterministic() {
        let seed = [0x42u8; 32];
        let ks1 = object_id_to_key_seed(&seed, "doc-1");
        let ks2 = object_id_to_key_seed(&seed, "doc-1");
        assert_eq!(ks1, ks2);
    }

    #[test]
    fn test_workspace_mapping_cache() {
        let mapping = WorkspaceMapping::new([0x42u8; 32]);
        let id1 = mapping.interface_id("doc-1");
        let id2 = mapping.interface_id("doc-1");
        assert_eq!(id1, id2, "cached lookup must return same result");

        // Verify cache is populated
        assert!(mapping.cache.contains_key("doc-1"));
    }

    #[test]
    fn test_workspace_mapping_key_seed() {
        let mapping = WorkspaceMapping::new([0x42u8; 32]);
        let ks = mapping.key_seed("doc-1");
        let expected = object_id_to_key_seed(&[0x42u8; 32], "doc-1");
        assert_eq!(ks, expected);
    }

    #[test]
    fn test_collision_resistance() {
        // Generate many IDs and verify no collisions
        let seed = [0x42u8; 32];
        let mut ids = std::collections::HashSet::new();
        for i in 0..1000 {
            let id = object_id_to_interface_id(&seed, &format!("doc-{i}"));
            assert!(ids.insert(id), "collision at doc-{i}");
        }
    }
}
