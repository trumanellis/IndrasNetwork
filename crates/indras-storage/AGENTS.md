# indras-storage

Tri-layer persistence for Indras Network. Combines an append-only `EventLog`, a redb-backed
`RedbStorage` for structured queryable metadata, and a BLAKE3 content-addressed `BlobStore`
for large payloads. All three layers are unified behind `CompositeStorage`.

## Purpose

Persist interface events, peer registry, sync state, and blob payloads across restarts while
supporting store-and-forward delivery tracking via the `PendingStore` trait. The layered
design matches access patterns: log for immutable history, redb for indexed queries, blobs
for large opaque content.

## Module Map

| Module | Contents |
|---|---|
| `append_log` | `EventLog`, `EventLogConfig`, `EventLogEntry`, `CompactionConfig` |
| `structured` | `RedbStorage`, `RedbStorageConfig`, `InterfaceStore`, `PeerRegistry`, `SyncStateStore` |
| `blobs` | `BlobStore`, `BlobStoreConfig`, `ContentRef` |
| `composite` | `CompositeStorage`, `CompositeStorageConfig`; unified façade over all three layers |
| `memory` | `InMemoryPendingStore`, `InMemoryPacketStore`; test-only in-memory impls |
| `persistent` | `PersistentPendingStore`; redb-backed `PendingStore` impl |
| `quota` | `QuotaManager`, `QuotaManagerBuilder`, `EvictionPolicy` |
| `error` | `StorageError` |
| `lib.rs` | `PendingStore` trait definition |

## Key Types

- **`EventLog`** — append-only per-interface log. Each entry carries an `EventId` + raw
  bytes. Supports sequential reads for replay and audit. Compaction via `CompactionConfig`
  trims entries older than a configurable horizon.
- **`RedbStorage`** — wraps a `redb::Database`; exposes three typed sub-stores:
  - `InterfaceStore` — CRUD for `InterfaceRecord` (name, creation time, member list)
  - `PeerRegistry` — stores `PeerRecord` per peer identity
  - `SyncStateStore` — tracks `SyncStateRecord` (last-seen `EventId` per peer per interface)
- **`BlobStore`** — content-addressed filesystem store; `put(bytes)` → BLAKE3 hex digest;
  `get(ContentRef)` → `Bytes`. Files named by digest under a configurable base directory.
- **`ContentRef`** — newtype wrapping the BLAKE3 hex digest string; used as a stable handle
  to retrieve blobs.
- **`CompositeStorage`** — top-level type that owns all three layers and exposes a unified
  async API. Generic over `I: PeerIdentity`.
- **`PendingStore<I>`** — async trait for store-and-forward tracking: `mark_pending`,
  `pending_for`, `mark_delivered`, `mark_delivered_up_to`, `clear_pending`.
- **`InMemoryPendingStore`** — `DashMap`-backed impl for tests; accepts an optional
  `QuotaManager` to cap per-peer queue depth.
- **`PersistentPendingStore`** — redb-backed `PendingStore` impl for production.
- **`QuotaManager`** — enforces per-peer and global event queue limits; applies
  `EvictionPolicy` (oldest-first by default) when limits are exceeded.

## Key Patterns

- **Separate concerns by access pattern**: event history → `EventLog`; queryable state →
  `RedbStorage`; large payloads → `BlobStore`. Don't put blobs in redb or indexed data in
  the log.
- **Content addressing**: `BlobStore` deduplicates automatically — identical bytes produce
  the same `ContentRef`. Callers store the `ContentRef` in redb alongside other metadata.
- **`PendingStore` for store-and-forward**: when an event cannot be delivered immediately
  (peer offline), call `mark_pending`; on reconnect, call `pending_for` to drain the queue,
  then `mark_delivered_up_to` for bulk acknowledgement.
- **`mark_delivered_up_to`** is a batch optimisation — prefer it over looping
  `mark_delivered` when acknowledging a contiguous sequence from one sender.
- **Quota eviction**: `InMemoryPendingStore::with_quota(QuotaManager::new(per_peer, global))`
  silently drops the oldest events when limits are hit. Check queue depth before relying on
  guaranteed delivery.

## Gotchas

- `redb` opens the database file with an exclusive lock. Only one `RedbStorage` instance
  per file at a time; opening a second will block or fail.
- `EventLog` compaction is not automatic — call it explicitly via `CompactionConfig` on a
  schedule. Logs grow unbounded otherwise.
- `BlobStore::get` returns `None` (not an error) for unknown `ContentRef`s; callers must
  handle the missing case explicitly.
- `CompositeStorage` is generic over `I: PeerIdentity`. When writing helpers that accept
  `CompositeStorage`, propagate the generic or pin to a concrete identity type.
- `PacketStore` trait is defined in `indras-core` and re-exported here for convenience —
  import it from `indras_storage::PacketStore` to avoid depending on `indras-core` directly
  if you only need storage types.
- `InMemoryPendingStore` and `InMemoryPacketStore` are not marked `#[cfg(test)]`; they can
  be used in production for ephemeral nodes, but data is lost on restart.
- `tempfile` is a dev-dependency; use it in tests that need a real filesystem path.

## Dependencies

| Crate | Use |
|---|---|
| `indras-core` | `PeerIdentity`, `EventId`, `PacketStore`, `InterfaceId` |
| `redb` | Embedded key-value database (structured layer) |
| `blake3` | Content hashing for `BlobStore` |
| `tokio` (fs, io-util) | Async file I/O for `EventLog` and `BlobStore` |
| `dashmap` | Concurrent maps in `InMemoryPendingStore` |
| `postcard` | Serialization of stored records |
| `tracing` | Structured logging in storage ops |
| `bytes` | Zero-copy payload handling |

## Testing

```bash
cargo test -p indras-storage
```

- `lib.rs` contains integration-style tests for `InMemoryPendingStore` and `QuotaManager`
  using `SimulationIdentity`.
- Filesystem-backed tests (`EventLog`, `BlobStore`, `RedbStorage`) use `tempfile::TempDir`
  for isolation; each test gets a fresh directory.
- `tokio-test` and `rand` are dev-dependencies available for async test helpers and random
  payload generation.
- Test object-safety of `PendingStore` with the `_assert_object_safe` compile-time check
  already present in `lib.rs`.
