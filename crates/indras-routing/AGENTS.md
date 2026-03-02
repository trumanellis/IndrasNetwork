# indras-routing

Store-and-forward routing with back-propagation for delivery confirmations. Supports both simulation identities (for testing) and real cryptographic identities.

## Purpose

Implements the routing decision layer for Indras Network. When a packet needs to be sent, this crate determines *how* to deliver it: directly, via relay, held for later, or dropped. It also tracks delivery confirmations back through the relay chain.

## Module Map

```
src/
  lib.rs       — module declarations, re-exports, doc examples
  router.rs    — StoreForwardRouter: main routing logic and decision engine
  table.rs     — RoutingTable: cached route info with staleness detection
  mutual.rs    — MutualPeerTracker: tracks shared peers between connected peers
  backprop.rs  — BackPropManager: propagates delivery confirmations upstream
  error.rs     — RoutingError, RoutingResult
```

## Key Types

- `StoreForwardRouter` — main entry point; call `.route(&packet, &peer).await` to get a decision
- `RoutingTable` — caches known routes; handles staleness so relay decisions don't use stale data
- `MutualPeerTracker` — when peer A connects to peer B, records which other peers they share; used to find relay candidates
- `BackPropManager` / `BackPropState` / `BackPropStatus` — tracks in-flight deliveries and propagates ACKs upstream when delivery is confirmed
- `RoutingDecision` (re-exported from `indras-core`) — enum: `DirectDelivery`, `RelayThrough`, `HoldForLater`, `Drop`
- `DropReason`, `RouteInfo` (re-exported from `indras-core`)
- `RoutingError` / `RoutingResult` — crate-local error type

## Routing Decision Flow

Four-step waterfall, evaluated in order:

1. **DIRECT** — destination is online and directly connected → deliver now
2. **HOLD** — destination is known but currently offline → store locally, deliver on reconnect
3. **RELAY** — destination not directly connected → find mutual peer as next hop
4. **DROP** — no route available → discard with a `DropReason`

Relay candidate selection uses `MutualPeerTracker`: it knows which peers two nodes share, so relay picks a node that both sender and destination are connected to.

## Back-Propagation

When a packet is relayed and eventually delivered, a confirmation travels back along the path. `BackPropManager` records which upstream node to notify for each in-flight packet, then sends ACKs when the downstream node confirms delivery. This lets the original sender know the packet arrived.

## Key Patterns

- `StoreForwardRouter` takes `topology` and `storage` at construction — topology answers "is peer X online / connected?", storage holds queued packets
- Call `router.on_peer_connect(&peer_a, &peer_b)` to update the mutual-peer index on each new connection
- `RoutingTable` entries have a staleness deadline; routes older than the threshold are treated as unknown
- `dashmap` used for concurrent route/mutual-peer maps — no global lock

## Dependencies

- `indras-core` — `Packet`, `PeerIdentity`, `RoutingDecision`, `DropReason`, `RouteInfo`, `Priority`
- `indras-storage` — packet store for hold queue
- `tokio` + `async-trait` — async routing calls
- `dashmap` — lock-free concurrent maps
- `chrono` — staleness timestamps
- `thiserror` / `tracing`

## Testing

```bash
cargo test -p indras-routing
```

Stress tests live alongside unit tests. Use simulation identities (`SimulationIdentity::new('A')`) for deterministic peer IDs in tests — no crypto required.

## Gotchas

- `RoutingDecision` and friends are defined in `indras-core`, not here; import from `indras_routing` re-exports for convenience
- Mutual-peer data goes stale if `on_peer_connect` / `on_peer_disconnect` events are missed — topology drift causes RELAY decisions to fail silently and fall through to DROP
- `BackPropManager` state is in-memory only; a crash loses pending ACK paths
