# indras-relay

Blind relay server for the Indras P2P mesh network. Stores and forwards encrypted event blobs without ability to read them.

## Purpose

The relay is a never-offline super-peer that provides **store-and-forward** capabilities for peers that reconnect after going offline. It observes gossip traffic on interface topics, caches encrypted `InterfaceEventMessage` blobs, and delivers them to peers on demand. The relay is optional—the network works without it, but it improves reliability for intermittently-connected peers.

**Key principle: The relay is BLIND.** It never receives interface encryption keys and cannot decrypt any content. This enables secure forwarding: peers trust the relay to store their data because the relay provably cannot read it.

## Architecture

### Core Components

**RelayNode** (`relay_node.rs`)
- Main server loop combining transport, gossip, and storage
- Creates its own iroh `Endpoint` and `Gossip` instance
- Accepts QUIC connections on `indras/1` ALPN
- Handles three relay-specific message types: `RelayRegister`, `RelayUnregister`, `RelayRetrieve`
- Spawns per-topic gossip observer tasks that store `InterfaceEventMessage` blobs
- Runs background cleanup and admin API tasks

**BlobStore** (`blob_store.rs`)
- redb-backed persistent storage for encrypted event blobs
- Composite key: `(interface_id, sender_hash, sequence)` for efficient range queries
- Tracks per-interface byte usage for quota enforcement
- TTL-based expiration with background cleanup
- Public methods: `store_event()`, `events_after()`, `evict_interface()`, `cleanup_expired()`

**RegistrationState** (`registration.rs`)
- DashMap-based in-memory tracking: peers → interfaces and interfaces → peers
- JSON persistence to `registrations.json` on disk
- Tracks registration time, display name, and last-seen timestamp per peer
- Public methods: `register()`, `unregister()`, `touch()`, `peer_interfaces()`, `registered_interfaces()`

**QuotaManager** (`quota.rs`)
- Per-peer interface count and byte limits
- Global byte cap to prevent total relay exhaustion
- Enforces limits before accepting new registrations
- Public methods: `can_register()`, `can_store()`, `record_registration()`, `record_storage()`

**AdminState + admin API** (`admin.rs`)
- Axum HTTP API with bearer token authentication
- Endpoints: `/health`, `/stats`, `/peers`, `/interfaces`
- All endpoints require `Authorization: Bearer <token>` header (except `/health`)
- Serialized responses for monitoring and debugging

**RelayConfig** (`config.rs`)
- TOML-based configuration with sensible defaults
- Sections: `quota` (per-peer limits), `storage` (retention and cleanup)
- Loadable via `RelayConfig::from_file()`

**RelayError** (`error.rs`)
- Unified error type wrapping transport, storage, quota, and serialization errors
- Integrates redb database errors via `#[from]`

## Data Flow

### Registration

1. Peer sends `RelayRegister(interface_ids, display_name)`
2. Relay checks quota via `QuotaManager::can_register()`
3. For each accepted interface:
   - Subscribe to gossip topic (derived from interface_id)
   - Spawn `spawn_topic_observer()` task to drain gossip messages
   - Persist registration to disk
4. Send `RelayRegisterAck` with accepted/rejected lists

### Observation

1. Gossip observer task receives `InterfaceEventMessage` on subscribed topic
2. Parse framed WireMessage, extract encrypted blob
3. Store as `StoredEvent` via `BlobStore::store_event()`
4. Update per-interface byte usage

### Retrieval

1. Peer sends `RelayRetrieve(interface_id, after_event_id)`
2. Query blob store for events after specified event_id
3. Send `RelayDelivery` with matching `StoredEvent` list
4. Peer decrypts blobs locally

### Cleanup

1. Background task wakes periodically (configurable interval)
2. Query blob store for events older than TTL
3. Delete expired events and update usage tables

## Design Decisions

- **No IndrasNode**: The relay creates its own iroh endpoint and bypasses `IndrasNode` and `IrohNetworkAdapter`. This avoids unnecessary overhead and allows the relay to act as a pure passthrough without implementing the full Indras protocol.

- **Gossip topic derivation**: Topics are derived using the exact same algorithm as `DiscoveryService::topic_for_interface` in indras-transport, ensuring the relay subscribes to the same topics peers publish to.

- **Composite key indexing**: Events are keyed by `(interface_id, sender_hash, sequence)` for efficient prefix-based range queries when peers request events after a specific event_id.

- **DashMap for registrations**: Concurrent peer registrations use `DashMap` for lock-free reads and atomic updates without blocking the main connection loop.

- **Quota checking at registration time**: Interface count limits are enforced when peers register, not at storage time, to provide early feedback.

- **Persistent registration state**: Peer registrations are saved to disk so that after relay restart, it automatically re-subscribes to known topics without losing coverage.

## Dependencies

- `indras-core`: InterfaceId, EventId, PeerIdentity types
- `indras-transport`: WireMessage protocol, IrohIdentity, gossip topic derivation
- `iroh`, `iroh-gossip`: Transport and gossip network primitives
- `redb`: Embedded relational database for persistent blob storage
- `axum`: HTTP API framework
- `tokio`: Async runtime
- `dashmap`, `serde`, `chrono`, `thiserror`: Utilities

## Testing

Each module includes comprehensive unit tests. Run tests via:

```bash
cargo test -p indras-relay
```

Key test coverage:
- BlobStore: store/retrieve, filtering, eviction, cleanup, usage tracking
- RegistrationState: register/unregister, persistence roundtrip, multi-peer scenarios
- QuotaManager: quota enforcement, registration limits, storage limits
