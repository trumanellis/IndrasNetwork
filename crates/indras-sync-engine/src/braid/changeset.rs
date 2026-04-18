//! Changeset types: the verified, broadcastable unit of change in the braid.
//!
//! The `patch` field of a `Changeset` is a [`PatchManifest`] — a list of
//! `(path, content_hash)` entries describing which vault file versions make
//! up this changeset. The blobs themselves already live in the vault's
//! content-addressed storage; this is simply the snapshot of which file
//! states constitute this changeset.

use crate::vault::vault_file::UserId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Content-addressed identifier for a [`Changeset`].
///
/// Computed as the blake3 hash of a canonical postcard encoding of
/// `(author, sorted_parents, patch_manifest, intent, timestamp_millis)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ChangeId(pub [u8; 32]);

impl ChangeId {
    /// Return the raw 32-byte hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ChangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

/// One file referenced by a [`PatchManifest`]: path + vault content hash + size.
///
/// `size` is carried so a peer can reconstruct a `ContentRef` and drive
/// `SyncToDisk` to materialize the blob without first consulting the
/// vault index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
pub struct PatchFile {
    /// Vault-relative path (forward slashes), e.g. `"src/lib.rs"`.
    pub path: String,
    /// BLAKE3 content hash of the file version at the time of the changeset.
    pub hash: [u8; 32],
    /// Content length in bytes.
    pub size: u64,
}

/// A changeset's patch is a manifest of vault file versions by content hash.
///
/// The blobs already live in the vault's content-addressed storage — this is
/// just the snapshot of which file states constitute this changeset. To
/// "apply" a changeset is to request those hashes from the vault and write
/// them to disk; the braid does not carry bytes itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchManifest {
    /// Files referenced by this patch, sorted by path for deterministic hashing.
    pub files: Vec<PatchFile>,
}

impl PatchManifest {
    /// Construct a manifest, sorting `files` by path for deterministic hashing.
    pub fn new(mut files: Vec<PatchFile>) -> Self {
        files.sort();
        Self { files }
    }
}

/// Proof of intent attached to a `Changeset`.
///
/// Peers trust signed `Evidence` on incoming changesets and do not re-run
/// verification on import.
///
/// - [`Evidence::Agent`]: automated verification (build + test + lint).
/// - [`Evidence::Human`]: explicit user approval to publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    /// Agent verification: build/test/lint results from cargo.
    Agent {
        /// Whether `cargo build` succeeded.
        compiled: bool,
        /// Names of the `cargo test -p <crate>` runs that passed.
        tests_passed: Vec<String>,
        /// Whether `cargo clippy -- -D warnings` was clean.
        lints_clean: bool,
        /// Total wall-clock runtime of the verification suite, in milliseconds.
        runtime_ms: u64,
        /// Agent who produced and signed this evidence.
        signed_by: UserId,
    },
    /// Human approval: explicit user consent to publish.
    Human {
        /// The user who approved this sync.
        approved_by: UserId,
        /// Timestamp of approval (Unix millis).
        approved_at_ms: i64,
        /// Optional message from the user.
        message: Option<String>,
    },
}

impl Evidence {
    /// Return the identity that signed / approved this evidence.
    pub fn signed_by(&self) -> UserId {
        match self {
            Evidence::Agent { signed_by, .. } => *signed_by,
            Evidence::Human { approved_by, .. } => *approved_by,
        }
    }

    /// Convenience constructor for human-approved evidence.
    pub fn human(user_id: UserId, message: Option<String>) -> Self {
        Evidence::Human {
            approved_by: user_id,
            approved_at_ms: chrono::Utc::now().timestamp_millis(),
            message,
        }
    }
}

/// A verified, broadcastable unit of change.
///
/// Parents are DAG edges: concurrent heads produce a braid. `id` is a
/// content hash over the other fields so duplicate changesets collapse
/// naturally under set-union merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changeset {
    /// Content-addressed id (blake3 of canonical encoding of the rest).
    pub id: ChangeId,
    /// Agent who authored this changeset.
    pub author: UserId,
    /// Parent changeset ids (stored sorted for deterministic hashing).
    pub parents: Vec<ChangeId>,
    /// Human-readable intent / commit message.
    pub intent: String,
    /// Manifest of vault file versions constituting this changeset.
    pub patch: PatchManifest,
    /// Signed verification outcome.
    pub evidence: Evidence,
    /// Authoring time in milliseconds since the Unix epoch.
    pub timestamp_millis: i64,
}

