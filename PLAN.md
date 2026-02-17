# Plan: Automerge Store-and-Forward Sync for Artifact Trees

## Problem

The current `indras-sync` crate uses Automerge's built-in sync protocol
(`generateSyncMessage` / `receiveSyncMessage`), which requires 2-4 round trips
to converge. With store-and-forward transport where peers are periodically
offline, each round trip may take hours or days. A group chat message that
should be delivered in one hop currently requires multiple back-and-forth
exchanges before all peers converge.

## Solution

Replace the multi-round-trip sync protocol with **raw Automerge changes +
per-peer head tracking** for artifact trees. One delivery = convergence.

### Core Insight

Instead of negotiating "what do you have?" via bloom filters (the sync protocol),
each peer tracks the last-known Automerge heads for every remote peer, then
sends `doc.get_changes(&known_heads)` as a single payload. The receiver applies
with `doc.apply_changes()` — duplicates are idempotent, so stale head info just
means slightly larger payloads, never data loss.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    indras-artifacts                       │
│  Vault, TreeArtifact, LeafArtifact, ArtifactIndex        │
│  (domain types — no sync logic)                          │
└──────────────────────┬──────────────────────────────────┘
                       │ uses
┌──────────────────────▼──────────────────────────────────┐
│                     indras-sync                          │
│                                                          │
│  ┌─────────────────┐  ┌──────────────────────────────┐  │
│  │InterfaceDocument │  │ ArtifactDocument (NEW)       │  │
│  │(members, meta,   │  │ (per-tree Automerge doc with │  │
│  │ events list)     │  │  references, grants, meta)   │  │
│  │                  │  │                              │  │
│  │Uses: sync proto- │  │ Uses: raw changes +          │  │
│  │col (online peers)│  │ head tracking (offline-safe) │  │
│  └─────────────────┘  └──────────────────────────────┘  │
│                                                          │
│  ┌─────────────────┐  ┌──────────────────────────────┐  │
│  │ EventStore       │  │ HeadTracker (NEW)            │  │
│  │ (store-and-fwd   │  │ (per-peer, per-artifact      │  │
│  │  delivery track) │  │  Automerge head tracking)    │  │
│  └─────────────────┘  └──────────────────────────────┘  │
└──────────────────────┬──────────────────────────────────┘
                       │ uses
┌──────────────────────▼──────────────────────────────────┐
│                   indras-network                         │
│                                                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │ ArtifactSyncRegistry (existing)                  │    │
│  │ Creates per-artifact gossip interfaces.          │    │
│  │ NEW: wires ArtifactDocument + HeadTracker         │    │
│  │ to the gossip transport layer.                   │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

## Existing Code Inventory

### What exists and stays as-is

| File | Purpose | Changes |
|------|---------|---------|
| `indras-artifacts/src/artifact.rs` | Leaf/Tree types, ArtifactId | None |
| `indras-artifacts/src/vault.rs` | Vault operations, tree composition | None |
| `indras-artifacts/src/access.rs` | Grants, AccessMode | None |
| `indras-sync/src/event_store.rs` | Store-and-forward delivery tracking | None |
| `indras-sync/src/document.rs` | InterfaceDocument (Automerge) | Keep for interface-level state |
| `indras-sync/src/sync_protocol.rs` | Automerge sync protocol wrapper | Keep for online interactive sync |

### What exists and gets extended

| File | Purpose | Changes |
|------|---------|---------|
| `indras-network/src/artifact_sync.rs` | ArtifactSyncRegistry | Wire to ArtifactDocument + HeadTracker |
| `indras-network/src/artifact_index.rs` | ArtifactIndex (HashMap) | Add methods to materialize from ArtifactDocument |
| `indras-sync/src/lib.rs` | Re-exports | Add new module exports |

### What gets created

