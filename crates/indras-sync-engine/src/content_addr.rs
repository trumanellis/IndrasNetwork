//! Content-addressed filesystem primitives.
//!
//! The source of truth is a global content-addressed store plus symlink
//! indexes — not files in directories. The disk is just a materialized view.
//!
//! # Core types
//!
//! - [`LogicalPath`] — a human-readable key in the content namespace (not a
//!   filesystem path).
//! - [`ContentAddr`] — a pointer to immutable content in the global store.
//! - [`SymlinkIndex`] — a mapping from logical paths to content addresses.
//!   This IS the filesystem state.
//! - [`IndexDelta`] / [`DeltaOp`] — what changed between two symlink indexes.
//! - [`Conflict`] — two participants changed the same symlink to different
//!   addresses.
//!
//! # Design
//!
//! Every edit creates a new blob in the content store and updates a symlink
//! to point to it. Old content persists forever — all changes are
//! nondestructive. Multiple indexes can coexist (one per agent, per user,
//! per peer), and merging is just reconciling which symlinks point where.

use indras_storage::ContentRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ── LogicalPath ────────────────────────────────────────────────────────

/// A human-readable key in the content namespace.
///
/// NOT a filesystem path — a logical identifier like `"src/lib.rs"`.
/// Always uses forward slashes, no leading slash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LogicalPath(pub String);

impl LogicalPath {
    /// Create a new logical path.
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// The path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LogicalPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for LogicalPath {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for LogicalPath {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ── ContentAddr ────────────────────────────────────────────────────────

/// A pointer to immutable content in the global store.
///
/// Wraps a BLAKE3 hash and size. Two `ContentAddr` values with the same
/// hash are guaranteed to reference identical bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentAddr {
    /// BLAKE3 hash of the content.
    pub hash: [u8; 32],
    /// Content length in bytes.
    pub size: u64,
}

impl ContentAddr {
    /// Create a new content address.
    pub fn new(hash: [u8; 32], size: u64) -> Self {
        Self { hash, size }
    }

    /// Compute a content address from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            hash: *blake3::hash(data).as_bytes(),
            size: data.len() as u64,
        }
    }

    /// Short hex display (first 8 chars).
    pub fn short_hex(&self) -> String {
        hex::encode(&self.hash[..4])
    }
}

impl fmt::Display for ContentAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({} B)", self.short_hex(), self.size)
    }
}

impl From<ContentRef> for ContentAddr {
    fn from(cr: ContentRef) -> Self {
        Self {
            hash: cr.hash,
            size: cr.size,
        }
    }
}

impl From<ContentAddr> for ContentRef {
    fn from(ca: ContentAddr) -> Self {
        ContentRef::new(ca.hash, ca.size)
    }
}

// ── SymlinkIndex ───────────────────────────────────────────────────────

/// The "filesystem" — a mapping from logical paths to content addresses.
///
/// This IS the state. The disk directory is just a materialized view.
/// Sorted by construction (`BTreeMap`), O(log n) lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymlinkIndex {
    /// Logical path → content address.
    pub entries: BTreeMap<LogicalPath, ContentAddr>,
}

impl SymlinkIndex {
    /// Create an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an index from an iterator of `(path, addr)` pairs.
    pub fn from_iter(iter: impl IntoIterator<Item = (LogicalPath, ContentAddr)>) -> Self {
        Self {
            entries: iter.into_iter().collect(),
        }
    }

    /// Look up the content address for a path.
    pub fn get(&self, path: &LogicalPath) -> Option<&ContentAddr> {
        self.entries.get(path)
    }

    /// Set a symlink: point `path` at `addr`.
    pub fn set(&mut self, path: LogicalPath, addr: ContentAddr) {
        self.entries.insert(path, addr);
    }

