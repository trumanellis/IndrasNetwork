//! Backup-peer role + selection logic.
//!
//! A *Backup Peer* is a DM peer this user has asked to hold
//! erasure-coded shards of their files. Distinct from a Steward
//! (who holds a share of recovery authority), Backup Peers carry
//! opaque ciphertext they can't read without the per-file keys.
//!
//! Plan-C introduces:
//!
//! 1. A per-peer role assignment — written as a small CRDT doc
//!    into the user↔peer DM realm so the peer's UI can surface
//!    "you're a backup peer for {name}" in their inbox.
//! 2. A [`BackupPeerPlan`] view-model for the sender's UI — which
//!    peers currently hold shards and their rough liveness.
//!
//! The actual file sharding / publication happens in
//! `file_shard` (slice C.3). This module is the *who* side of that
//! pipeline.

use serde::{Deserialize, Serialize};

use indras_network::document::DocumentSchema;

/// Doc key prefix a user writes into a peer's DM realm to ask them
/// to hold shards. Suffixed with the requester's UID so a steward
/// who is also a backup peer doesn't clobber one role with the
/// other.
pub const BACKUP_ROLE_KEY_PREFIX: &str = "_backup_role:";

/// Build the CRDT doc key for the requester writing a role
/// assignment into the peer's DM realm.
pub fn backup_role_doc_key(requester_user_id: &[u8; 32]) -> String {
    format!("{}{}", BACKUP_ROLE_KEY_PREFIX, hex::encode(requester_user_id))
}

/// Assignment carried in the `_backup_role:{requester_uid}` doc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupPeerAssignment {
    /// Requester's `UserId`. Readers pin this against the doc-key
    /// suffix so a malicious peer can't squat another requester's
    /// slot.
    pub requester_user_id: [u8; 32],
    /// Requester's display-name at assignment time — rendered in
    /// the peer's inbox.
    pub requester_display_name: String,
    /// Plain-language description the peer sees ("hold backup
    /// pieces of my files — no private data, nothing you can read").
    pub responsibility_text: String,
    /// Approximate number of shards the requester intends to
    /// distribute across their backup peers. Informational only.
    pub shard_capacity_estimate: u32,
    /// Wall-clock millis of the assignment.
    pub assigned_at_millis: i64,
    /// `true` once the requester has retired this peer from their
    /// backup set. Readers stop pulling shards.
    pub retired: bool,
}

impl DocumentSchema for BackupPeerAssignment {
    fn merge(&mut self, remote: Self) {
        if remote.assigned_at_millis > self.assigned_at_millis {
            *self = remote;
        }
    }
}

/// Default, plain-language description of the backup-peer role.
pub const DEFAULT_BACKUP_RESPONSIBILITY: &str =
    "You'd hold a few pieces of their backup files. The pieces are \
sealed — you can't read what's inside — and any one of them is useless \
on its own. If they lose their device, their pieces come back from \
you and their other backup peers to rebuild the files.";

/// Planner output describing which peers currently hold backup
/// shards for the user.
#[derive(Debug, Clone, Default)]
pub struct BackupPeerPlan {
    /// Sender-side summary of active backup peers.
    pub peers: Vec<BackupPeerEntry>,
}

/// One backup peer in the plan.
#[derive(Debug, Clone)]
pub struct BackupPeerEntry {
    /// Peer's `UserId` hex.
    pub peer_user_id_hex: String,
    /// Peer's display name (falls back to hex prefix).
    pub peer_label: String,
    /// Most recent online indicator from the peer-liveness map.
    pub online: bool,
    /// Whether this peer currently has an active assignment.
    pub active: bool,
}

impl BackupPeerPlan {
    /// Select the top-N most reliable peers for sharding. Current
    /// ranking: online first, then alphabetical by label so the
    /// selection is deterministic for tests.
    pub fn select_top(
        mut candidates: Vec<BackupPeerEntry>,
        target_count: usize,
    ) -> BackupPeerPlan {
        candidates.sort_by(|a, b| {
            b.online
                .cmp(&a.online)
                .then_with(|| a.peer_label.to_ascii_lowercase().cmp(&b.peer_label.to_ascii_lowercase()))
        });
        candidates.truncate(target_count);
        BackupPeerPlan { peers: candidates }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_key_is_stable_and_prefixed() {
        let uid = [0xccu8; 32];
        let k = backup_role_doc_key(&uid);
        assert!(k.starts_with(BACKUP_ROLE_KEY_PREFIX));
        assert_eq!(k, backup_role_doc_key(&uid));
        assert_eq!(k.len(), BACKUP_ROLE_KEY_PREFIX.len() + 64);
    }

    #[test]
    fn assignment_merge_prefers_newer() {
        let old = BackupPeerAssignment {
            requester_user_id: [1; 32],
            requester_display_name: "v1".into(),
            responsibility_text: "old".into(),
            shard_capacity_estimate: 3,
            assigned_at_millis: 100,
            retired: false,
        };
        let new = BackupPeerAssignment {
            requester_display_name: "v2".into(),
            responsibility_text: "new".into(),
            assigned_at_millis: 500,
            ..old.clone()
        };
        let mut a = old.clone();
        a.merge(new.clone());
        assert_eq!(a.assigned_at_millis, 500);
        assert_eq!(a.requester_display_name, "v2");
        let mut b = new;
        b.merge(old);
        assert_eq!(b.requester_display_name, "v2");
    }

    #[test]
    fn select_top_prefers_online_then_alphabetical() {
        let e = |name: &str, online: bool| BackupPeerEntry {
            peer_user_id_hex: format!("{name}-uid"),
            peer_label: name.into(),
            online,
            active: false,
        };
        let input = vec![
            e("Zelda", true),
            e("bob", false),
            e("Alice", true),
            e("Carol", false),
        ];
        let plan = BackupPeerPlan::select_top(input, 3);
        let names: Vec<&str> = plan.peers.iter().map(|p| p.peer_label.as_str()).collect();
        assert_eq!(names, vec!["Alice", "Zelda", "bob"]);
    }

    #[test]
    fn retired_assignment_still_merges_by_timestamp() {
        let active = BackupPeerAssignment {
            requester_user_id: [2; 32],
            requester_display_name: "A".into(),
            responsibility_text: "r".into(),
            shard_capacity_estimate: 1,
            assigned_at_millis: 100,
            retired: false,
        };
        let retired = BackupPeerAssignment {
            retired: true,
            assigned_at_millis: 200,
            ..active.clone()
        };
        let mut state = active.clone();
        state.merge(retired);
        assert!(state.retired);
    }
}