| File | Purpose |
|------|---------|
| `indras-sync/src/artifact_document.rs` | Per-tree Automerge document |
| `indras-sync/src/head_tracker.rs` | Per-peer head tracking for raw changes |
| `indras-sync/src/raw_sync.rs` | Raw changes sync protocol (replaces interactive sync for artifacts) |

## Implementation Steps

### Step 1: `ArtifactDocument` — Per-Tree Automerge Document

**File:** `crates/indras-sync/src/artifact_document.rs`

An Automerge `AutoCommit` document representing a single shared Tree artifact.
Unlike `InterfaceDocument` (which stores an event log), this stores the tree's
mutable state directly as Automerge types for automatic conflict resolution.

**Document schema:**

```json
{
  "artifact_id": "<hex>",
  "steward": "<hex>",
  "artifact_type": "Story|Gallery|Document|...",
  "status": "active|recalled|transferred",
  "created_at": <timestamp>,
  "references": [
    { "artifact_id": "<hex>", "position": <u64>, "label": "<string>|null" },
    ...
  ],
  "grants": [
    { "grantee": "<hex>", "mode": "permanent|revocable|timed", "granted_at": <i64>, "granted_by": "<hex>" },
    ...
  ],
  "metadata": {
    "<key>": <bytes>,
    ...
  }
}
```

**Why this schema works with Automerge:**

- `references` is a List of Maps — concurrent appends merge cleanly (both items
  appear, order determined by actor ID). This is the primary operation for group
  chat (append message refs) and collections (add child artifacts).
- `grants` is a List of Maps — steward-only writes in practice, but concurrent
  grant additions from different stewards (after transfer) merge safely.
- `metadata` is a Map — concurrent key writes to different keys merge cleanly.
  Same-key conflicts use Automerge's LWW (last-writer-wins per actor).
- `status`, `steward` are scalar registers — LWW semantics. Only the steward
  writes these, so conflicts are rare.

**API surface:**

```rust
pub struct ArtifactDocument {
    doc: AutoCommit,
}

impl ArtifactDocument {
    // Construction
    pub fn new(artifact_id: &ArtifactId, steward: &MemberId, tree_type: &TreeType, now: i64) -> Self;
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError>;
    pub fn save(&mut self) -> Vec<u8>;
    pub fn fork(&mut self) -> Result<Self, SyncError>;

    // Head tracking (for raw changes sync)
    pub fn get_heads(&self) -> Vec<ChangeHash>;
    pub fn get_changes_since(&self, heads: &[ChangeHash]) -> Vec<Change>;
    pub fn get_all_changes(&self) -> Vec<Change>;
    pub fn apply_changes(&mut self, changes: Vec<Change>) -> Result<(), SyncError>;

    // References (tree children)
    pub fn append_ref(&mut self, child_id: &ArtifactId, position: u64, label: Option<&str>);
    pub fn remove_ref(&mut self, child_id: &ArtifactId);
    pub fn references(&self) -> Vec<ArtifactRef>;

    // Grants
    pub fn add_grant(&mut self, grant: &AccessGrant);
    pub fn remove_grant(&mut self, grantee: &MemberId);
    pub fn grants(&self) -> Vec<AccessGrant>;

    // Metadata
    pub fn set_metadata(&mut self, key: &str, value: &[u8]);
    pub fn get_metadata(&self, key: &str) -> Option<Vec<u8>>;
    pub fn metadata(&self) -> BTreeMap<String, Vec<u8>>;

    // Status
    pub fn status(&self) -> ArtifactStatus;
    pub fn set_status(&mut self, status: &ArtifactStatus);
    pub fn steward(&self) -> MemberId;
    pub fn set_steward(&mut self, steward: &MemberId);
}
```

**Tests:**

- Create document, verify schema initialized
- Append refs concurrently from forked docs, merge, verify both present
- Add grants, verify serialization roundtrip
- Set metadata, verify concurrent key writes merge
- Save/load roundtrip
- `get_changes_since` returns correct delta
- `apply_changes` with duplicates is idempotent

### Step 2: `HeadTracker` — Per-Peer Automerge Head Tracking

