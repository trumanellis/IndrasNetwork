# indras-peering

Reusable P2P peering lifecycle for Indras Network applications.

## Purpose

Extracts the network lifecycle — contact polling, peer event broadcasting, world-view persistence, and graceful shutdown — into a UI-agnostic crate. Both standalone apps (`indras-chat`) and embedded consumers (`indras-workspace`) use the same `PeeringRuntime`.

## Architecture

```
PeeringRuntime
├── ContactPoller     (polls contacts realm, diffs peers, emits events)
├── EventForwarder    (forwards GlobalEvents from IndrasNetwork)
├── PeriodicSaver     (saves world view to disk on interval)
└── TaskSupervisor    (monitors task health, emits warnings)
```

### Construction Modes

- **`boot(config)`** — Standalone: creates `IndrasNetwork`, starts it, spawns tasks. Runtime owns the network and stops it on shutdown.
- **`create(name, pass_story, config)`** — Like `boot` but creates a brand-new identity first.
- **`attach(network, config)`** — Embedded: wraps an existing started network. Runtime does NOT own the network — shutdown only cancels tasks and saves state.

### Event System

Two reactive channels:
- **`broadcast::Sender<PeerEvent>`** — All events (peer changes, conversations, saves, warnings). Use `subscribe()` or `subscribe_with_snapshot()`.
- **`watch::Sender<Vec<PeerInfo>>`** — Latest peer list snapshot. Use `watch_peers()` for reactive updates.

`subscribe_with_snapshot()` atomically returns both a broadcast receiver and the current peer list, avoiding the race condition where peers connect between subscribe and initial read.

### Shutdown

`shutdown(&self)` is idempotent (guarded by `AtomicBool`):
1. Cancels all background tasks via `CancellationToken`
2. Joins task handles
3. Saves world view (best-effort)
4. Stops network (only if runtime owns it)

## Key Types

| Type | Description |
|------|-------------|
| `PeeringRuntime` | Central lifecycle manager |
| `PeerInfo` | Peer snapshot: member ID, display name, sentiment, status |
| `PeerEvent` | Event enum: PeerConnected, PeerDisconnected, PeersChanged, ConversationOpened, etc. |
| `PeeringConfig` | Data dir, poll interval, save interval |
| `PeeringError` | Typed errors: Network, AlreadyShutDown, ContactsRealmNotJoined, NoPeerInRealm |

## File Layout

| File | Responsibility |
|------|---------------|
| `runtime.rs` | `PeeringRuntime` struct, construction, peer ops, shutdown |
| `tasks.rs` | Background task spawning (poller, forwarder, saver, supervisor) |
| `event.rs` | `PeerEvent` enum, `PeerInfo` struct |
| `error.rs` | `PeeringError` enum |
| `config.rs` | `PeeringConfig` with defaults |
| `lib.rs` | Public re-exports |

## Dependencies

- **`indras-network`** — Lower-layer network, contacts realm, identity
- **`indras-sync-engine`** — Sentiment types (`SentimentView`)
- **`indras-crypto`** — `PassStory` for identity creation
- **`tokio`** / **`tokio-util`** — Async runtime, channels, cancellation
- **`futures`** — Stream utilities for event forwarding

## Consumers

- **`indras-chat`** — Standalone chat app, uses `boot()` or `create()`, owns the network
- **`indras-workspace`** — Embedded chat view, uses `attach()` with a shared network