    /// Remove a symlink.
    pub fn remove(&mut self, path: &LogicalPath) -> Option<ContentAddr> {
        self.entries.remove(path)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = (&LogicalPath, &ContentAddr)> {
        self.entries.iter()
    }

    /// All logical paths in the index.
    pub fn paths(&self) -> impl Iterator<Item = &LogicalPath> {
        self.entries.keys()
    }

    /// Compute the delta from `base` to `self`.
    ///
    /// - Paths in `self` but not in `base` → `Add`
    /// - Paths in both but different content → `Modify`
    /// - Paths in `base` but not in `self` → `Delete`
    pub fn diff(&self, base: &SymlinkIndex) -> IndexDelta {
        let mut ops = BTreeMap::new();

        // Paths in self: Add (if new) or Modify (if changed)
        for (path, addr) in &self.entries {
            match base.entries.get(path) {
                None => {
                    ops.insert(path.clone(), DeltaOp::Add(*addr));
                }
                Some(base_addr) if base_addr != addr => {
                    ops.insert(path.clone(), DeltaOp::Modify(*addr));
                }
                Some(_) => {} // unchanged
            }
        }

        // Paths in base but not in self → Delete
        for path in base.entries.keys() {
            if !self.entries.contains_key(path) {
                ops.insert(path.clone(), DeltaOp::Delete);
            }
        }

        IndexDelta { ops }
    }

    /// Apply a delta to produce a new index.
    pub fn apply(&self, delta: &IndexDelta) -> SymlinkIndex {
        let mut result = self.clone();
        for (path, op) in &delta.ops {
            match op {
                DeltaOp::Add(addr) | DeltaOp::Modify(addr) => {
                    result.entries.insert(path.clone(), *addr);
                }
                DeltaOp::Delete => {
                    result.entries.remove(path);
                }
            }
        }
        result
    }

    /// Three-way merge of two indexes relative to a common base.
    ///
    /// For each path in the union of all three indexes:
    /// - If only one side changed it → take that side
    /// - If both changed it the same way → take either (identical)
    /// - If both changed it differently → conflict
    /// - If one deleted and the other modified → conflict
    ///
    /// Returns the merged index and any conflicts. Both versions of a
    /// conflicted path are always available in the content store.
    pub fn three_way_merge(
        base: &SymlinkIndex,
        ours: &SymlinkIndex,
        theirs: &SymlinkIndex,
    ) -> (SymlinkIndex, Vec<Conflict>) {
        let mut merged = BTreeMap::new();
        let mut conflicts = Vec::new();

        // Collect all paths from all three indexes.
        let all_paths: BTreeSet<&LogicalPath> = base
            .entries
            .keys()
            .chain(ours.entries.keys())
            .chain(theirs.entries.keys())
            .collect();

        for path in all_paths {
            let b = base.entries.get(path);
            let o = ours.entries.get(path);
            let t = theirs.entries.get(path);

            match (b, o, t) {
                // Both sides unchanged from base (or path only in base and
                // both deleted it).
                (Some(bv), Some(ov), Some(tv)) if ov == bv && tv == bv => {
                    merged.insert(path.clone(), *bv);
                }

                // Only ours changed (theirs == base or theirs absent and base absent).
                (bv, Some(ov), tv) if tv == bv => {
                    merged.insert(path.clone(), *ov);
                }

                // Only theirs changed (ours == base or ours absent and base absent).
                (bv, ov, Some(tv)) if ov == bv => {
                    merged.insert(path.clone(), *tv);
                }

                // Both changed the same way.
                (_, Some(ov), Some(tv)) if ov == tv => {
                    merged.insert(path.clone(), *ov);
                }

                // Both deleted.
                (Some(_), None, None) => {
                    // Path deleted by both — omit from merged.
                }

                // Path only in base, not in ours or theirs — already deleted.
                (_, None, None) => {}

                // Conflict: both sides changed differently.
                (_, Some(ov), Some(tv)) => {
                    // Default to theirs in the merged index (LWW-style),
                    // but record the conflict so the caller can override.
                    merged.insert(path.clone(), *tv);
                    conflicts.push(Conflict {
                        path: path.clone(),
                        ours: *ov,
                        theirs: *tv,
                        base: b.copied(),
                    });
                }

                // Conflict: one side deleted, other modified.
                (Some(_bv), None, Some(tv)) => {
                    // We deleted, they modified — take theirs, flag conflict.
                    merged.insert(path.clone(), *tv);
                    conflicts.push(Conflict {
                        path: path.clone(),
                        ours: ContentAddr::new([0; 32], 0), // sentinel: deleted
                        theirs: *tv,
                        base: b.copied(),
                    });
                }
                (Some(_bv), Some(ov), None) => {
                    // They deleted, we modified — keep ours, flag conflict.
                    merged.insert(path.clone(), *ov);
                    conflicts.push(Conflict {
                        path: path.clone(),
                        ours: *ov,
                        theirs: ContentAddr::new([0; 32], 0), // sentinel: deleted
                        base: b.copied(),
                    });
                }

                // New path only in ours.
                (None, Some(ov), None) => {
                    merged.insert(path.clone(), *ov);
                }

                // New path only in theirs.
                (None, None, Some(tv)) => {
                    merged.insert(path.clone(), *tv);
                }
            }
        }

        (SymlinkIndex { entries: merged }, conflicts)
    }
}

// ── IndexDelta / DeltaOp ───────────────────────────────────────────────

/// What changed in a single symlink.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeltaOp {
    /// Path was added (not present in parent).
    Add(ContentAddr),
    /// Path existed in parent but now points to a different address.
    Modify(ContentAddr),
    /// Path existed in parent but was removed.
    Delete,
}