**File:** `crates/indras-sync/src/head_tracker.rs`

Tracks what each remote peer is known to have for each artifact's Automerge
document. This replaces the interactive "bloom filter negotiation" of the
sync protocol with explicit bookkeeping.

```rust
/// Key: (ArtifactId, MemberId) → last known Automerge heads for that peer
pub struct HeadTracker {
    heads: HashMap<(ArtifactId, MemberId), Vec<ChangeHash>>,
}

impl HeadTracker {
    pub fn new() -> Self;

    /// Record that a peer now has these heads for an artifact.
    pub fn update(&mut self, artifact_id: &ArtifactId, peer: &MemberId, heads: Vec<ChangeHash>);

    /// Get the last known heads for a peer's copy of an artifact.
    /// Returns empty slice if unknown (triggers full sync).
    pub fn get(&self, artifact_id: &ArtifactId, peer: &MemberId) -> &[ChangeHash];

    /// Remove all tracking for a peer (they left / were revoked).
    pub fn remove_peer(&mut self, peer: &MemberId);

    /// Remove all tracking for an artifact (it was recalled).
    pub fn remove_artifact(&mut self, artifact_id: &ArtifactId);

    /// Serialize to bytes (for persistence across restarts).
    pub fn save(&self) -> Vec<u8>;

    /// Load from bytes.
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError>;
}
```

**Design decisions:**

- **Empty heads = full sync.** If we have no record of a peer (first encounter,
  lost state, new member), `get()` returns `&[]`, which means
  `get_changes_since(&[])` returns ALL changes. Safe and correct.
