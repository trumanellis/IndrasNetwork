//! Changeset types: the verified, broadcastable unit of change in the braid.
//!
//! A `Changeset` carries a [`SymlinkIndex`] — the full state of the
//! content-addressed filesystem at that point — plus an [`IndexDelta`]
//! describing what changed from the first parent. The actual bytes live in
//! the global content store; the changeset just references content
//! addresses.
//!
//! Legacy [`PatchManifest`] and [`PatchFile`] types are re-exported as
//! conversions to/from [`SymlinkIndex`] for migration.

use crate::content_addr::{ContentAddr, IndexDelta, LogicalPath, SymlinkIndex};
use crate::vault::vault_file::UserId;
use indras_crypto::{PQIdentity, PQPublicIdentity, PQSignature};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Content-addressed identifier for a [`Changeset`].
///
/// Computed as the blake3 hash of a canonical postcard encoding of
/// `(author, sorted_parents, index, intent, timestamp_millis)`.
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

// ── Legacy compat types ────────────────────────────────────────────────
// Kept during migration so downstream code can convert incrementally.

/// One file referenced by a [`PatchManifest`]: path + vault content hash + size.
///
/// **Migration note**: prefer [`SymlinkIndex`] entries (`LogicalPath → ContentAddr`)
/// for new code. `PatchFile` is retained for backward compatibility.
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
/// **Migration note**: prefer [`SymlinkIndex`] for new code.
/// `PatchManifest` is retained for backward compatibility and converts
/// to/from `SymlinkIndex` via `From` impls.
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

impl From<SymlinkIndex> for PatchManifest {
    fn from(idx: SymlinkIndex) -> Self {
        let files = idx
            .entries
            .into_iter()
            .map(|(path, addr)| PatchFile {
                path: path.0,
                hash: addr.hash,
                size: addr.size,
            })
            .collect();
        // Already sorted because BTreeMap iterates in order.
        PatchManifest { files }
    }
}

impl From<PatchManifest> for SymlinkIndex {
    fn from(manifest: PatchManifest) -> Self {
        SymlinkIndex::from_iter(manifest.files.into_iter().map(|pf| {
            (
                LogicalPath::new(pf.path),
                ContentAddr::new(pf.hash, pf.size),
            )
        }))
    }
}

impl From<&PatchManifest> for SymlinkIndex {
    fn from(manifest: &PatchManifest) -> Self {
        SymlinkIndex::from_iter(manifest.files.iter().map(|pf| {
            (
                LogicalPath::new(&pf.path),
                ContentAddr::new(pf.hash, pf.size),
            )
        }))
    }
}

impl From<&SymlinkIndex> for PatchManifest {
    fn from(idx: &SymlinkIndex) -> Self {
        let files = idx
            .iter()
            .map(|(path, addr)| PatchFile {
                path: path.0.clone(),
                hash: addr.hash,
                size: addr.size,
            })
            .collect();
        PatchManifest { files }
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
///
/// The `index` field is the full [`SymlinkIndex`] at this point — the
/// complete content-addressed filesystem state. The `delta` is what
/// changed from the first parent's index (or all-`Add` for root
/// changesets). `delta` is NOT included in the `ChangeId` hash.
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
    /// Full symlink index at this point — the complete filesystem state.
    pub index: SymlinkIndex,
    /// What changed from the first parent's index. Not in ChangeId hash.
    #[serde(default)]
    pub delta: IndexDelta,
    /// Signed verification outcome.
    pub evidence: Evidence,
    /// Authoring time in milliseconds since the Unix epoch.
    pub timestamp_millis: i64,
    /// ML-DSA-65 signature over the `ChangeId` bytes.
    pub signature: PQSignature,
}

/// Backward-compatible accessor: the `patch` field was renamed to `index`.
impl Changeset {
    /// Legacy accessor — returns the index as a `PatchManifest`.
    pub fn patch(&self) -> PatchManifest {
        PatchManifest::from(&self.index)
    }
}

impl Changeset {
    /// Compute the deterministic `ChangeId` for the given fields.
    ///
    /// `parents` is sorted internally. The `delta` is NOT included — it
    /// is derived metadata.
    pub fn compute_id(
        author: &UserId,
        parents: &[ChangeId],
        index: &SymlinkIndex,
        intent: &str,
        timestamp_millis: i64,
    ) -> ChangeId {
        let mut sorted_parents: Vec<ChangeId> = parents.to_vec();
        sorted_parents.sort();
        // Convert SymlinkIndex to PatchManifest for hash computation to
        // maintain backward compatibility with existing ChangeIds during
        // migration. TODO: hash SymlinkIndex directly once migration is
        // complete.
        let patch: PatchManifest = index.into();
        let payload = (author, &sorted_parents, &patch, intent, timestamp_millis);
        let bytes = postcard::to_allocvec(&payload)
            .expect("Changeset::compute_id: postcard serialization is infallible for these types");
        ChangeId(*blake3::hash(&bytes).as_bytes())
    }

