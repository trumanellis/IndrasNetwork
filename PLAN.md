# Plan: Consolidate Holonic Composition into Tree Paradigm

## Context

The codebase has two parallel hierarchy models describing the same concept — parent/child artifact relationships:

1. **Artifact layer** (`indras-artifacts`): `TreeArtifact.references` (ordered, labeled children) + `Artifact.parent`
2. **Index layer** (`indras-network`): `HomeArtifactEntry.parent` + `HomeArtifactEntry.children`

These can drift out of sync because nothing connects them. The "holonic" vocabulary adds cognitive overhead — the operations (`ancestors`, `children_of`, `attach_child`) are all tree operations. This change unifies under tree terminology and removes the redundant `children` field from `HomeArtifactEntry`.

## Changes

### Step 1: Rename `HolonicError` to `TreeError`

**File:** `crates/indras-network/src/access.rs`

- Rename `HolonicError` enum to `TreeError`
- Update `Display` impl and doc comments
- Update `impl std::error::Error`

### Step 2: Update re-exports

**File:** `crates/indras-network/src/lib.rs`

- Change `HolonicError` to `TreeError` in the re-export

### Step 3: Rename methods and remove `children` field from `HomeArtifactEntry`

**File:** `crates/indras-network/src/artifact_index.rs`

Terminology renames:
- `compose()` → `attach_children()`
- `decompose()` → `detach_all_children()`
- `holon_size()` → `subtree_size()`
- All `HolonicError` references → `TreeError`
- Section header "Holonic composition operations" → "Tree composition operations"
- All doc comments: "holon" → "tree"/"subtree", "holonic" → "tree"

Structural change — remove `children: Vec<ArtifactId>` from `HomeArtifactEntry`:
- Remove the `children` field from the struct
- Remove `#[serde(default)]` on `children`
- Update doc comment on `parent` from "part of" to "child of (None if root)"

Update `children_of()` to derive children by scanning:
```rust
pub fn children_of(&self, id: &ArtifactId) -> Vec<&HomeArtifactEntry> {
    self.artifacts.values()
        .filter(|e| e.parent.as_ref() == Some(id))
        .collect()
}
```

Update `attach_children()` (was `compose`):
- Remove the block that pushes to `parent.children`
- Keep: validation, cycle detection, setting `child.parent`

Update `detach_all_children()` (was `decompose`):
- Instead of reading `parent.children.clone()`, scan for children: `self.artifacts.values().filter(|e| e.parent == Some(*parent_id)).map(|e| e.id).collect()`
- Remove `parent.children.clear()`
- Keep: grant materialization on detach

Update `attach_child()`:
- Remove the block that pushes to `parent.children`
- Keep: validation, cycle detection, setting `child.parent`

Update `detach_child()`:
- Remove `parent.children.contains(child_id)` check — instead check `child.parent == Some(*parent_id)`
- Remove `parent.children.retain(...)`
- Keep: grant materialization on detach

Update `collect_descendants()`:
- Instead of iterating `entry.children`, scan for children with matching parent

Update `is_leaf()`:
- Instead of checking `e.children.is_empty()`, scan: `!self.artifacts.values().any(|e| e.parent.as_ref() == Some(id))`

Update `transfer()`:
- Remove `children: entry.children.clone()` from the new entry construction

Update `test_entry()` and `make_entry()` helpers:
- Remove `children: Vec::new()` field

Update all tests:
- Remove assertions on `.children` field (e.g., `parent_entry.children.len()`)
- Replace with `children_of()` assertions
- Rename `test_compose_*` → `test_attach_children_*`
- Rename `test_decompose_*` → `test_detach_all_children_*`
- Rename `test_holon_size` → `test_subtree_size`
- Update holonic section header in tests

### Step 4: Update `HomeRealm` public API

**File:** `crates/indras-network/src/home_realm.rs`

- `compose_artifact()` → `attach_children()`
- `decompose_artifact()` → `detach_all_children()`
- Section header "Tree composition" → "Tree composition"
- Doc comments: "holon" → "tree"/"subtree"
- Update internal calls to match renamed `ArtifactIndex` methods
- Update `HolonicError` references → `TreeError`

### Step 5: Update `Vault` section headers and comments

**File:** `crates/indras-artifacts/src/vault.rs`

- Section header "Holonic composition" → "Tree composition"
- Doc comments referencing holonic → tree
- `VaultError::CycleDetected`, `AlreadyHasParent`, `NotAChild` — these are already tree-neutral names, keep as-is

### Step 6: Update PLAN.md reference

**File:** `PLAN.md`

- Change "holonic composition" to "tree composition" (line 48)

### Step 7: Update test file

**File:** `crates/indras-network/tests/artifact_access.rs`

- Update any `HolonicError` references to `TreeError`
- Update any holonic terminology in test names/comments

## Files Modified (summary)

| File | Change Type |
|------|-------------|
| `crates/indras-network/src/access.rs` | Rename `HolonicError` → `TreeError` |
| `crates/indras-network/src/lib.rs` | Update re-export |
| `crates/indras-network/src/artifact_index.rs` | Remove `children` field, rename methods, update implementations |
| `crates/indras-network/src/home_realm.rs` | Rename public API methods |
| `crates/indras-artifacts/src/vault.rs` | Update section headers/comments |
| `PLAN.md` | Terminology update |
| `crates/indras-network/tests/artifact_access.rs` | Update error type references |

## Verification

```bash
# Build both affected crates
cargo build -p indras-artifacts -p indras-network

# Run all tests in affected crates
cargo test -p indras-artifacts -p indras-network

# Grep to confirm no remaining holonic references
rg -i "holon" --type rust
```

## Notes

- `parent: Option<ArtifactId>` stays on `HomeArtifactEntry` — it's needed for ancestor walks (access inheritance). This isn't redundancy with `Artifact.parent`; they're the same fact at two architectural layers (artifact data vs. home index).
- `children_of()` becomes an O(n) scan instead of O(1) field lookup. This is fine — n is a single user's artifact count, not a global dataset.
- The `ArtifactRef` struct (with `position` and `label`) on `TreeArtifact.references` remains the richer, ordered child representation at the artifact layer. The index layer only needs to know parent pointers.
