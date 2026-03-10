# indras-relay Implementation Report

**Date:** 2026-03-09
**Branch:** `worktree-relay-node`
**Commit:** `ff8ddfb`

## Overview

Implemented `indras-relay`, a blind store-and-forward relay server for the Indras P2P mesh network. The relay acts as an always-on super-peer that caches encrypted gossip traffic and delivers missed events to peers that reconnect after being offline — without ever decrypting any content.

## What Was Built

### New Crate: `crates/indras-relay/` (2,828 lines across 11 files)

| File | Lines | Purpose |
|------|-------|---------|
| `relay_node.rs` | ~600 | Core server: iroh QUIC endpoint, gossip topic subscription, connection dispatch, message handling |
| `blob_store.rs` | ~500 | redb-backed persistent storage for encrypted event blobs, indexed by (InterfaceId, EventId) composite keys |
| `registration.rs` | ~390 | Peer-to-interface registration tracking with JSON persistence to disk |
| `admin.rs` | ~180 | HTTP admin API (axum) with bearer token auth: `/health`, `/stats`, `/peers`, `/interfaces` |
| `config.rs` | ~170 | TOML config parsing with `QuotaConfig` and `StorageConfig` sub-sections |
| `quota.rs` | ~150 | Per-peer byte limits, interface count limits, and global byte cap enforcement |
| `error.rs` | ~60 | Unified `RelayError` type covering storage, quota, transport, config, and serialization errors |
| `lib.rs` | ~30 | Library root with module declarations and re-exports |
| `main.rs` | ~50 | CLI entry point using clap with config file, data dir, and admin bind overrides |
| `Cargo.toml` | ~50 | Bin + lib targets, dependencies on iroh, redb, axum, tokio, etc. |
| `AGENTS.md` | ~120 | Architectural documentation per project convention |

### Wire Protocol Extension: `crates/indras-transport/src/protocol.rs` (+294 lines)

Added 5 new `WireMessage` variants and 6 supporting structs:

| Message | Direction | Purpose |
|---------|-----------|---------|
| `RelayRegister` | Peer -> Relay | Register interfaces for store-and-forward |
| `RelayRegisterAck` | Relay -> Peer | Acknowledge registration (accepted/rejected) |
| `RelayUnregister` | Peer -> Relay | Unregister interfaces |
| `RelayRetrieve` | Peer -> Relay | Request stored events after a given EventId |
| `RelayDelivery` | Relay -> Peer | Deliver stored encrypted events |
| `StoredEvent` | (data type) | Opaque encrypted event blob with metadata |

### Other Changes

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Added `indras-relay` to members and workspace deps |
| `Cargo.lock` | Updated with new dependencies (redb, axum, tower-http, futures-lite) |
| `connection.rs` | Fixed pre-existing test bug (missing `local_only` field) |

## Architecture

```
Peer A                    Relay R                     Peer B (offline)
  |                         |                            |
  |-- RelayRegister ------->|                            |
  |<-- RelayRegisterAck ----|                            |
  |                         |                            |
  |== gossip: InterfaceEvent ==>| (observes & stores)   |
  |                         |                            |
  |                         |           (B comes online) |
  |                         |<--- RelayRetrieve ---------|
  |                         |---- RelayDelivery -------->|
  |                         |                            |
```

The relay is **blind** by design:
- Subscribes to gossip topics (public, derived from InterfaceId)
- Stores `InterfaceEventMessage` blobs as opaque encrypted bytes
- Never receives interface keys, never joins as a CRDT participant
- Cannot decrypt `encrypted_event` payloads (ChaCha20-Poly1305)
- Can only see: `interface_id`, `event_id`, `nonce`, sender identity

## Key Design Decisions

1. **Bypasses IndrasNode**: Uses iroh `Endpoint` + `Gossip` directly rather than composing with `IrohNetworkAdapter`, keeping the relay simple and decoupled from node-layer logic.

2. **Gossip topic derivation**: `topic_for_interface()` exactly matches `DiscoveryService::topic_for_interface()` so the relay observes the same topics peers publish to.

3. **redb storage**: Chose redb for zero-config embedded storage. Events use composite keys `(interface_id[32] ++ sender_hash[8] ++ sequence[8])` for efficient range scans.

4. **Per-topic observer tasks**: Each gossip subscription spawns an independent tokio task that drains the `GossipReceiver` stream, providing isolation and backpressure.

5. **Quota enforcement**: Checked at registration time (interface count) and could be extended to check at storage time (byte limits). Global cap prevents any single relay from unbounded growth.

## Test Results

### indras-relay (17 tests)
- `blob_store::tests` (5): store/retrieve, filtering, eviction, usage tracking, TTL cleanup
- `registration::tests` (4): register/lookup, unregister, persistence roundtrip, multi-peer
- `quota::tests` (5): registration limits, storage limits, recording, unregistration
- `config::tests` (2): defaults, TOML parsing
- All **17 passed** in 0.32s

### indras-transport relay tests (5 tests)
- Roundtrip serialization for all 5 new `WireMessage` variants
- All **5 passed**

### Workspace
- `cargo build -p indras-relay` — clean (0 errors, 0 warnings)
- `cargo check` — no regressions (only pre-existing `indras-simulation` errors)

## Future Work (Phase 2+)

- Client-side relay integration in `indras-network` (auto-discover, auto-register, auto-retrieve)
- Paid quotas / token-based storage accounting
- Relay federation (relays sharing registration info)
- Artifact pinning (large blob storage beyond events)
