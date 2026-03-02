# indras-transport

iroh-based QUIC transport layer. Manages peer connections, connection pooling, peer discovery
via iroh-gossip, and the wire framing protocol that sits between raw QUIC streams and the
higher-level indras-network messaging logic.

## Purpose

Implement the network I/O boundary: accept and establish QUIC connections (with hole
punching), maintain a live connection pool keyed by `iroh::PublicKey`, drive peer discovery
over gossip topics, and serialize/deserialize `WireMessage` frames using postcard encoding.

## Module Map

| Module | Contents |
|---|---|
| `connection` | `ConnectionManager`, `ConnectionConfig`, `ConnectionStats`, `ConnectionError` |
| `discovery` | `DiscoveryService`, `DiscoveryConfig`, `DiscoveryStats`, `PeerEvent`, `PeerInfo` |
| `adapter` | `IrohNetworkAdapter`, `AdapterConfig`, `AdapterError`; bridges transport ↔ indras-core |
| `identity` | `IrohIdentity`; wraps `iroh::PublicKey` as a `PeerIdentity` impl |
| `protocol` | `WireMessage` enum, all message structs, ALPN constant, framing functions |
| `error` | `TransportError` |

## Key Types

- **`ConnectionManager`** — owns the `iroh::Endpoint`, accepts inbound connections, dials
  outbound connections, and pools live `iroh::endpoint::Connection` handles keyed by
  `PublicKey`. Call `connect(NodeAddr)` to get a pooled connection.
- **`ConnectionConfig`** — tuning knobs: keepalive interval, idle timeout, max concurrent
  streams, relay URL override.
- **`DiscoveryService`** — wraps `iroh-gossip` to publish and receive `PeerInfo` on a gossip
  topic derived from the interface ID. Emits `PeerEvent::{Joined, Left}` to the adapter.
- **`IrohNetworkAdapter`** — implements the `indras-core` network interface for a real iroh
  node; translates `InterfaceEvent`s to `WireMessage`s and dispatches them over pooled
  connections.
- **`IrohIdentity`** — newtype over `iroh::PublicKey` that implements `PeerIdentity`; used
  wherever the network stack needs a real identity (vs. `SimulationIdentity`).
- **`WireMessage`** — enum of all messages that cross the wire:
  - `SerializedPacket` — store-and-forward event payload
  - `SerializedConfirmation` — delivery acknowledgement
  - `SyncRequest` / `SyncResponse` — state reconciliation handshake
  - `InterfaceJoinMessage` / `InterfaceLeaveMessage` — membership signals
  - `IntroductionRequestMessage` / `IntroductionResponseMessage` — peer introduction handshake
  - `PeerIntroductionMessage` — third-party introduction (A introduces B to C)
  - `PresenceInfo` / `RealmPeerInfo` — online presence and realm membership metadata
- **`frame_message(msg)`** — serializes a `WireMessage` to postcard bytes with a 4-byte
  little-endian length prefix. **`parse_framed_message(buf)`** — inverse operation.
- **`ALPN_INDRAS`** — the ALPN byte string that iroh uses to route streams to this protocol.

## Key Patterns

- **Connection pool**: `ConnectionManager` caches one `Connection` per remote `PublicKey`;
  callers get the cached handle or a fresh dial. Pool entries are evicted on connection close.
- **Framing protocol**: every QUIC stream carries zero or more length-prefixed postcard frames.
  Always use `frame_message` / `parse_framed_message` — never write raw bytes to a stream.
- **ALPN routing**: iroh multiplexes multiple protocols on one endpoint via ALPN. This crate
  registers `ALPN_INDRAS`. Other crates on the same endpoint must use different ALPN strings.
- **Gossip topics**: `DiscoveryService` derives a gossip topic from `InterfaceId` bytes so
  each interface has an isolated peer-discovery namespace.
- **Hole punching**: iroh handles NAT traversal internally; `ConnectionManager` just calls
  `endpoint.connect(node_addr, ALPN_INDRAS)` and iroh attempts direct + relay paths.

## Gotchas

- `IrohNetworkAdapter` is async and long-lived; spawn it on a dedicated tokio task, not
  inline in a request handler.
- `ConnectionManager::connect` may return a relay-routed connection if direct hole punching
  fails — latency can be higher. Check `ConnectionStats::is_direct` if you care.
- iroh's `NodeAddr` includes both the public key and one or more socket addresses (direct
  or relay). Passing only a `PublicKey` without addresses requires a relay-capable endpoint.
- `postcard` serialization is not self-describing; the `WireMessage` enum discriminant must
  stay stable across versions. Add new variants at the end only.
- `DiscoveryService` requires the gossip router to be running on the same iroh endpoint;
  ensure `iroh-gossip` is initialised before calling `DiscoveryService::start`.
- Re-exported iroh types (`Connection`, `Endpoint`, `EndpointAddr`, `PublicKey`,
  `SecretKey`) are passed through from `iroh` directly — check the workspace iroh version
  when upgrading.

## Dependencies

| Crate | Use |
|---|---|
| `indras-core` | `PeerIdentity`, `InterfaceEvent`, `PacketStore`, core traits |
| `iroh` | QUIC endpoint, `PublicKey`, `SecretKey`, `NodeAddr` |
| `iroh-gossip` | Peer discovery pub/sub |
| `tokio` | Async runtime, channels |
| `postcard` | Wire serialization |
| `dashmap` | Connection pool (concurrent map) |
| `tracing` | Structured logging |
| `bytes` | Zero-copy payload buffers |

## Testing

```bash
cargo test -p indras-transport
```

- Unit tests for `protocol` module: round-trip `frame_message` / `parse_framed_message`
  for each `WireMessage` variant.
- Integration tests that exercise `ConnectionManager` require a live iroh endpoint; use
  `tokio-test` and a loopback endpoint pair (bind two managers on `127.0.0.1:0`).
- `DiscoveryService` tests need a gossip router; either mock the gossip channel or use a
  local relay-free two-node setup.
- Prefer `SimulationIdentity` + `mock_transport` from `indras-core` for logic tests that
  do not need real network I/O.