    /// Construct a new signed `Changeset`, filling `id` via `compute_id`.
    ///
    /// The stored `parents` vector is sorted so on-wire representations are
    /// deterministic regardless of caller order.
    ///
    /// `delta` is computed automatically: if a `parent_index` is provided
    /// (the first parent's SymlinkIndex), the delta is `index.diff(parent_index)`.
    /// If `None` (root changeset), every entry is `Add`.
    pub fn with_index(
        author: UserId,
        mut parents: Vec<ChangeId>,
        intent: String,
        index: SymlinkIndex,
        parent_index: Option<&SymlinkIndex>,
        evidence: Evidence,
        timestamp_millis: i64,
        identity: &PQIdentity,
    ) -> Self {
        parents.sort();
        let id = Self::compute_id(&author, &parents, &index, &intent, timestamp_millis);
        let delta = match parent_index {
            Some(pi) => index.diff(pi),
            None => IndexDelta::from_root(&index),
        };
        let signature = identity.sign(id.as_bytes());
        Self {
            id,
            author,
            parents,
            intent,
            index,
            delta,
            evidence,
            timestamp_millis,
            signature,
        }
    }

    /// Construct an unsigned `Changeset` with a dummy signature.
    ///
    /// For unit tests and offline analysis where no signing identity is
    /// available. The dummy signature will never pass verification.
    pub fn new_unsigned(
        author: UserId,
        mut parents: Vec<ChangeId>,
        intent: String,
        index: SymlinkIndex,
        parent_index: Option<&SymlinkIndex>,
        evidence: Evidence,
        timestamp_millis: i64,
    ) -> Self {
        parents.sort();
        let id = Self::compute_id(&author, &parents, &index, &intent, timestamp_millis);
        let delta = match parent_index {
            Some(pi) => index.diff(pi),
            None => IndexDelta::from_root(&index),
        };
        Self {
            id,
            author,
            parents,
            intent,
            index,
            delta,
            evidence,
            timestamp_millis,
            signature: PQSignature::dummy(),
        }
    }

    /// Verify the changeset's ML-DSA-65 signature against a verifying key.
    ///
    /// Returns `true` if the signature is valid for this changeset's `ChangeId`.
    /// The caller is responsible for ensuring the verifying key belongs to
    /// `self.author` (i.e., `blake3(vk_bytes) == self.author`).
    pub fn verify_signature(&self, verifying_key: &PQPublicIdentity) -> bool {
        verifying_key.verify(self.id.as_bytes(), &self.signature)
    }

    /// Whether this changeset carries a real signature (not a dummy).
    pub fn is_signed(&self) -> bool {
        self.signature != PQSignature::dummy()
    }

