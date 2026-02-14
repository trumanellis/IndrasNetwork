# Migration Plan: Automerge → Yrs

## Overview

Replace Automerge 0.7 with Yrs (Rust Yjs implementation) as the CRDT backbone for IndrasNetwork's P2P sync layer. This enables native compatibility with the Yjs ecosystem, including AppFlowy, AFFiNE, BlockSuite, and any Yjs-based collaborative application.

**Branch:** `feature/yrs-migration`
**Estimated scope:** ~10 files changed, ~1 file rewritten, 0 files deleted

---

## Table of Contents

1. [Architecture Impact](#1-architecture-impact)
2. [Dependency Changes](#2-dependency-changes)
3. [Phase 1: Core Document Rewrite](#3-phase-1-core-document-rewrite)
4. [Phase 2: Sync Protocol Migration](#4-phase-2-sync-protocol-migration)
5. [Phase 3: Storage Layer Updates](#5-phase-3-storage-layer-updates)
6. [Phase 4: Example and Application Updates](#6-phase-4-example-and-application-updates)
7. [Phase 5: Testing and Validation](#7-phase-5-testing-and-validation)
8. [Phase 6: Awareness Protocol (New Feature)](#8-phase-6-awareness-protocol)
9. [API Translation Reference](#9-api-translation-reference)
10. [Risk Register](#10-risk-register)

---

## 1. Architecture Impact

### What stays the same

The architecture is remarkably well-isolated. All direct Automerge API calls live in **one file**: `crates/indras-sync/src/document.rs`. Everything above uses the `InterfaceDocument` abstraction.

```
┌─────────────────────────────────────────────────────┐
│  Application Layer (indras-node, examples, sim)     │  ← No Automerge imports
│  Uses: NInterface, InterfaceDocument methods         │
├─────────────────────────────────────────────────────┤
│  Network Layer (indras-network)                      │  ← No Automerge imports
│  Uses: Document<T>, Realm, postcard events           │
├─────────────────────────────────────────────────────┤
│  Sync Protocol (indras-sync/sync_protocol.rs)        │  ← ChangeHash → StateVector
│  Uses: PeerSyncState, head tracking                  │
├─────────────────────────────────────────────────────┤
│  Core Document (indras-sync/document.rs)             │  ← REWRITE THIS FILE
│  Uses: AutoCommit → Doc, ObjId → MapRef/ArrayRef     │
├─────────────────────────────────────────────────────┤
│  Wire Protocol (indras-core/traits.rs)               │  ← SyncMessage.heads type change
│  Uses: SyncMessage { heads: Vec<[u8;32]> }           │
└─────────────────────────────────────────────────────┘
```

### Files by migration tier

| Tier | File | Change Type | Effort |
|------|------|------------|--------|
| **T1: Rewrite** | `crates/indras-sync/src/document.rs` | Full rewrite (~360 lines) | High |
| **T2: Type change** | `crates/indras-sync/src/sync_protocol.rs` | `ChangeHash` → `StateVector` | Medium |
| **T2: Type change** | `crates/indras-core/src/traits.rs` | `SyncMessage.heads` field type | Low |
| **T3: Dependency** | `Cargo.toml` (root) | Swap workspace dep | Trivial |
| **T3: Dependency** | `crates/indras-sync/Cargo.toml` | Swap crate dep | Trivial |
| **T3: Dependency** | `examples/indras-notes/Cargo.toml` | Swap crate dep | Trivial |
| **T3: Dependency** | `examples/sync-demo/Cargo.toml` | Swap crate dep | Trivial |
| **T4: Consumers** | `examples/indras-notes/src/syncable_notebook.rs` | Replace `ChangeHash` usage | Low |
| **T4: Consumers** | `examples/sync-demo/src/document.rs` | Replace `ChangeHash` usage | Low |
| **T5: String only** | `crates/indras-storage/src/blobs/content_ref.rs` | `"automerge/snapshot"` → `"yrs/snapshot"` | Trivial |

### Files that should NOT change

These use only the `InterfaceDocument` / `NInterface` abstraction and should compile without modification if we preserve the public API:

- `crates/indras-sync/src/n_interface.rs`
- `crates/indras-sync/src/event_store.rs`
- `crates/indras-node/src/message_handler.rs`
- `crates/indras-node/src/sync_task.rs`
- `crates/indras-node/src/lib.rs`
- `crates/indras-network/src/document.rs`
- `crates/indras-network/src/escape.rs`
- `simulation/src/integration_scenarios.rs`

---

## 2. Dependency Changes

### Root `Cargo.toml`

```toml
# Remove
automerge = "0.7"

# Add
yrs = "0.21"
y-sync = "0.4"
```

> **Note:** Use yrs 0.21 (not 0.25) initially — this is the version AppFlowy-Collab pins to. Matching versions avoids encoding incompatibilities. Upgrade after integration is proven.

### Crate-level `Cargo.toml` files

For each crate that currently has `automerge.workspace = true`:

```toml
# Remove
automerge.workspace = true

# Add
yrs.workspace = true
```

Affected crates:
- `crates/indras-sync/Cargo.toml`
- `examples/indras-notes/Cargo.toml`
- `examples/sync-demo/Cargo.toml`
- `simulation/Cargo.toml` (if it has a direct dep)

---

## 3. Phase 1: Core Document Rewrite

**File:** `crates/indras-sync/src/document.rs`
**Goal:** Replace `InterfaceDocument` internals while preserving public API signatures where possible.

### Current struct

```rust
pub struct InterfaceDocument {
    doc: AutoCommit,
    members_id: ObjId,
    metadata_id: ObjId,
    events_id: ObjId,
}
```

### New struct

```rust
use yrs::{Doc, Map, MapRef, Array, ArrayRef, Transact, ReadTxn, TransactionMut};
use yrs::updates::encoder::v1::encode_state_as_update;
use yrs::updates::decoder::v1::decode_update;

pub struct InterfaceDocument {
    doc: Doc,
    // No ObjId caching needed — Yrs resolves shared types by name
}
```

### Key differences

| Automerge pattern | Yrs pattern |
|-------------------|-------------|
| `AutoCommit::new()` + `put_object(ROOT, "members", Map)` | `Doc::new()` — shared types created on first access via `doc.get_or_insert_map("members")` |
| Cache `ObjId` at construction | Get `MapRef`/`ArrayRef` from `doc` on each access (cheap, no lookup) |
| `refresh_object_ids()` after sync | **Remove entirely** — Yrs does not have this concept |
| `doc.put(obj_id, key, value)` | `let map: MapRef = doc.get_or_insert_map("members"); map.insert(&mut txn, key, value);` |
| `doc.get(obj_id, key)` | `map.get(&txn, key)` |
| `doc.insert(list_id, idx, value)` | `array.insert(&mut txn, idx, value)` |
| `doc.save()` → `Vec<u8>` | `txn.encode_state_as_update_v1(&StateVector::default())` |
| `doc.load_incremental(bytes)` | `txn.apply_update(Update::decode_v1(bytes)?)` |
| `doc.get_heads()` → `Vec<ChangeHash>` | `txn.state_vector()` → `StateVector` |
| `doc.save_after(heads)` | `txn.encode_state_as_update_v1(&their_state_vector)` |
| `doc.fork()` | Encode full state, decode into new `Doc` |
| `doc.merge(other)` | Encode other as update, apply to self |

### Constructor: `InterfaceDocument::new()`

```rust
impl InterfaceDocument {
    pub fn new() -> Self {
        let doc = Doc::new();
        // Pre-initialize shared types so they exist for sync
        {
            let mut txn = doc.transact_mut();
            // These create the shared types if they don't exist
            txn.get_or_insert_map("members");
            txn.get_or_insert_map("metadata");
            txn.get_or_insert_array("events");
        }
        Self { doc }
    }
}
```

### Load from bytes: `InterfaceDocument::load()`

```rust
pub fn load(bytes: &[u8]) -> Result<Self, SyncError> {
    let doc = Doc::new();
    {
        let mut txn = doc.transact_mut();
        let update = Update::decode_v1(bytes)
            .map_err(|e| SyncError::DeserializationFailed(e.to_string()))?;
        txn.apply_update(update)?;
    }
    Ok(Self { doc })
}
```

### Member operations

```rust
pub fn add_member(&mut self, peer_id: &[u8; 32]) -> Result<(), SyncError> {
    let mut txn = self.doc.transact_mut();
    let members = txn.get_or_insert_map("members");
    members.insert(&mut txn, hex::encode(peer_id), true);
    Ok(())
}

pub fn remove_member(&mut self, peer_id: &[u8; 32]) -> Result<(), SyncError> {
    let mut txn = self.doc.transact_mut();
    let members = txn.get_or_insert_map("members");
    members.remove(&mut txn, &hex::encode(peer_id));
    Ok(())
}

pub fn members(&self) -> Result<Vec<[u8; 32]>, SyncError> {
    let txn = self.doc.transact();
    let members = txn.get_or_insert_map("members");
    members.iter(&txn)
        .filter_map(|(key, _)| {
            let bytes = hex::decode(&key).ok()?;
            let arr: [u8; 32] = bytes.try_into().ok()?;
            Some(arr)
        })
        .collect()
}
```

### Metadata operations

```rust
pub fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), SyncError> {
    let mut txn = self.doc.transact_mut();
    let metadata = txn.get_or_insert_map("metadata");
    metadata.insert(&mut txn, key, value);
    Ok(())
}

pub fn get_metadata(&self, key: &str) -> Result<Option<String>, SyncError> {
    let txn = self.doc.transact();
    let metadata = txn.get_or_insert_map("metadata");
    Ok(metadata.get(&txn, key).map(|v| v.to_string(&txn)))
}
```

### Event operations

```rust
pub fn append_event(&mut self, event_bytes: &[u8]) -> Result<(), SyncError> {
    let mut txn = self.doc.transact_mut();
    let events = txn.get_or_insert_array("events");
    events.push_back(&mut txn, event_bytes.to_vec());
    Ok(())
}

pub fn events(&self) -> Result<Vec<Vec<u8>>, SyncError> {
    let txn = self.doc.transact();
    let events = txn.get_or_insert_array("events");
    let mut result = Vec::new();
    for value in events.iter(&txn) {
        if let yrs::Any::Buffer(buf) = value {
            result.push(buf.to_vec());
        }
    }
    Ok(result)
}

pub fn events_since(&self, index: u32) -> Result<Vec<Vec<u8>>, SyncError> {
    let txn = self.doc.transact();
    let events = txn.get_or_insert_array("events");
    let len = events.len(&txn);
    let mut result = Vec::new();
    for i in index..len {
        if let Some(value) = events.get(&txn, i) {
            if let yrs::Any::Buffer(buf) = value {
                result.push(buf.to_vec());
            }
        }
    }
    Ok(result)
}
```

### Serialization

```rust
pub fn save(&self) -> Vec<u8> {
    let txn = self.doc.transact();
    txn.encode_state_as_update_v1(&StateVector::default())
}

pub fn state_vector(&self) -> StateVector {
    let txn = self.doc.transact();
    txn.state_vector()
}

pub fn encode_state_vector(&self) -> Vec<u8> {
    self.state_vector().encode_v1()
}

pub fn generate_sync_message(&self, their_state_vector: &[u8]) -> Result<Vec<u8>, SyncError> {
    let sv = StateVector::decode_v1(their_state_vector)
        .map_err(|e| SyncError::DeserializationFailed(e.to_string()))?;
    let txn = self.doc.transact();
    Ok(txn.encode_state_as_update_v1(&sv))
}

pub fn apply_sync_message(&mut self, update_bytes: &[u8]) -> Result<(), SyncError> {
    let mut txn = self.doc.transact_mut();
    let update = Update::decode_v1(update_bytes)
        .map_err(|e| SyncError::DeserializationFailed(e.to_string()))?;
    txn.apply_update(update)?;
    Ok(())
}
```

### Fork and merge

```rust
pub fn fork(&self) -> Result<Self, SyncError> {
    let bytes = self.save();
    Self::load(&bytes)
}

pub fn merge(&mut self, other: &InterfaceDocument) -> Result<(), SyncError> {
    let update_bytes = other.save();
    self.apply_sync_message(&update_bytes)
}
```

### Removed methods

- `refresh_object_ids()` — **Delete entirely.** Yrs shared types are resolved by name, not by mutable object IDs. There is no equivalent and no need for one.

### RwLock consideration

The current `NInterface` wraps `InterfaceDocument` in `RwLock<InterfaceDocument>`. Yrs `Doc` uses interior mutability via transactions:
- `doc.transact()` → shared read (like `RwLock::read()`)
- `doc.transact_mut()` → exclusive write (like `RwLock::write()`)

**Recommendation:** Keep the `RwLock` for now. It provides a familiar synchronization boundary and the overhead is negligible. Removing it is a future optimization.

---

## 4. Phase 2: Sync Protocol Migration

**File:** `crates/indras-sync/src/sync_protocol.rs`

### Current: ChangeHash-based head tracking

```rust
pub struct PeerSyncState {
    pub their_heads: Vec<ChangeHash>,  // 32-byte hashes
    pub awaiting_response: bool,
    pub rounds: u32,
}
```

### New: StateVector-based tracking

```rust
pub struct PeerSyncState {
    pub their_state_vector: Vec<u8>,   // Serialized Yrs StateVector
    pub awaiting_response: bool,
    pub rounds: u32,
}
```

### Sync flow change

**Before (Automerge):**
```
A → B: save_after(B's heads) + A's heads
B → A: save_after(A's heads) + B's heads
Compare heads for convergence
```

**After (Yrs):**
```
A → B: A's state_vector (encoded)
B → A: encode_state_as_update_v1(A's state_vector) + B's state_vector
A → B: encode_state_as_update_v1(B's state_vector)
Done (2 rounds max for Yrs vs N rounds for Automerge)
```

Yrs sync is simpler — state vectors describe exactly what each peer has, so the diff is computed in one step. No iterative head comparison needed.

### Wire protocol change

**File:** `crates/indras-core/src/traits.rs`

```rust
// Before
pub struct SyncMessage {
    pub sync_data: Vec<u8>,
    pub heads: Vec<[u8; 32]>,  // Automerge ChangeHash array
}

// After
pub struct SyncMessage {
    pub sync_data: Vec<u8>,          // Yrs Update (encoded)
    pub state_vector: Vec<u8>,       // Yrs StateVector (encoded)
}
```

**This is a breaking wire protocol change.** Old peers cannot sync with new peers. Since this is greenfield, that's acceptable.

### Convergence check

```rust
// Before: compare head hash vectors
fn is_sync_complete(our_heads: &[ChangeHash], their_heads: &[ChangeHash]) -> bool {
    our_heads == their_heads
}

// After: compare state vectors
fn is_sync_complete(our_sv: &StateVector, their_sv: &StateVector) -> bool {
    // After applying their update, our SV should include their state
    // and vice versa — or simply check if the update is empty
    our_sv == their_sv
}
```

In practice, Yrs sync can use a simpler check: if `encode_state_as_update_v1(their_sv)` produces an empty update, we're in sync.

---

## 5. Phase 3: Storage Layer Updates

**File:** `crates/indras-storage/src/blobs/content_ref.rs`

Single string change:
```rust
// Before
"automerge/snapshot"

// After
"yrs/update_v1"
```

### Storage format

Yrs documents should be stored as full-state updates:
```rust
let bytes = txn.encode_state_as_update_v1(&StateVector::default());
storage.store("yrs/update_v1", &bytes)?;
```

Loading:
```rust
let bytes = storage.load(content_ref)?;
let update = Update::decode_v1(&bytes)?;
txn.apply_update(update)?;
```

---

## 6. Phase 4: Example and Application Updates

### `examples/indras-notes/src/syncable_notebook.rs`

Replace `ChangeHash` usage with state vector bytes:

```rust
// Before
use automerge::ChangeHash;
fn sync_state(&self) -> Vec<ChangeHash> { ... }

// After
fn sync_state(&self) -> Vec<u8> {
    self.interface_doc.encode_state_vector()
}
```

### `examples/sync-demo/src/document.rs`

Same pattern — replace `ChangeHash` with serialized state vector.

### Lua bindings

`examples/indras-notes/src/lua/bindings/syncable_notebook.rs` uses `InterfaceDocument` methods only — should work without changes if method signatures are preserved. Verify hex encoding of state info still makes sense for Lua consumers.

---

## 7. Phase 5: Testing and Validation

### Critical tests to write/update

1. **Basic document operations**
   - Create document, add members, set metadata, append events
   - Serialize and deserialize round-trip
   - Verify all data survives save/load cycle

2. **Two-peer sync convergence**
   - Peer Zephyr and Peer Nova make independent edits
   - Exchange state vectors and updates
   - Verify both documents converge to identical state

3. **Concurrent event append ordering**
   - Both peers append events simultaneously
   - Verify events list contains all events from both peers
   - **Note:** Yrs may order concurrent array insertions differently than Automerge. Document the actual behavior rather than asserting a specific order.

4. **Offline peer catch-up**
   - Peer Sage goes offline, Peer Orion makes changes
   - Sage comes back, exchanges state vectors
   - Verify Sage receives exactly the missed changes

5. **Three-peer mesh sync**
   - Peers Lyra, Kai, and Ember in a triangle
   - Each makes independent edits
   - Sync in arbitrary order
   - Verify all three converge

6. **Fork and merge**
   - Fork a document, make changes to both
   - Merge back, verify no data loss

7. **Large event history**
   - Append 10,000 events
   - Verify sync performance (state vector exchange should be O(1), not O(n))

### Test commands

```bash
# Core sync tests
cargo test -p indras-sync

# Integration tests
cargo test -p indras-node

# Example tests
cargo test -p indras-notes
cargo test -p sync-demo

# Full workspace
cargo test --workspace
```

### Build verification

```bash
# Must compile cleanly
cargo build --workspace

# No warnings
cargo clippy --workspace
```

---

## 8. Phase 6: Awareness Protocol

**New feature unlocked by Yrs** — not available in Automerge.

Yrs includes an Awareness protocol for ephemeral presence state (cursors, selections, online status). This is separate from document sync.

```rust
use y_sync::awareness::Awareness;

let awareness = Awareness::new(doc);

// Set local state (cursor position, user name, etc.)
awareness.set_local_state(serde_json::to_string(&json!({
    "user": { "name": "Zephyr", "color": "#ff6b6b" },
    "cursor": { "index": 42 }
}))?);

// Listen for remote awareness changes
awareness.on_update(|awareness, event| {
    for client_id in &event.updated {
        if let Some(state) = awareness.get_states().get(client_id) {
            // Update UI with remote cursor/presence
        }
    }
});

// Exchange awareness state with peers
let update = awareness.update()?;
// Send `update` bytes to peer
// Peer applies: awareness.apply_update(update)?;
```

**Defer this to after the core migration.** It's a net-new feature that AppFlowy will use for collaborative cursors, but it's not needed for the Automerge replacement itself.

---

## 9. API Translation Reference

Complete mapping of every Automerge API call found in the codebase:

| Automerge Call | Location | Yrs Equivalent |
|----------------|----------|----------------|
| `AutoCommit::new()` | document.rs:56 | `Doc::new()` |
| `AutoCommit::load(bytes)` | document.rs:81 | `Doc::new()` + `txn.apply_update(Update::decode_v1(bytes))` |
| `doc.put_object(ROOT, key, ObjType::Map)` | document.rs:60,64 | `txn.get_or_insert_map(key)` |
| `doc.put_object(ROOT, key, ObjType::List)` | document.rs:68 | `txn.get_or_insert_array(key)` |
| `doc.get(obj_id, key)` | document.rs:85-136,254,272,325-339 | `map.get(&txn, key)` or `array.get(&txn, idx)` |
| `doc.put(obj_id, key, value)` | document.rs:167,173,205-221 | `map.insert(&mut txn, key, value)` |
| `doc.delete(obj_id, key)` | document.rs:173 | `map.remove(&mut txn, key)` |
| `doc.insert(obj_id, idx, value)` | document.rs:237 | `array.insert(&mut txn, idx, value)` |
| `doc.length(obj_id)` | document.rs:235,251,269 | `array.len(&txn)` |
| `doc.keys(obj_id)` | document.rs:190 | `map.keys(&txn)` / `map.iter(&txn)` |
| `doc.get_heads()` | document.rs:147,152,241 | `txn.state_vector()` |
| `doc.save()` | document.rs:142 | `txn.encode_state_as_update_v1(&StateVector::default())` |
| `doc.save_after(heads)` | document.rs:292 | `txn.encode_state_as_update_v1(&their_state_vector)` |
| `doc.load_incremental(bytes)` | document.rs:298 | `txn.apply_update(Update::decode_v1(bytes))` |
| `doc.fork()` | document.rs:321 | Encode full state → decode into new `Doc` |
| `doc.merge(other)` | document.rs:310 | Encode other → `txn.apply_update(...)` |
| `ScalarValue::Bytes(data)` | document.rs:237,256,274 | `yrs::Any::Buffer(data.into())` |
| `Value::Scalar(ScalarValue::Bytes(..))` | document.rs:255,273 | Pattern match on `yrs::Value` / `yrs::Any::Buffer` |

---

## 10. Risk Register

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Concurrent array ordering differs** | Medium | Write tests, document actual Yrs behavior, don't assert specific ordering of concurrent appends |
| **Serialized document size changes** | Low | Benchmark before/after. Yrs V1 encoding is generally compact. |
| **RwLock + Yrs transactions** | Low | Keep RwLock initially. Yrs transactions are short-lived and the RwLock prevents data races at the InterfaceDocument level. |
| **Breaking wire protocol** | N/A | Greenfield project, no backward compatibility needed |
| **yrs version mismatch with AppFlowy** | Medium | Pin to same yrs version AppFlowy uses (check AppFlowy-Collab Cargo.toml). Currently yrs 0.21. |
| **`get_or_insert_*` creates types on read** | Low | Pre-initialize shared types in constructor. Use `get_map`/`get_array` (non-creating) for read paths if available. |
| **Event deduplication** | Low | Current dual-write pattern (EventStore + document) is preserved. Events are append-only, no dedup needed. |

---

## Execution Checklist

- [ ] **Phase 0:** Update `Cargo.toml` dependencies (root + crates)
- [ ] **Phase 1:** Rewrite `InterfaceDocument` in `crates/indras-sync/src/document.rs`
- [ ] **Phase 1:** Remove `refresh_object_ids()` and all callers
- [ ] **Phase 2:** Update `PeerSyncState` in `sync_protocol.rs`
- [ ] **Phase 2:** Update `SyncMessage` in `indras-core/src/traits.rs`
- [ ] **Phase 2:** Update sync flow in `generate_sync_message()` / `apply_sync_message()`
- [ ] **Phase 3:** Update content type string in storage
- [ ] **Phase 4:** Update `syncable_notebook.rs` (indras-notes example)
- [ ] **Phase 4:** Update `document.rs` (sync-demo example)
- [ ] **Phase 5:** All existing tests pass with `cargo test --workspace`
- [ ] **Phase 5:** Write new convergence tests for Yrs-specific behaviors
- [ ] **Phase 5:** `cargo clippy --workspace` clean
- [ ] **Phase 6 (future):** Add Awareness protocol support
