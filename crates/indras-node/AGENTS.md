# indras-node

High-level P2P node coordinator that composes the full Indras Network stack into a single
operable unit. `IndrasNode` wires together transport (iroh/QUIC), storage (redb + blobs +
append-only logs), CRDT sync (Automerge via indras-sync), and post-quantum crypto into a
unified API for creating interfaces, sending messages, and subscribing to events.

## Module Map

| Module | Role |
|---|---|
| `lib.rs` | Public re-exports, `IndrasNode` struct and impl |
| `config.rs` | `NodeConfig` — data directory, network flags, sync intervals |
| `error.rs` | `NodeError`, `NodeResult` |
| `keystore.rs` | `Keystore`, `EncryptedKeystore`, `StoryKeystore` — key persistence |
| `message_handler.rs` | `MessageHandler` — background task: verify, decrypt, append, ack |
| `sync_task.rs` | Background CRDT sync loop — periodically pushes Automerge state to peers |

## Key Types

- **`IndrasNode`** — top-level handle; holds `Arc<DashMap<InterfaceId, InterfaceState>>`
- **`NodeConfig`** — data directory, `local_only` mode, `allow_legacy_unsigned` flag
- **`Keystore`** — loads/saves Ed25519 (iroh) + ML-DSA-65 (PQ signing) + ML-KEM-768 (KEM) keys
- **`EncryptedKeystore`** — wraps `Keystore` with Argon2id + ChaCha20-Poly1305 at-rest encryption
- **`StoryKeystore`** — simple unencrypted keystore variant for testing/dev
- **`NetworkMessage`** — enum: `InterfaceEvent`, `SyncRequest`, `SyncResponse`, `EventAck`
- **`SignedNetworkMessage`** — wraps `NetworkMessage` with ML-DSA-65 signature (~5.3 KB overhead)
- **`MessageHandler`** — spawned tokio task; receives `(IrohIdentity, Vec<u8>)` from transport

## Key Patterns

**Startup sequence:** `NodeConfig::with_data_dir` → `IndrasNode::new` (loads keystore, opens
storage, starts transport) → `node.start()` (spawns `MessageHandler` and `sync_task` loops).

**Interface lifecycle:** `create_interface` generates `InterfaceId` + `InterfaceKey`, stores
state in `DashMap`, begins listening. Peers join via `join_interface(key)`. Events flow through
`send_message` → encrypt with `InterfaceKey` (ChaCha20-Poly1305) → sign with PQ identity →
send via transport.

**Message handling pipeline:** incoming bytes → try parse as `SignedNetworkMessage` (verify
ML-DSA-65) → fall back to legacy unsigned if `allow_legacy_unsigned` → dispatch to
`handle_interface_event` / `handle_sync_request` / `handle_sync_response` / `handle_event_ack`.

**Sync:** `sync_task` wakes on interval, calls `interface.generate_sync(&peer)` to produce
Automerge sync messages, sends as `NetworkMessage::SyncRequest`. On receiving a sync request
the `MessageHandler` applies it and immediately sends a `SyncResponse` without waiting for the
next cycle.

**Key files on disk:** `identity.key` (Ed25519), `identity_sk.pq` / `identity_pk.pq`
(ML-DSA-65), `kem_dk.pq` / `kem_ek.pq` (ML-KEM-768), `keystore.salt` (Argon2id salt).
Encrypted variants use `.enc` suffix.

## Gotchas

- `SignedNetworkMessage` carries ~5.3 KB overhead per message (3309-byte signature + 1952-byte
  verifying key). Do not use for high-frequency small messages without batching.
- `allow_legacy_unsigned: true` is required during transitions between signed and unsigned
  peers. Set to `false` in production to enforce PQ authentication.
- `MessageError::UnknownInterface` is logged at `debug` (not `error`) because it is normal
  during startup/shutdown when peers send to interfaces not yet loaded.
- `MessageError::Decryption` logs the sender's `short_id` to help diagnose key mismatches
  between peers who have stale interface keys.
- `state_vector` field in `InterfaceSyncRequest` / `InterfaceSyncResponse` is reserved; unused
  with Automerge but kept for wire compatibility.
- `DutyCycleManager` is `!Send`/`!Sync` by design — wrap in `Arc<Mutex<>>` for multi-thread use.

## Dependencies

Internal: `indras-core`, `indras-transport`, `indras-storage`, `indras-sync`, `indras-crypto`

External: `iroh` (transport), `tokio`, `dashmap`, `postcard` (serialization), `argon2`,
`chacha20poly1305`, `bytes`, `serde`, `hex`, `base64`, `rand`, `tracing`

Dev: `tokio-test`, `tempfile`, `futures`

## Testing

Integration tests live in `tests/` (not `src/`). They require a real tokio runtime and
temporary directories (`tempfile`). Unit tests for message serialization are in
`message_handler.rs` and cover round-trips for all four `NetworkMessage` variants.

```bash
cargo test -p indras-node
```