    /// Construct a new unsigned `Changeset` from a legacy `PatchManifest`.
    ///
    /// Converts the manifest to a `SymlinkIndex` internally. Delta is
    /// computed as all-`Add` (no parent index provided). Uses a dummy
    /// signature — for tests and backward compat.
    pub fn new(
        author: UserId,
        parents: Vec<ChangeId>,
        intent: String,
        patch: PatchManifest,
        evidence: Evidence,
        timestamp_millis: i64,
    ) -> Self {
        let index: SymlinkIndex = patch.into();
        Self::new_unsigned(author, parents, intent, index, None, evidence, timestamp_millis)
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

    fn sample_index() -> SymlinkIndex {
        SymlinkIndex::from_iter([(
            LogicalPath::new("src/lib.rs"),
            ContentAddr::new([7u8; 32], 0),
        )])
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

        let a = Changeset::new_unsigned(
            author,
            vec![p1, p2],
            "intent".into(),
            patch.clone(),
            sample_evidence(author),
            100,
        );
        let b = Changeset::new_unsigned(
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

        let base = Changeset::new_unsigned(
            author,
            parents.clone(),
            "a".into(),
            patch.clone(),
            ev.clone(),
            100,
        );

        let diff_author = Changeset::new_unsigned(
            [9u8; 32],
            parents.clone(),
            "a".into(),
            patch.clone(),
            ev.clone(),
            100,
        );
        let diff_parents = Changeset::new_unsigned(
            author,
            vec![ChangeId([4u8; 32])],
            "a".into(),
            patch.clone(),
            ev.clone(),
            100,
        );
        let diff_intent = Changeset::new_unsigned(
            author,
            parents.clone(),
            "b".into(),
            patch.clone(),
            ev.clone(),
            100,
        );
        let diff_patch = Changeset::new_unsigned(
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
        let diff_time = Changeset::new_unsigned(author, parents, "a".into(), patch, ev, 101);

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

    #[test]
    fn symlink_index_patch_manifest_roundtrip() {
        let idx = sample_index();
        let manifest: PatchManifest = idx.clone().into();
        let idx2: SymlinkIndex = manifest.into();
        assert_eq!(idx, idx2);
    }

    #[test]
    fn with_index_computes_delta() {
        let parent_index = SymlinkIndex::from_iter([
            (LogicalPath::new("a.rs"), ContentAddr::new([1; 32], 10)),
            (LogicalPath::new("b.rs"), ContentAddr::new([2; 32], 20)),
        ]);
        let child_index = SymlinkIndex::from_iter([
            (LogicalPath::new("a.rs"), ContentAddr::new([1; 32], 10)),
            (LogicalPath::new("b.rs"), ContentAddr::new([3; 32], 30)), // modified
            (LogicalPath::new("c.rs"), ContentAddr::new([4; 32], 40)), // added
        ]);

        let cs = Changeset::with_index(
            [0u8; 32],
            vec![],
            "test".into(),
            child_index,
            Some(&parent_index),
            sample_evidence([0u8; 32]),
            100,
        );

        assert_eq!(cs.delta.len(), 2); // b.rs modified, c.rs added
        assert!(!cs.delta.ops.contains_key(&LogicalPath::new("a.rs"))); // unchanged
    }

    #[test]
    fn root_changeset_delta_is_all_add() {
        let idx = sample_index();
        let cs = Changeset::with_index(
            [0u8; 32],
            vec![],
            "root".into(),
            idx,
            None,
            sample_evidence([0u8; 32]),
            100,
        );
        assert_eq!(cs.delta.len(), 1);
        assert!(matches!(
            cs.delta.ops[&LogicalPath::new("src/lib.rs")],
            crate::content_addr::DeltaOp::Add(_)
        ));
    }

    #[test]
    fn new_and_with_index_produce_same_id() {
        let author: UserId = [1u8; 32];
        let patch = sample_patch();
        let idx: SymlinkIndex = patch.clone().into();

        let from_new = Changeset::new(
            author, vec![], "intent".into(), patch,
            sample_evidence(author), 100,
        );
        let from_with = Changeset::with_index(
            author, vec![], "intent".into(), idx, None,
            sample_evidence(author), 100,
        );
        assert_eq!(from_new.id, from_with.id);
    }

    #[test]
    fn legacy_patch_accessor() {
        let cs = Changeset::new(
            [0u8; 32], vec![], "test".into(), sample_patch(),
            sample_evidence([0u8; 32]), 100,
        );
        let patch = cs.patch();
        assert_eq!(patch.files.len(), 1);
        assert_eq!(patch.files[0].path, "src/lib.rs");
    }
}