impl DeltaOp {
    /// The new content address, if any (None for Delete).
    pub fn addr(&self) -> Option<&ContentAddr> {
        match self {
            DeltaOp::Add(a) | DeltaOp::Modify(a) => Some(a),
            DeltaOp::Delete => None,
        }
    }
}

/// The diff between a changeset's index and its first parent's index.
///
/// For root changesets (no parents), every entry is an `Add`.
/// Delta is NOT included in the `ChangeId` hash — it's derived metadata,
/// recomputable from `my_index` and `parent[0].index`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDelta {
    /// Path → operation.
    pub ops: BTreeMap<LogicalPath, DeltaOp>,
}

impl IndexDelta {
    /// Create an empty delta.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of changed paths.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Iterate over all operations.
    pub fn iter(&self) -> impl Iterator<Item = (&LogicalPath, &DeltaOp)> {
        self.ops.iter()
    }

    /// Paths that were added or modified (not deleted).
    pub fn updated_addrs(&self) -> impl Iterator<Item = (&LogicalPath, &ContentAddr)> {
        self.ops.iter().filter_map(|(p, op)| op.addr().map(|a| (p, a)))
    }

    /// Paths that were deleted.
    pub fn deleted_paths(&self) -> impl Iterator<Item = &LogicalPath> {
        self.ops
            .iter()
            .filter_map(|(p, op)| matches!(op, DeltaOp::Delete).then_some(p))
    }

    /// Build a "root delta" — every entry in `index` is an `Add`.
    pub fn from_root(index: &SymlinkIndex) -> Self {
        Self {
            ops: index
                .entries
                .iter()
                .map(|(p, a)| (p.clone(), DeltaOp::Add(*a)))
                .collect(),
        }
    }
}

// ── Conflict ───────────────────────────────────────────────────────────

/// Two participants changed the same symlink to different addresses.
///
/// Both versions persist in the content store — nothing is lost.
/// The `base` field enables future three-way content merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The path where the two sides disagree.
    pub path: LogicalPath,
    /// Our content address (zero-hash sentinel if we deleted).
    pub ours: ContentAddr,
    /// Their content address (zero-hash sentinel if they deleted).
    pub theirs: ContentAddr,
    /// The content address at the LCA, if the path existed there.
    pub base: Option<ContentAddr>,
}

impl Conflict {
    /// Whether our side deleted the path.
    pub fn ours_deleted(&self) -> bool {
        self.ours.hash == [0; 32]
    }