impl Changeset {
    /// Compute the deterministic `ChangeId` for the given fields.
    ///
    /// `parents` is sorted internally.
    pub fn compute_id(
        author: &UserId,
        parents: &[ChangeId],
        patch: &PatchManifest,
        intent: &str,
        timestamp_millis: i64,
    ) -> ChangeId {
        let mut sorted_parents: Vec<ChangeId> = parents.to_vec();
        sorted_parents.sort();
        let payload = (author, &sorted_parents, patch, intent, timestamp_millis);
        let bytes = postcard::to_allocvec(&payload)
            .expect("Changeset::compute_id: postcard serialization is infallible for these types");
        ChangeId(*blake3::hash(&bytes).as_bytes())
    }

    /// Construct a new `Changeset`, filling `id` via `compute_id`.
    ///
    /// The stored `parents` vector is sorted so on-wire representations are
    /// deterministic regardless of caller order.
    pub fn new(
        author: UserId,
        mut parents: Vec<ChangeId>,
        intent: String,
        patch: PatchManifest,
        evidence: Evidence,
        timestamp_millis: i64,
    ) -> Self {
        parents.sort();
        let id = Self::compute_id(&author, &parents, &patch, &intent, timestamp_millis);
        Self {
            id,
            author,
            parents,
            intent,
            patch,
            evidence,
            timestamp_millis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_evidence(agent: UserId) -> Evidence {
        Evidence::Agent {
            compiled: true,
            tests_passed: vec!["indras-sync-engine".into()],
            lints_clean: true,
            runtime_ms: 1234,
            signed_by: agent,
        }
    }

    fn sample_patch() -> PatchManifest {
        PatchManifest::new(vec![PatchFile {
            path: "src/lib.rs".into(),
            hash: [7u8; 32],
            size: 0,
        }])
    }

    #[test]
    fn changeset_id_is_deterministic() {
        let author: UserId = [1u8; 32];
        let p1 = ChangeId([2u8; 32]);
        let p2 = ChangeId([3u8; 32]);
        let patch = sample_patch();

        let a = Changeset::new(
            author,
            vec![p1, p2],
            "intent".into(),
            patch.clone(),
            sample_evidence(author),
            100,
        );
        let b = Changeset::new(
            author,
            vec![p2, p1],
            "intent".into(),
            patch,
            sample_evidence(author),
            100,
        );
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn changeset_id_differs_on_any_field_change() {
        let author: UserId = [1u8; 32];
        let parents = vec![ChangeId([2u8; 32])];
        let patch = sample_patch();
        let ev = sample_evidence(author);

        let base = Changeset::new(
            author,
            parents.clone(),
            "a".into(),
            patch.clone(),
            ev.clone(),
            100,
        );

        let diff_author = Changeset::new(
            [9u8; 32],
            parents.clone(),
            "a".into(),
            patch.clone(),
            ev.clone(),
            100,
        );
        let diff_parents = Changeset::new(
            author,
            vec![ChangeId([4u8; 32])],
            "a".into(),
            patch.clone(),
            ev.clone(),
            100,
        );
        let diff_intent = Changeset::new(
            author,
            parents.clone(),
            "b".into(),
            patch.clone(),
            ev.clone(),
            100,
        );
        let diff_patch = Changeset::new(
            author,
            parents.clone(),
            "a".into(),
            PatchManifest::new(vec![PatchFile {
                path: "src/lib.rs".into(),
                hash: [8u8; 32],
                size: 0,
            }]),
            ev.clone(),
            100,
        );
        let diff_time = Changeset::new(author, parents, "a".into(), patch, ev, 101);

        for other in [diff_author, diff_parents, diff_intent, diff_patch, diff_time] {
            assert_ne!(
                base.id, other.id,
                "id should change when any hashed field changes"
            );
        }
    }

    #[test]
    fn patch_manifest_new_sorts() {
        let m = PatchManifest::new(vec![
            PatchFile { path: "z.rs".into(), hash: [0u8; 32], size: 0 },
            PatchFile { path: "a.rs".into(), hash: [1u8; 32], size: 0 },
        ]);
        assert_eq!(m.files[0].path, "a.rs");
        assert_eq!(m.files[1].path, "z.rs");
    }
}
