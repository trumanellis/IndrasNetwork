//! VaultFileDocument — CRDT document tracking the vault file tree.
//!
//! Merge strategy: set-union by path key with per-file LWW by `modified_ms`.
//! Concurrent edits within `CONFLICT_WINDOW_MS` create conflict records.

use super::vault_file::{ConflictRecord, UserId, VaultFile, CONFLICT_WINDOW_MS};
use crate::team::Team;
use indras_network::document::DocumentSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// CRDT document tracking all files in a synced vault.
///
/// Merge strategy: set-union by path key with per-file LWW by `modified_ms`.
/// Concurrent edits within `CONFLICT_WINDOW_MS` create conflict records.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultFileDocument {
    /// All tracked files, keyed by relative path.
    pub files: BTreeMap<String, VaultFile>,
    /// Detected conflicts awaiting resolution.
    pub conflicts: Vec<ConflictRecord>,
    /// Team associated with this vault — roster of logical AI agents plus
    /// the id of the team realm where the braid DAG gossips. Default is
    /// empty (no team, no DAG channel) for vaults without agents.
    #[serde(default)]
    pub team: Team,
}

impl DocumentSchema for VaultFileDocument {
    fn merge(&mut self, remote: Self) {
        for (path, remote_file) in remote.files {
            match self.files.get(&path) {
                Some(local)
                    if !local.deleted
                        && !remote_file.deleted
                        && local.hash != remote_file.hash =>
                {
                    // Same-author edits are sequential (not concurrent), so
                    // just use LWW without conflict detection. This prevents
                    // spurious conflicts when historical messages are replayed
                    // (e.g., the Document listener's refresh_from_crdt).
                    if local.author == remote_file.author {
                        if remote_file.modified_ms > local.modified_ms {
                            self.files.insert(path, remote_file);
                        }
                    } else {
                        let time_gap =
                            (local.modified_ms - remote_file.modified_ms).abs();
                        if time_gap < CONFLICT_WINDOW_MS {
                            // Concurrent edit by different authors — keep LWW winner,
                            // record conflict for loser
                            let (winner, loser) =
                                if remote_file.modified_ms >= local.modified_ms {
                                    (remote_file.clone(), local.clone())
                                } else {
                                    (local.clone(), remote_file.clone())
                                };
                            let conflict = ConflictRecord {
                                path: path.clone(),
                                winner_hash: winner.hash,
                                loser_hash: loser.hash,
                                loser_author: loser.author,
                                detected_ms: chrono::Utc::now().timestamp_millis(),
                                resolved: false,
                            };
                            // Dedup: don't add if same (path, loser_hash) exists
                            let dominated = self
                                .conflicts
                                .iter()
                                .any(|c| c.path == path && c.loser_hash == loser.hash);
                            if !dominated {
                                self.conflicts.push(conflict);
                            }
                            self.files.insert(path, winner);
                        } else if remote_file.modified_ms > local.modified_ms {
                            // Remote is clearly newer
                            self.files.insert(path, remote_file);
                        }
                        // else: local is newer, keep it
                    }
                }
                Some(local) => {
                    // Same hash, or one/both deleted — standard LWW
                    if remote_file.modified_ms > local.modified_ms {
                        self.files.insert(path, remote_file);
                    }
                }
                None => {
                    // New file from remote
                    self.files.insert(path, remote_file);
                }
            }
        }

        // Union conflicts, dedup by (path, loser_hash).
        // If a matching conflict exists, propagate the `resolved` flag.
        for conflict in remote.conflicts {
            if let Some(existing) = self
                .conflicts
                .iter_mut()
                .find(|c| c.path == conflict.path && c.loser_hash == conflict.loser_hash)
            {
                // Propagate resolution: once resolved on any peer, it's resolved everywhere
                if conflict.resolved {
                    existing.resolved = true;
                }
            } else {
                self.conflicts.push(conflict);
            }
        }

        self.team.merge(remote.team);
    }

    fn extract_delta(_old: &Self, _new: &Self) -> Option<Vec<u8>> {
        // Always send full state (no delta). The vault index is small
        // metadata, and deltas cause load_or_create to miss the latest
        // state when creating fresh Document handles.
        None
    }

}

impl VaultFileDocument {
    /// Insert or update a file by path.
    pub fn upsert(&mut self, file: VaultFile) {
        self.files.insert(file.path.clone(), file);
    }

    /// Tombstone a file (mark as deleted).
    pub fn remove(&mut self, path: &str, author: UserId) {
        if let Some(file) = self.files.get_mut(path) {
            file.deleted = true;
            file.modified_ms = chrono::Utc::now().timestamp_millis();
            file.author = author;
        }
    }

    /// Return all non-deleted files.
    pub fn active_files(&self) -> Vec<&VaultFile> {
        self.files.values().filter(|f| !f.deleted).collect()
    }