    /// Whether their side deleted the path.
    pub fn theirs_deleted(&self) -> bool {
        self.theirs.hash == [0; 32]
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> ContentAddr {
        ContentAddr::new([byte; 32], byte as u64 * 100)
    }

    fn path(s: &str) -> LogicalPath {
        LogicalPath::new(s)
    }

    fn index(entries: &[(&str, u8)]) -> SymlinkIndex {
        SymlinkIndex::from_iter(
            entries
                .iter()
                .map(|(p, b)| (path(p), addr(*b))),
        )
    }

    // ── LogicalPath ────────────────────────────────────────

    #[test]
    fn logical_path_ord() {
        let a = path("a.rs");
        let b = path("b.rs");
        assert!(a < b);
    }

    #[test]
    fn logical_path_display() {
        let p = path("src/lib.rs");
        assert_eq!(format!("{p}"), "src/lib.rs");
    }

    // ── ContentAddr ────────────────────────────────────────

    #[test]
    fn content_addr_from_bytes() {
        let a = ContentAddr::from_bytes(b"hello");
        let b = ContentAddr::from_bytes(b"hello");
        assert_eq!(a, b);

        let c = ContentAddr::from_bytes(b"world");
        assert_ne!(a, c);
    }

    #[test]
    fn content_addr_content_ref_roundtrip() {
        let ca = addr(42);
        let cr: ContentRef = ca.into();
        let ca2: ContentAddr = cr.into();
        assert_eq!(ca, ca2);
    }

    // ── SymlinkIndex ───────────────────────────────────────

    #[test]
    fn index_basic_operations() {
        let mut idx = SymlinkIndex::new();
        assert!(idx.is_empty());

        idx.set(path("a.rs"), addr(1));
        assert_eq!(idx.len(), 1);
        assert_eq!(idx.get(&path("a.rs")), Some(&addr(1)));

        idx.remove(&path("a.rs"));
        assert!(idx.is_empty());
    }

    // ── Diff ───────────────────────────────────────────────

    #[test]
    fn diff_empty_to_populated() {
        let base = SymlinkIndex::new();
        let current = index(&[("a.rs", 1), ("b.rs", 2)]);

        let delta = current.diff(&base);
        assert_eq!(delta.len(), 2);
        assert_eq!(delta.ops[&path("a.rs")], DeltaOp::Add(addr(1)));
        assert_eq!(delta.ops[&path("b.rs")], DeltaOp::Add(addr(2)));
    }

    #[test]
    fn diff_populated_to_empty() {
        let base = index(&[("a.rs", 1), ("b.rs", 2)]);
        let current = SymlinkIndex::new();

        let delta = current.diff(&base);
        assert_eq!(delta.len(), 2);
        assert_eq!(delta.ops[&path("a.rs")], DeltaOp::Delete);
        assert_eq!(delta.ops[&path("b.rs")], DeltaOp::Delete);
    }

    #[test]
    fn diff_modification() {
        let base = index(&[("a.rs", 1), ("b.rs", 2)]);
        let current = index(&[("a.rs", 1), ("b.rs", 3)]); // b.rs changed

        let delta = current.diff(&base);
        assert_eq!(delta.len(), 1);
        assert_eq!(delta.ops[&path("b.rs")], DeltaOp::Modify(addr(3)));
    }

    #[test]
    fn diff_mixed() {
        let base = index(&[("a.rs", 1), ("b.rs", 2), ("c.rs", 3)]);
        let current = index(&[("a.rs", 1), ("b.rs", 4), ("d.rs", 5)]);
        // a.rs unchanged, b.rs modified, c.rs deleted, d.rs added

        let delta = current.diff(&base);
        assert_eq!(delta.len(), 3);
        assert_eq!(delta.ops[&path("b.rs")], DeltaOp::Modify(addr(4)));
        assert_eq!(delta.ops[&path("c.rs")], DeltaOp::Delete);
        assert_eq!(delta.ops[&path("d.rs")], DeltaOp::Add(addr(5)));
    }

    #[test]
    fn diff_identical_indexes_is_empty() {
        let idx = index(&[("a.rs", 1), ("b.rs", 2)]);
        let delta = idx.diff(&idx);
        assert!(delta.is_empty());
    }

    // ── Apply ──────────────────────────────────────────────

    #[test]
    fn apply_roundtrips_with_diff() {
        let base = index(&[("a.rs", 1), ("b.rs", 2), ("c.rs", 3)]);
        let target = index(&[("a.rs", 1), ("b.rs", 4), ("d.rs", 5)]);

        let delta = target.diff(&base);
        let result = base.apply(&delta);
        assert_eq!(result, target);
    }

    #[test]
    fn apply_empty_delta_is_identity() {
        let idx = index(&[("a.rs", 1)]);
        let result = idx.apply(&IndexDelta::new());
        assert_eq!(result, idx);
    }

    // ── Three-way merge ────────────────────────────────────

    #[test]
    fn merge_no_conflicts() {
        let base = index(&[("a.rs", 1), ("b.rs", 2)]);
        let ours = index(&[("a.rs", 1), ("b.rs", 2), ("c.rs", 3)]); // added c
        let theirs = index(&[("a.rs", 4), ("b.rs", 2)]); // modified a

        let (merged, conflicts) = SymlinkIndex::three_way_merge(&base, &ours, &theirs);
        assert!(conflicts.is_empty());
        assert_eq!(merged.get(&path("a.rs")), Some(&addr(4))); // theirs
        assert_eq!(merged.get(&path("b.rs")), Some(&addr(2))); // unchanged
        assert_eq!(merged.get(&path("c.rs")), Some(&addr(3))); // ours
    }

    #[test]
    fn merge_both_add_same_path_same_content() {
        let base = SymlinkIndex::new();
        let ours = index(&[("a.rs", 1)]);
        let theirs = index(&[("a.rs", 1)]);

        let (merged, conflicts) = SymlinkIndex::three_way_merge(&base, &ours, &theirs);
        assert!(conflicts.is_empty());
        assert_eq!(merged.get(&path("a.rs")), Some(&addr(1)));
    }

    #[test]
    fn merge_conflict_both_modify_differently() {
        let base = index(&[("a.rs", 1)]);
        let ours = index(&[("a.rs", 2)]);
        let theirs = index(&[("a.rs", 3)]);

        let (merged, conflicts) = SymlinkIndex::three_way_merge(&base, &ours, &theirs);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, path("a.rs"));
        assert_eq!(conflicts[0].ours, addr(2));
        assert_eq!(conflicts[0].theirs, addr(3));
        assert_eq!(conflicts[0].base, Some(addr(1)));
        // Default resolution: theirs wins
        assert_eq!(merged.get(&path("a.rs")), Some(&addr(3)));
    }

