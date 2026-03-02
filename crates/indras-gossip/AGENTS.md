# indras-gossip

Topic-based pub/sub gossip built on `iroh-gossip`. One topic per N-peer interface. Messages are signed for authenticity and delivered via split sender/receiver handles.

## Purpose

Provides a high-level gossip layer for broadcasting `InterfaceEvent`s within N-peer interfaces. Each interface maps to one iroh-gossip topic. The crate abstracts signing, serialization, and the iroh router integration behind a clean builder API.

## Module Map

```
src/
  lib.rs       — module declarations, re-exports, GOSSIP_ALPN re-export
  node.rs      — IndrasGossip, IndrasGossipBuilder: endpoint lifecycle, topic subscription
  topic.rs     — SplitTopic, TopicHandle, TopicReceiver: split send/recv handles
  message.rs   — SignedMessage, WireMessage, ReceivedMessage: signing and wire format
  events.rs    — GossipNodeEvent, SimpleGossipEvent: event enums for node and simple mode
  error.rs     — GossipError, GossipResult
```

## Key Types

- `IndrasGossip<I>` — main gossip node, generic over identity type `I: PeerIdentity`; holds the iroh-gossip handle and manages subscriptions
- `IndrasGossipBuilder` — fluent builder: set `secret_key`, then `build(&endpoint)` to get an `IndrasGossip`
- `SplitTopic` — returned by `gossip.subscribe(interface_id, bootstrap_peers).await`; contains a `sender` and `receiver`
- `TopicHandle` — the send side; call `.broadcast(&event).await` to sign and fan out a message
- `TopicReceiver` — the receive side; call `.recv().await` → `Option<Result<InterfaceEvent, GossipError>>`
- `SignedMessage` — on-wire message with signature; deserialized from raw gossip bytes
- `WireMessage` — the inner payload before signing; serialized with `postcard`
- `ReceivedMessage` — verified and deserialized message ready for application use
- `GossipNodeEvent` / `SimpleGossipEvent` — event enums wrapping iroh-gossip neighbor/message events

## Usage Pattern

```rust
// 1. Build endpoint and gossip node
let gossip: IndrasGossip<SimulationIdentity> = IndrasGossipBuilder::new()
    .secret_key(secret_key)
    .build(&endpoint);

// 2. Register ALPN with iroh router
let router = Router::builder(endpoint.clone())
    .accept(IndrasGossip::<SimulationIdentity>::alpn(), gossip.gossip().clone())
    .spawn();

// 3. Subscribe to an interface topic
let split = gossip.subscribe(interface_id, bootstrap_peers).await?;

// 4. Send from one task, receive in another
split.sender.broadcast(&event).await?;
while let Some(result) = split.receiver.recv().await { ... }
```

## Key Patterns

- Generic over `I: PeerIdentity` — works with `SimulationIdentity` in tests and real crypto keys in production
- `SplitTopic` is designed for concurrent tasks: clone or move `sender` to a write task, move `receiver` to a read task
- Message signing uses the `secret_key` provided at builder time; receivers verify signatures before delivering to the app
- Bootstrap peers are passed at subscribe time — iroh-gossip uses them to join the topic mesh
- `GOSSIP_ALPN` is re-exported from `iroh-gossip` so callers don't need an extra direct dependency

## Dependencies

- `indras-core` — `InterfaceId`, `InterfaceEvent`, `PeerIdentity`, `SimulationIdentity`
- `indras-transport` — transport-level iroh integration
- `iroh` + `iroh-gossip` — underlying gossip substrate
- `postcard` — compact binary serialization for `WireMessage`
- `serde` — derive macros
- `dashmap` — concurrent topic map
- `n0-future` — iroh ecosystem async utilities
- `tokio` + `async-trait`
- `rand` — key generation helpers

## Testing

```bash
cargo test -p indras-gossip
```

Stress tests use `SimulationIdentity` (single-letter peer IDs A, B, C…) to avoid crypto overhead. Bootstrap peer lists can be empty in single-node tests.

## Gotchas

- The `WorkerGuard` returned by iroh-gossip must be kept alive; dropping it shuts down background message processing
- `GOSSIP_ALPN` must be registered with the iroh `Router` before any topic subscriptions are made; subscribing without it causes silent message loss
- `SplitTopic.receiver` is not `Clone` — only one consumer per topic per node; fan-out to multiple consumers requires an application-level channel
- Signature verification failures surface as `Err(GossipError)` from `receiver.recv()`, not panics — handle them in the receive loop
