//! Peer key directory — CRDT document mapping `UserId` to PQ public keys.
//!
//! Each peer publishes two keys when it first joins a vault:
//!
//! - Its **ML-DSA-65 verifying key** (identity-bound: `UserId = blake3(vk_bytes)`).
//! - Its **ML-KEM-768 encapsulation key** (for peers that want to encrypt
//!   something to this user — notably a steward share in the Backup-plan
//!   flow).
//!
//! Merge is insert-only per map: once published, a key cannot be
//! overwritten. Verifying keys are immutable by construction (a different
//! key would hash to a different `UserId`); KEM keys rotate out-of-band
//! and the directory records the first value seen per peer.

use crate::vault::vault_file::UserId;
use indras_crypto::pq_kem::{PQEncapsulationKey, PQ_ENCAPSULATION_KEY_SIZE};
use indras_crypto::PQPublicIdentity;
use indras_network::document::DocumentSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// CRDT document: maps `UserId` to raw PQ public key bytes.
///
/// Two parallel maps — one for ML-DSA-65 verifying keys (used to verify
/// changeset signatures), one for ML-KEM-768 encapsulation keys (used to
/// encrypt to a peer, e.g. a steward share). Separate so the rollout is
/// incremental: older peers that only published a verifying key still
/// merge cleanly and appear signature-verifiable but not steward-eligible
/// until they re-publish.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerKeyDirectory {
    /// UserId → ML-DSA-65 verifying key bytes (1,952 bytes each).
    pub keys: BTreeMap<UserId, Vec<u8>>,
    /// UserId → ML-KEM-768 encapsulation key bytes (1,184 bytes each).
    #[serde(default)]
    pub kem_keys: BTreeMap<UserId, Vec<u8>>,
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

    /// Publish a peer's ML-KEM-768 encapsulation key.
    ///
    /// Validates the byte length only — KEM keys are not identity-bound
    /// by hash. Returns `true` if newly inserted, `false` if the size is
    /// wrong or an entry already exists for this `UserId`.
    pub fn publish_kem(&mut self, user_id: UserId, ek_bytes: Vec<u8>) -> bool {
        if ek_bytes.len() != PQ_ENCAPSULATION_KEY_SIZE {
            return false;
        }
        if self.kem_keys.contains_key(&user_id) {
            return false;
        }
        self.kem_keys.insert(user_id, ek_bytes);
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

    /// Look up a peer's ML-KEM-768 encapsulation key by `UserId`.
    pub fn get_kem(&self, user_id: &UserId) -> Option<PQEncapsulationKey> {
        self.kem_keys
            .get(user_id)
            .and_then(|bytes| PQEncapsulationKey::from_bytes(bytes).ok())
    }

    /// Enumerate peers that have published a KEM encapsulation key,
    /// along with the decoded key. Suitable for "pick a backup friend"
    /// UIs that need to encrypt a share to a peer.
    pub fn peers_with_kem(&self) -> Vec<(UserId, PQEncapsulationKey)> {
        self.kem_keys
            .iter()
            .filter_map(|(uid, bytes)| {
                PQEncapsulationKey::from_bytes(bytes)
                    .ok()
                    .map(|ek| (*uid, ek))
            })
            .collect()
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
        for (user_id, ek_bytes) in remote.kem_keys {
            self.kem_keys.entry(user_id).or_insert(ek_bytes);
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
    fn publish_kem_and_lookup() {
        use indras_crypto::pq_kem::PQKemKeyPair;
        let identity = PQIdentity::generate();
        let kem = PQKemKeyPair::generate();

        let mut dir = PeerKeyDirectory::default();
        assert!(dir.publish_kem(identity.user_id(), kem.encapsulation_key_bytes()));

        let recovered = dir.get_kem(&identity.user_id()).expect("kem key present");
        assert_eq!(recovered.to_bytes(), kem.encapsulation_key_bytes());
    }

    #[test]
    fn publish_kem_rejects_wrong_size() {
        let identity = PQIdentity::generate();
        let mut dir = PeerKeyDirectory::default();
        assert!(!dir.publish_kem(identity.user_id(), vec![0u8; 10]));
        assert!(dir.kem_keys.is_empty());
    }

    #[test]
    fn publish_kem_idempotent() {
        use indras_crypto::pq_kem::PQKemKeyPair;
        let identity = PQIdentity::generate();
        let kem = PQKemKeyPair::generate();
        let ek = kem.encapsulation_key_bytes();

        let mut dir = PeerKeyDirectory::default();
        assert!(dir.publish_kem(identity.user_id(), ek.clone()));
        assert!(!dir.publish_kem(identity.user_id(), ek));
        assert_eq!(dir.kem_keys.len(), 1);
    }

    #[test]
    fn peers_with_kem_returns_only_published() {
        use indras_crypto::pq_kem::PQKemKeyPair;
        let a = PQIdentity::generate();
        let b = PQIdentity::generate();
        let kem_a = PQKemKeyPair::generate();

        let mut dir = PeerKeyDirectory::default();
        dir.publish(a.user_id(), a.verifying_key_bytes());
        dir.publish_kem(a.user_id(), kem_a.encapsulation_key_bytes());
        dir.publish(b.user_id(), b.verifying_key_bytes());
        // b intentionally publishes no KEM key — must not show up.

        let list = dir.peers_with_kem();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, a.user_id());
    }

    #[test]
    fn merge_unions_kem_keys() {
        use indras_crypto::pq_kem::PQKemKeyPair;
        let a = PQIdentity::generate();
        let b = PQIdentity::generate();
        let kem_a = PQKemKeyPair::generate();
        let kem_b = PQKemKeyPair::generate();

        let mut dir_a = PeerKeyDirectory::default();
        dir_a.publish(a.user_id(), a.verifying_key_bytes());
        dir_a.publish_kem(a.user_id(), kem_a.encapsulation_key_bytes());

        let mut dir_b = PeerKeyDirectory::default();
        dir_b.publish(b.user_id(), b.verifying_key_bytes());
        dir_b.publish_kem(b.user_id(), kem_b.encapsulation_key_bytes());

        dir_a.merge(dir_b);
        assert_eq!(dir_a.peers_with_kem().len(), 2);
        assert!(dir_a.get_kem(&a.user_id()).is_some());
        assert!(dir_a.get_kem(&b.user_id()).is_some());
    }

    #[test]
    fn verify_changeset_signature() {
        use crate::braid::changeset::{Changeset, Evidence, PatchManifest};

        let identity = PQIdentity::generate();
        let user_id = identity.user_id();

        let cs = Changeset::with_index(
            user_id,
            vec![],
            "test commit".into(),
            crate::content_addr::SymlinkIndex::new(),
            None,
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
