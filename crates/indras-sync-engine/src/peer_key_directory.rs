//! Peer key directory — CRDT document mapping `UserId` to PQ verifying keys.
//!
//! Each peer publishes its ML-DSA-65 verifying key when it first joins a
//! vault. Other peers use this directory to verify changeset signatures.
//!
//! Merge is insert-only per `UserId`: once a key is published, it cannot
//! be changed (the `UserId` is derived from the key via `blake3(vk_bytes)`,
//! so a different key would produce a different `UserId`).

use crate::vault::vault_file::UserId;
use indras_crypto::PQPublicIdentity;
use indras_network::document::DocumentSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// CRDT document: maps `UserId` to raw PQ verifying key bytes.
///
/// Merge is set-union by `UserId`. Keys are immutable once published
/// because `UserId = blake3(verifying_key_bytes)` — a different key
/// would hash to a different `UserId`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerKeyDirectory {
    /// UserId → ML-DSA-65 verifying key bytes (1,952 bytes each).
    pub keys: BTreeMap<UserId, Vec<u8>>,
}

impl PeerKeyDirectory {
    /// Publish a peer's verifying key.
    ///
    /// Validates that `blake3(vk_bytes) == user_id` before inserting.
    /// Returns `true` if the key was newly inserted, `false` if already present.
    pub fn publish(&mut self, user_id: UserId, vk_bytes: Vec<u8>) -> bool {
        // Verify the binding: UserId must be blake3 of the verifying key.
        let expected = *blake3::hash(&vk_bytes).as_bytes();
        if expected != user_id {
            return false;
        }
        if self.keys.contains_key(&user_id) {
            return false;
        }
        self.keys.insert(user_id, vk_bytes);
        true
    }

    /// Look up a peer's verifying key by `UserId`.
    ///
    /// Returns a `PQPublicIdentity` if the key is present and valid.
    pub fn get(&self, user_id: &UserId) -> Option<PQPublicIdentity> {
        self.keys
            .get(user_id)
            .and_then(|bytes| PQPublicIdentity::from_bytes(bytes).ok())
    }

    /// Number of registered peers.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the directory is empty.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl DocumentSchema for PeerKeyDirectory {
    /// Merge via set-union by `UserId`.
    ///
    /// If both sides have a key for the same `UserId`, keep the local one
    /// (they must be identical since `UserId = blake3(vk_bytes)`).
    fn merge(&mut self, remote: Self) {
        for (user_id, vk_bytes) in remote.keys {
            self.keys.entry(user_id).or_insert(vk_bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_crypto::PQIdentity;

    #[test]
    fn publish_and_lookup() {
        let identity = PQIdentity::generate();
        let user_id = identity.user_id();
        let vk_bytes = identity.verifying_key_bytes();

        let mut dir = PeerKeyDirectory::default();
        assert!(dir.publish(user_id, vk_bytes));
        assert_eq!(dir.len(), 1);

        let pubkey = dir.get(&user_id).expect("key should be present");
        assert_eq!(pubkey, identity.verifying_key());
    }

    #[test]
    fn publish_rejects_wrong_binding() {
        let identity = PQIdentity::generate();
        let wrong_user_id = [0xFFu8; 32];
        let vk_bytes = identity.verifying_key_bytes();

        let mut dir = PeerKeyDirectory::default();
        assert!(!dir.publish(wrong_user_id, vk_bytes));
        assert!(dir.is_empty());
    }

    #[test]
    fn publish_idempotent() {
        let identity = PQIdentity::generate();
        let user_id = identity.user_id();
        let vk_bytes = identity.verifying_key_bytes();

        let mut dir = PeerKeyDirectory::default();
        assert!(dir.publish(user_id, vk_bytes.clone()));
        assert!(!dir.publish(user_id, vk_bytes));
        assert_eq!(dir.len(), 1);
    }

    #[test]
    fn merge_union() {
        let id_a = PQIdentity::generate();
        let id_b = PQIdentity::generate();

        let mut dir_a = PeerKeyDirectory::default();
        dir_a.publish(id_a.user_id(), id_a.verifying_key_bytes());

        let mut dir_b = PeerKeyDirectory::default();
        dir_b.publish(id_b.user_id(), id_b.verifying_key_bytes());

        dir_a.merge(dir_b);
        assert_eq!(dir_a.len(), 2);
        assert!(dir_a.get(&id_a.user_id()).is_some());
        assert!(dir_a.get(&id_b.user_id()).is_some());
    }

    #[test]
    fn verify_changeset_signature() {
        use crate::braid::changeset::{Changeset, Evidence, PatchManifest};

        let identity = PQIdentity::generate();
        let user_id = identity.user_id();

        let cs = Changeset::new(
            user_id,
            vec![],
            "test commit".into(),
            PatchManifest::default(),
            Evidence::human(user_id, None),
            1000,
            &identity,
        );

        // Publish key and verify
        let mut dir = PeerKeyDirectory::default();
        dir.publish(user_id, identity.verifying_key_bytes());

        let pubkey = dir.get(&user_id).unwrap();
        assert!(cs.verify_signature(&pubkey));
        assert!(cs.is_signed());

        // Wrong key should fail
        let other = PQIdentity::generate();
        let other_pubkey = other.verifying_key();
        assert!(!cs.verify_signature(&other_pubkey));
    }
}