    /// Return all unresolved conflicts.
    pub fn unresolved_conflicts(&self) -> Vec<&ConflictRecord> {
        self.conflicts.iter().filter(|c| !c.resolved).collect()
    }

    /// Mark a conflict as resolved.
    ///
    /// Also resolves any sibling conflicts for the same path that share the
    /// same winner hash. This handles spurious conflicts created by the
    /// Document listener replaying historical events — those conflicts have
    /// different loser hashes but the same winner, and should all be resolved
    /// when the user accepts the winner.
    pub fn resolve_conflict(&mut self, path: &str, loser_hash: &[u8; 32]) {
        let winner_hash = self
            .conflicts
            .iter()
            .find(|c| c.path == path && &c.loser_hash == loser_hash)
            .map(|c| c.winner_hash);

        for conflict in &mut self.conflicts {
            if conflict.path == path {
                if &conflict.loser_hash == loser_hash {
                    conflict.resolved = true;
                } else if let Some(wh) = winner_hash {
                    if conflict.winner_hash == wh {
                        conflict.resolved = true;
                    }
                }
            }
        }
    }

    /// Count of active (non-deleted) files.
    pub fn len(&self) -> usize {
        self.files.values().filter(|f| !f.deleted).count()
    }

    /// Whether there are no active files.
    pub fn is_empty(&self) -> bool {
        !self.files.values().any(|f| !f.deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::vault_file::VaultFile;

    fn member_a() -> UserId {
        [1u8; 32]
    }
    fn member_b() -> UserId {
        [2u8; 32]
    }

    fn make_file(path: &str, content: &[u8], modified_ms: i64, author: UserId) -> VaultFile {
        let hash = *blake3::hash(content).as_bytes();
        VaultFile {
            path: path.into(),
            hash,
            size: content.len() as u64,
            modified_ms,
            author,
            deleted: false,
            content: None,
        }
    }

    // --- CRDT merge tests ---

    #[test]
    fn merge_two_different_files_union() {
        let mut doc_a = VaultFileDocument::default();
        let mut doc_b = VaultFileDocument::default();

        doc_a.upsert(make_file("a.md", b"alpha", 100, member_a()));
        doc_b.upsert(make_file("b.md", b"beta", 100, member_b()));

        doc_a.merge(doc_b);
        assert_eq!(doc_a.len(), 2);
        assert!(doc_a.files.contains_key("a.md"));
        assert!(doc_a.files.contains_key("b.md"));
    }

    #[test]
    fn merge_same_file_lww_no_conflict_outside_window() {
        let mut doc_a = VaultFileDocument::default();
        let mut doc_b = VaultFileDocument::default();

        // >60s apart, different content
        doc_a.upsert(make_file("notes.md", b"old", 100_000, member_a()));
        doc_b.upsert(make_file("notes.md", b"new", 200_000, member_b()));

        doc_a.merge(doc_b);
        assert_eq!(doc_a.len(), 1);
        // Remote wins (newer)
        assert_eq!(doc_a.files["notes.md"].author, member_b());
        // No conflict
        assert!(doc_a.conflicts.is_empty());
    }

    #[test]
    fn merge_same_file_conflict_within_window() {
        let mut doc_a = VaultFileDocument::default();
        let mut doc_b = VaultFileDocument::default();

        let now = chrono::Utc::now().timestamp_millis();
        // <60s apart, different content
        doc_a.upsert(make_file("notes.md", b"version-a", now, member_a()));
        doc_b.upsert(make_file("notes.md", b"version-b", now + 5_000, member_b()));

        doc_a.merge(doc_b);
        assert_eq!(doc_a.len(), 1);
        // Winner is remote (5s newer)
        assert_eq!(doc_a.files["notes.md"].author, member_b());
        // Conflict recorded
        assert_eq!(doc_a.conflicts.len(), 1);
        let c = &doc_a.conflicts[0];
        assert_eq!(c.path, "notes.md");
        assert_eq!(c.loser_author, member_a());
        assert!(!c.resolved);
    }

    #[test]
    fn merge_same_file_same_hash_no_conflict() {
        let mut doc_a = VaultFileDocument::default();
        let mut doc_b = VaultFileDocument::default();

        let now = chrono::Utc::now().timestamp_millis();
        // Same content, within window
        doc_a.upsert(make_file("notes.md", b"identical", now, member_a()));
        doc_b.upsert(make_file("notes.md", b"identical", now + 1_000, member_b()));

        doc_a.merge(doc_b);
        assert_eq!(doc_a.len(), 1);
        // No conflict (same hash)
        assert!(doc_a.conflicts.is_empty());
        // LWW: member_b wins (newer)
        assert_eq!(doc_a.files["notes.md"].author, member_b());
    }

    #[test]
    fn merge_delete_vs_edit_lww() {
        let mut doc_a = VaultFileDocument::default();
        let mut doc_b = VaultFileDocument::default();

        // A edits, B deletes (B is newer)
        doc_a.upsert(make_file("notes.md", b"edited", 100, member_a()));
        let mut deleted = make_file("notes.md", b"old", 200, member_b());
        deleted.deleted = true;
        doc_b.upsert(deleted);

        doc_a.merge(doc_b);
        // Delete wins (newer modified_ms)
        assert!(doc_a.files["notes.md"].deleted);
        assert_eq!(doc_a.len(), 0); // active_files excludes deleted
    }

    #[test]
    fn merge_edit_vs_delete_edit_wins_if_newer() {
        let mut doc_a = VaultFileDocument::default();
        let mut doc_b = VaultFileDocument::default();

        // A edits (newer), B deletes (older)
        doc_a.upsert(make_file("notes.md", b"edited", 300, member_a()));
        let mut deleted = make_file("notes.md", b"old", 100, member_b());
        deleted.deleted = true;
        doc_b.upsert(deleted);

        doc_a.merge(doc_b);
        // Edit wins (newer)
        assert!(!doc_a.files["notes.md"].deleted);
        assert_eq!(doc_a.len(), 1);
    }

    #[test]
    fn conflict_dedup_on_merge() {
        let mut doc_a = VaultFileDocument::default();
        let mut doc_b = VaultFileDocument::default();

        let loser_hash = *blake3::hash(b"loser-content").as_bytes();
        let conflict = ConflictRecord {
            path: "x.md".into(),
            winner_hash: [0; 32],
            loser_hash,
            loser_author: member_b(),
            detected_ms: 1000,
            resolved: false,
        };
        doc_a.conflicts.push(conflict.clone());
        doc_b.conflicts.push(conflict);

        doc_a.merge(doc_b);
        // Should not duplicate
        assert_eq!(doc_a.conflicts.len(), 1);
    }

    #[test]
    fn extract_delta_always_none() {
        // extract_delta is intentionally disabled (always returns None)
        // to ensure full-state sync for reliable Document loading.
        let mut old = VaultFileDocument::default();
        old.upsert(make_file("existing.md", b"old-content", 100, member_a()));

        let mut new = old.clone();
        new.upsert(make_file("existing.md", b"new-content", 200, member_a()));

        assert!(VaultFileDocument::extract_delta(&old, &new).is_none());
    }

    #[test]
    fn serialization_round_trip() {
        let mut doc = VaultFileDocument::default();
        doc.upsert(make_file("a.md", b"aaa", 100, member_a()));
        doc.upsert(make_file("b.md", b"bbb", 200, member_b()));
        doc.conflicts.push(ConflictRecord {
            path: "c.md".into(),
            winner_hash: [1; 32],
            loser_hash: [2; 32],
            loser_author: member_b(),
            detected_ms: 999,
            resolved: false,
        });

        let bytes = postcard::to_allocvec(&doc).unwrap();
        let decoded: VaultFileDocument = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(doc.files.len(), decoded.files.len());
        assert_eq!(doc.conflicts.len(), decoded.conflicts.len());
        for (path, file) in &doc.files {
            assert_eq!(file, &decoded.files[path]);
        }
    }

    #[test]
    fn upsert_and_remove() {
        let mut doc = VaultFileDocument::default();
        doc.upsert(make_file("a.md", b"hello", 100, member_a()));
        assert_eq!(doc.len(), 1);

        doc.remove("a.md", member_b());
        assert_eq!(doc.len(), 0);
        assert!(doc.files["a.md"].deleted);
    }

    #[test]
    fn merge_team_unions_roster() {
        use crate::team::{LogicalAgentId, Team};
        let mut a = VaultFileDocument::default();
        a.team = Team {
            roster: vec![LogicalAgentId::new("agent1")],
            ..Default::default()
        };
        let mut b = VaultFileDocument::default();
        b.team = Team {
            roster: vec![LogicalAgentId::new("agent2")],
            ..Default::default()
        };
        a.merge(b);
        assert_eq!(
            a.team.roster,
            vec![LogicalAgentId::new("agent1"), LogicalAgentId::new("agent2")]
        );
    }

    #[test]
    fn resolve_conflict_marks_resolved() {
        let mut doc = VaultFileDocument::default();
        let loser_hash = [42u8; 32];
        doc.conflicts.push(ConflictRecord {
            path: "x.md".into(),
            winner_hash: [0; 32],
            loser_hash,
            loser_author: member_b(),
            detected_ms: 1000,
            resolved: false,
        });

        assert_eq!(doc.unresolved_conflicts().len(), 1);
        doc.resolve_conflict("x.md", &loser_hash);
        assert_eq!(doc.unresolved_conflicts().len(), 0);
    }
}
