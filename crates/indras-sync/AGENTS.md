# indras-sync — AI Agent Guide

## Purpose

CRDT-based document synchronization using Automerge for N-peer interfaces. Provides the sync layer that `indras-network` builds on top of.

## Dual Sync Strategy

The crate implements two complementary sync mechanisms:

### 1. InterfaceDocument (interactive, online)

Full Automerge sync protocol for real-time collaboration. Used for realm state (membership, settings, shared data). The `SyncProtocol` generates and receives sync messages per peer.

### 2. ArtifactDocument (raw changes, offline-safe)

Per-artifact Automerge document using `save_after` / `load_incremental` for store-and-forward sync. Designed for artifact tree metadata — references, grants, status. Works offline because it doesn't need a live sync session.

**Document structure:**
```
ROOT/
  artifact_id: String    steward: String     artifact_type: String
  status: String         created_at: i64
  references: List[Map{artifact_id, position, label}]
  grants: List[Map{grantee, mode, granted_at, granted_by}]
  metadata: Map{key -> Bytes}
```

## Module Map

| Module | Key Types | What It Does |
|--------|-----------|-------------|
| `document.rs` | `InterfaceDocument` | Automerge document backing an N-peer interface |
| `artifact_document.rs` | `ArtifactDocument` | Per-tree Automerge doc for artifact metadata sync |
| `head_tracker.rs` | `HeadTracker` | Tracks last-known Automerge heads per (artifact, peer) |
| `raw_sync.rs` | `RawSync`, `ArtifactSyncPayload` | Stateless prepare/apply pattern for delta sync |
| `event_store.rs` | `EventStore` | Store-and-forward event storage with delivery tracking |
| `sync_protocol.rs` | `SyncProtocol`, `SyncState`, `PeerSyncState` | Automerge sync protocol handlers |
| `n_interface.rs` | `NInterface` | N-peer shared interface implementation |
| `error.rs` | `SyncError`, `SyncResult` | Error types |

## HeadTracker

Tracks the last-known Automerge `ChangeHash` heads for each `(ArtifactId, PlayerId)` pair.

- **Empty entry** (unknown peer) → triggers full sync (all changes sent)
- **Stale entry** → over-sends (safe, Automerge deduplicates)
- **Current entry** → minimal delta sent

Serializable via postcard for persistence across restarts.

## RawSync Pattern

```
Sender:   RawSync::prepare_payload(doc, tracker, artifact_id, recipient) → ArtifactSyncPayload
          ↓ transport (gossip, relay, store-and-forward)
Receiver: RawSync::apply_payload(doc, tracker, payload, sender) → ()
```

- `prepare_payload` uses `tracker.get()` to find what the recipient already has, then calls `doc.save_after(known_heads)`
- `apply_payload` calls `doc.load_incremental()` (idempotent) and updates the tracker with the sender's heads
- `broadcast_payloads` prepares payloads for all audience members (skipping self)

## Common Tasks

**Using ArtifactDocument for a new artifact**: `ArtifactDocument::new(artifact_id, steward, tree_type, now)` creates with initialized schema. Use `append_ref()`, `add_grant()`, `set_metadata()` to populate.

**Bootstrapping from a received payload**: `ArtifactDocument::empty()` creates a shell, then `load_incremental(data)` populates the schema from the sender's changes.

**Persisting sync state**: `HeadTracker::save()` / `HeadTracker::load()` for postcard serialization. `ArtifactDocument::save()` / `ArtifactDocument::load()` for Automerge state.

## Gotchas

- **Never cache Automerge `ObjId`s** — they go stale after sync/merge. The `ArtifactDocument` re-lookups `references_obj()`, `grants_obj()`, `metadata_obj()` on every access
- **`ArtifactDocument::empty()`** is for bootstrapping from received payloads — it has no schema until `load_incremental()` is called
- **`load_incremental` is idempotent** — applying the same bytes twice has no effect
- **HeadTracker entries are overwritten** — `update()` replaces, it doesn't append

## Dependencies

Internal: `indras-core`, `indras-artifacts`
External: `automerge 0.7`, `postcard`, `serde`

## Testing

```bash
cargo test -p indras-sync
```