    #[test]
    fn merge_conflict_delete_vs_modify() {
        let base = index(&[("a.rs", 1)]);
        let ours = SymlinkIndex::new(); // deleted a.rs
        let theirs = index(&[("a.rs", 2)]); // modified a.rs

        let (merged, conflicts) = SymlinkIndex::three_way_merge(&base, &ours, &theirs);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].ours_deleted());
        assert_eq!(conflicts[0].theirs, addr(2));
        // Default: theirs wins (modification preserved)
        assert_eq!(merged.get(&path("a.rs")), Some(&addr(2)));
    }

    #[test]
    fn merge_both_delete() {
        let base = index(&[("a.rs", 1), ("b.rs", 2)]);
        let ours = index(&[("b.rs", 2)]); // deleted a.rs
        let theirs = index(&[("b.rs", 2)]); // also deleted a.rs

        let (merged, conflicts) = SymlinkIndex::three_way_merge(&base, &ours, &theirs);
        assert!(conflicts.is_empty());
        assert!(merged.get(&path("a.rs")).is_none());
        assert_eq!(merged.get(&path("b.rs")), Some(&addr(2)));
    }

    #[test]
    fn merge_disjoint_additions() {
        let base = SymlinkIndex::new();
        let ours = index(&[("a.rs", 1)]);
        let theirs = index(&[("b.rs", 2)]);

        let (merged, conflicts) = SymlinkIndex::three_way_merge(&base, &ours, &theirs);
        assert!(conflicts.is_empty());
        assert_eq!(merged.len(), 2);
        assert_eq!(merged.get(&path("a.rs")), Some(&addr(1)));
        assert_eq!(merged.get(&path("b.rs")), Some(&addr(2)));
    }

    // ── IndexDelta helpers ─────────────────────────────────

    #[test]
    fn from_root_makes_all_adds() {
        let idx = index(&[("a.rs", 1), ("b.rs", 2)]);
        let delta = IndexDelta::from_root(&idx);
        assert_eq!(delta.len(), 2);
        assert!(matches!(delta.ops[&path("a.rs")], DeltaOp::Add(_)));
        assert!(matches!(delta.ops[&path("b.rs")], DeltaOp::Add(_)));
    }

    #[test]
    fn updated_addrs_skips_deletes() {
        let base = index(&[("a.rs", 1), ("b.rs", 2)]);
        let current = index(&[("a.rs", 3)]); // b deleted, a modified
        let delta = current.diff(&base);

        let updated: Vec<_> = delta.updated_addrs().collect();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].0, &path("a.rs"));
    }

    #[test]
    fn deleted_paths_skips_adds_and_modifies() {
        let base = index(&[("a.rs", 1), ("b.rs", 2)]);
        let current = index(&[("a.rs", 3)]); // b deleted, a modified
        let delta = current.diff(&base);

        let deleted: Vec<_> = delta.deleted_paths().collect();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0], &path("b.rs"));
    }

    // ── Conflict helpers ───────────────────────────────────

    #[test]
    fn conflict_deleted_sentinels() {
        let c = Conflict {
            path: path("x.rs"),
            ours: ContentAddr::new([0; 32], 0),
            theirs: addr(1),
            base: Some(addr(2)),
        };
        assert!(c.ours_deleted());
        assert!(!c.theirs_deleted());
    }
}
