//! Per-realm Project registry — a CRDT-synced document that records every
//! Project under a given Realm.
//!
//! The source of truth for project metadata; `VaultManager`'s in-memory DashMap
//! caches are populated from this document. One [`Document<ProjectRegistry>`]
//! lives under every Realm (home, DMs, Groups, Worlds) at the slot name
//! `"_projects"` — persistence, multi-device sync, and P2P propagation between
//! realm members all fall out of the shared `Document<T>` primitive.
//!
//! # CRDT semantics
//!
//! Entries are keyed by the 32-byte project id (freshly derived via
//! `artifact_interface_id(&generate_tree_id())`, so concurrent creators never
//! collide in practice). Merge is **set-union** over the `projects` map:
//!
//! - Entries present on only one side carry through unchanged.
//! - On the vanishingly-rare key collision, the entry with the larger
//!   `created_at` wins; ties (same timestamp) are broken by preferring the
//!   entry whose `manifest_head` hash compares greater, so the rule is fully
//!   deterministic on every peer.
//!
//! See [`RealmChatDocument`](indras_network::chat_message::RealmChatDocument)
//! for the canonical in-tree example this mirrors.

use std::collections::HashMap;

use indras_network::document::DocumentSchema;
use indras_network::member::MemberId;
use indras_storage::blobs::ContentRef;
use serde::{Deserialize, Serialize};

/// Id of a Project realm (32 bytes, freshly derived on creation).
pub type ProjectId = [u8; 32];

/// Per-realm catalog of every Project under a parent Realm.
///
/// Stored as a [`Document<ProjectRegistry>`] at slot name `"_projects"`; the
/// map is the single source of truth for project metadata visible to any peer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProjectRegistry {
    /// All projects registered under the owning realm, keyed by project id.
    pub projects: HashMap<ProjectId, ProjectEntry>,
}

/// One row in the [`ProjectRegistry`] — enough metadata for the UI to render
/// the project and for [`open_project`](crate::project) to fetch content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectEntry {
    /// Project realm id.
    pub id: ProjectId,
    /// Human-readable project name.
    pub name: String,
    /// Content-addressed reference to the project's current `PatchManifest`.
    pub manifest_head: ContentRef,
    /// Member who first wrote the entry (used for display and as a tiebreak
    /// hint when the UI wants to group projects by creator).
    pub creator: MemberId,
    /// Unix millis when the entry was first created on the authoring device.
    pub created_at: i64,
}

impl ProjectRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace an entry by project id.
    ///
    /// Callers should prefer this for local updates rather than touching
    /// `projects` directly so future invariants (e.g. indexing) stay
    /// encapsulated.
    pub fn insert(&mut self, entry: ProjectEntry) {
        self.projects.insert(entry.id, entry);
    }

    /// Update the `manifest_head` of an existing entry.
    ///
    /// No-op if the entry is not present — callers should `insert` first.
    pub fn set_head(&mut self, id: &ProjectId, head: ContentRef) {
        if let Some(e) = self.projects.get_mut(id) {
            e.manifest_head = head;
        }
    }

    /// Snapshot of every entry in deterministic hash-order for iteration.
    pub fn entries(&self) -> Vec<&ProjectEntry> {
        let mut out: Vec<&ProjectEntry> = self.projects.values().collect();
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
        out
    }
}

impl DocumentSchema for ProjectRegistry {
    /// Set-union merge over `projects`.
    ///
    /// * Disjoint keys: keep both.
    /// * Same key: prefer the larger `created_at`; on tie, prefer the entry
    ///   whose `manifest_head.hash` compares greater. Deterministic on every
    ///   peer so replicas converge without additional coordination.
    fn merge(&mut self, remote: Self) {
        for (pid, remote_entry) in remote.projects {
            match self.projects.get(&pid) {
                None => {
                    self.projects.insert(pid, remote_entry);
                }
                Some(local_entry) => {
                    let prefer_remote = match remote_entry
                        .created_at
                        .cmp(&local_entry.created_at)
                    {
                        std::cmp::Ordering::Greater => true,
                        std::cmp::Ordering::Less => false,
                        std::cmp::Ordering::Equal => {
                            remote_entry.manifest_head.hash > local_entry.manifest_head.hash
                        }
                    };
                    if prefer_remote {
                        self.projects.insert(pid, remote_entry);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u8, name: &str, created_at: i64) -> ProjectEntry {
        ProjectEntry {
            id: [id; 32],
            name: name.to_string(),
            manifest_head: ContentRef {
                hash: [id; 32],
                size: u64::from(id),
            },
            creator: [0u8; 32],
            created_at,
        }
    }

    #[test]
    fn merge_is_set_union() {
        // Disjoint keys → union.
        let mut a = ProjectRegistry::new();
        a.insert(entry(1, "alpha", 100));
        let mut b = ProjectRegistry::new();
        b.insert(entry(2, "beta", 200));

        a.merge(b);
        assert_eq!(a.projects.len(), 2);
        assert!(a.projects.contains_key(&[1u8; 32]));
        assert!(a.projects.contains_key(&[2u8; 32]));
    }

    #[test]
    fn merge_collision_prefers_larger_created_at() {
        let mut local = ProjectRegistry::new();
        local.insert(entry(1, "local", 100));
        let mut remote = ProjectRegistry::new();
        remote.insert(entry(1, "remote", 200));

        local.merge(remote);
        let got = local.projects.get(&[1u8; 32]).unwrap();
        assert_eq!(got.name, "remote");
        assert_eq!(got.created_at, 200);
    }

    #[test]
    fn merge_collision_tiebreaks_by_manifest_head_hash() {
        // Same created_at; manifest hashes differ so the one with the
        // lexicographically greater hash must win deterministically.
        let mut local = ProjectRegistry::new();
        let mut low = entry(1, "low", 100);
        low.manifest_head.hash = [0x01; 32];
        local.insert(low);

        let mut remote = ProjectRegistry::new();
        let mut high = entry(1, "high", 100);
        high.manifest_head.hash = [0xff; 32];
        remote.insert(high);

        local.merge(remote);
        assert_eq!(local.projects.get(&[1u8; 32]).unwrap().name, "high");
    }

    #[test]
    fn merge_collision_ignores_smaller_created_at() {
        let mut local = ProjectRegistry::new();
        local.insert(entry(1, "local", 200));
        let mut remote = ProjectRegistry::new();
        remote.insert(entry(1, "remote", 100));

        local.merge(remote);
        assert_eq!(local.projects.get(&[1u8; 32]).unwrap().name, "local");
    }

    #[test]
    fn serde_round_trip() {
        let mut reg = ProjectRegistry::new();
        reg.insert(entry(7, "seven", 12345));

        let bytes = postcard::to_allocvec(&reg).expect("encode");
        let decoded: ProjectRegistry = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(reg, decoded);
    }

    #[test]
    fn entries_sorted_by_created_at_then_id() {
        let mut reg = ProjectRegistry::new();
        reg.insert(entry(2, "b", 200));
        reg.insert(entry(1, "a", 100));
        reg.insert(entry(3, "c", 200));

        let names: Vec<&str> = reg.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
}