- **Stale heads = over-send.** If our record is outdated (peer got changes from
  someone else we don't know about), we send changes they might already have.
  `apply_changes()` silently ignores duplicates. Bandwidth cost, never data loss.
- **Persistence is optional.** Without persistence, the first reconnect does a
  full sync (larger payload). With persistence, only deltas. Both converge.

**Tests:**

- Update and retrieve heads
- Unknown peer returns empty slice
- Remove peer clears all artifacts for that peer
- Remove artifact clears all peers for that artifact
- Save/load roundtrip

### Step 3: `RawSync` — Single-Delivery Sync Protocol

**File:** `crates/indras-sync/src/raw_sync.rs`

The protocol that replaces multi-round-trip Automerge sync with single-hop
delivery over store-and-forward transport.

```rust
/// A sync payload: raw Automerge changes + sender's current heads.
#[derive(Serialize, Deserialize)]
pub struct ArtifactSyncPayload {
    /// Which artifact this is for
    pub artifact_id: ArtifactId,
    /// The sender's current heads after these changes
    pub sender_heads: Vec<ChangeHash>,
    /// Raw encoded changes (delta since recipient's last known heads)
    pub changes: Vec<u8>,
}

pub struct RawSync;

impl RawSync {
    /// Prepare a sync payload for a specific peer.
    ///
    /// Computes the delta between our document and what the peer last had.
    /// If we have no head tracking for this peer, sends all changes.
    pub fn prepare_payload(
        doc: &ArtifactDocument,
        tracker: &HeadTracker,
        artifact_id: &ArtifactId,
        recipient: &MemberId,
    ) -> ArtifactSyncPayload;

    /// Apply a received sync payload.
    ///
    /// Applies changes to the local document and updates head tracking
    /// for the sender.
    pub fn apply_payload(
        doc: &mut ArtifactDocument,
        tracker: &mut HeadTracker,
        payload: ArtifactSyncPayload,
        sender: &MemberId,
    ) -> Result<(), SyncError>;

    /// Prepare payloads for ALL audience members of an artifact.
    ///
    /// Used after a local mutation (append ref, add grant, etc.) to
    /// enqueue sync payloads for each peer via store-and-forward.
    pub fn broadcast_payloads(
        doc: &ArtifactDocument,
        tracker: &HeadTracker,
        artifact_id: &ArtifactId,
        audience: &[MemberId],
        self_id: &MemberId,
    ) -> Vec<(MemberId, ArtifactSyncPayload)>;
}
```

**The flow:**

```
LOCAL MUTATION:
  1. Modify ArtifactDocument (append_ref, add_grant, etc.)
  2. Call RawSync::broadcast_payloads() → Vec<(peer, payload)>
  3. For each (peer, payload): enqueue via store-and-forward transport

INCOMING PAYLOAD:
  1. Receive ArtifactSyncPayload from transport
  2. Call RawSync::apply_payload() → updates doc + head tracker
  3. Optionally: send our own changes back if we have diverged
```

**Tests:**

- Prepare payload from known heads → only delta changes
- Prepare payload from unknown peer → all changes
- Apply payload updates document state
- Apply payload updates head tracker with sender's heads
- Apply duplicate payload is idempotent (same state)
- Broadcast skips self
- Round-trip: A mutates → prepare → B applies → B has A's state

### Step 4: Wire into `ArtifactSyncRegistry`

**File:** `crates/indras-network/src/artifact_sync.rs` (modify existing)

The existing `ArtifactSyncRegistry` creates/tears down per-artifact gossip
interfaces based on audience. Extend it to manage `ArtifactDocument` +
`HeadTracker` instances and route payloads through the gossip transport.

**Changes to struct:**

```rust
pub struct ArtifactSyncRegistry {
    node: Arc<IndrasNode>,
    self_id: MemberId,
    active: DashMap<ArtifactId, InterfaceId>,
    // NEW: per-artifact Automerge documents
    documents: DashMap<ArtifactId, ArtifactDocument>,
    // NEW: shared head tracker across all artifacts
    head_tracker: RwLock<HeadTracker>,
}
```

**New methods:**

```rust
impl ArtifactSyncRegistry {
    /// Get or create the ArtifactDocument for a tree.
    pub fn get_or_create_document(
        &self, artifact_id: &ArtifactId, entry: &HomeArtifactEntry
    ) -> &ArtifactDocument;

    /// Handle a local mutation to an artifact tree.
    /// Broadcasts raw changes to all audience members via gossip.
    pub async fn on_local_mutation(&self, artifact_id: &ArtifactId) -> Result<()>;

    /// Handle an incoming sync payload from the gossip transport.
    pub async fn on_incoming_payload(
        &self, sender: &MemberId, payload: ArtifactSyncPayload
    ) -> Result<()>;

    /// Persist head tracker + document state (on shutdown / periodic).
    pub async fn persist(&self) -> Result<()>;

    /// Load persisted state (on startup).
    pub async fn load_persisted(&self) -> Result<()>;
}
```

**Integration with existing `reconcile()`:**

The existing `reconcile()` method already handles interface lifecycle. Add
document lifecycle alongside it:

```rust
pub async fn reconcile(&self, artifact_id: &ArtifactId, entry: &HomeArtifactEntry) -> Result<()> {
    let audience = ...; // existing grant filtering

    if audience.is_empty() {
        self.teardown(artifact_id).await?;
        self.documents.remove(artifact_id);                        // NEW
        self.head_tracker.write().remove_artifact(artifact_id);    // NEW
    } else {
        self.ensure(artifact_id, &audience).await?;
        self.get_or_create_document(artifact_id, entry);           // NEW
    }
    Ok(())
}
```

### Step 5: Integration Tests

**File:** `crates/indras-sync/tests/artifact_sync_integration.rs`

End-to-end tests covering the full store-and-forward artifact sync flow:

1. **Basic sync:** A creates Story, appends message ref, syncs to B via raw
   payload. Assert B has the ref.

2. **Offline convergence:** A and B both append refs while disconnected, exchange
   payloads, both converge to same state with both refs present.

3. **Three-peer group:** A, B, C share a Story. A appends ref. Payload reaches
   B (online) and is queued for C (offline). C comes online, applies payload,
   all three converge.

4. **New member full sync:** A grants access to D. D has no prior state.
   HeadTracker returns empty → D receives all changes. D now has full document.

5. **Revoke cleanup:** A revokes B. B's head tracking is cleared. Interface
   torn down. B can no longer receive updates.

6. **Recall cascade:** A recalls a parent tree. All descendant documents are
   cleaned up. Head tracking cleared for the whole subtree.

7. **Stale heads (over-send):** A's record of B is outdated (B got changes from
   C that A doesn't know about). A sends changes B already has. B applies
   idempotently. No data loss, no corruption.

8. **Metadata concurrent write:** A sets metadata key "color" = blue, B sets
   "size" = large concurrently. After sync, both have both keys.

### Step 6: Simulation Scenario

**File:** `simulation/scripts/scenarios/artifact_sync_offline.lua`

A Lua simulation scenario exercising the offline sync path:

```
Setup: 3 peers (A, B, C) share a Story tree

  1. A appends 5 message leaf refs to Story
  2. B goes offline
  3. A appends 3 more refs
  4. C appends 2 refs while B is still offline
  5. B comes online → receives queued payloads from A and C
  6. Assert: all 3 peers have 10 refs in their Story document
  7. Assert: all 3 peers have identical Automerge heads
  8. Assert: HeadTracker shows correct heads for each peer pair
```

## Phasing

| Phase | Steps | Deliverable | Can merge independently |
|-------|-------|-------------|------------------------|
| 1 | Steps 1-2 | `ArtifactDocument` + `HeadTracker` with unit tests | Yes |
| 2 | Step 3 | `RawSync` protocol with unit tests | Yes (depends on Phase 1) |
| 3 | Step 4 | Wire into `ArtifactSyncRegistry` | Yes (depends on Phase 2) |
| 4 | Steps 5-6 | Integration tests + simulation scenario | Yes (depends on Phase 3) |

## What This Does NOT Change

- **Leaf artifacts** — Still content-addressed blob transfer. No CRDT needed.
  `ArtifactId::Blob` = BLAKE3 hash of payload. Send blob, receive blob, done.

- **InterfaceDocument** — Still used for interface-level state (member list,
  metadata, event log). The interactive sync protocol is fine here because
  interface membership changes are rare and peers are usually online for them.

- **EventStore** — Still used for lightweight real-time event delivery (presence,
  typing indicators, ephemeral signals). Complementary, not replaced.

- **Vault operations** — `attach_child`, `compose`, `grant_access` etc. remain
  the domain-level API. They produce mutations that then get synced via
  `ArtifactDocument`.

- **ArtifactIndex** — Remains the local source-of-truth HashMap. The
  `ArtifactDocument` is the sync representation; the index is the query
  representation. Changes flow: local mutation → ArtifactDocument → sync →
  remote ArtifactDocument → remote ArtifactIndex.

## Open Questions

1. **Reference ordering:** When two peers concurrently append to the same Story,
   Automerge merges both items into the list. The order is deterministic (by
   actor ID) but may not match wall-clock time. The existing `position: u64`
   field in `ArtifactRef` could serve as a sortable timestamp. Should we use
   `created_at` from the leaf artifact, or a dedicated sequence number?

2. **Document persistence format:** Should each `ArtifactDocument` be persisted
   as a standalone Automerge binary file (one file per tree artifact), or stored
   as a blob in the existing artifact store keyed by `ArtifactId::Doc`?
   Standalone files are simpler; store-backed is more consistent with existing
   patterns.

3. **Head tracker persistence frequency:** On every mutation? On shutdown only?
   Periodic timer? Loss of head tracker state just means one over-sized sync
   payload, so the stakes are low. Shutdown-only is probably sufficient.

## Dependencies

- `automerge` 0.7 (already in workspace `Cargo.toml`)
- `postcard` (already used for serialization in `indras-sync`)
- `serde` (already used throughout)
- No new external dependencies required
