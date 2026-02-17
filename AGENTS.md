# IndrasNetwork — AI Agent Guide

## What This Is

A Rust workspace implementing a peer-to-peer network with CRDT-based sync, artifact sharing, and an attention economy. Single import surface via `indras-network`; app layer via `indras-sync-engine`.

## Crate Dependency Layers

```
Layer 5 (apps):     indras-sync-engine, indras-dashboard, indras-workspace, viewers, examples
Layer 4 (SDK):      indras-network (single import surface)
Layer 3 (node):     indras-node
Layer 2 (services): indras-sync, indras-messaging, indras-gossip, indras-routing, indras-dtn
Layer 1 (infra):    indras-transport, indras-storage, indras-crypto, indras-logging, indras-iot
Layer 0 (core):     indras-core, indras-artifacts
```

## Crate Directory

| Crate | Purpose |
|-------|---------|
| `indras-core` | Traits and types: `PeerIdentity`, `NInterfaceTrait`, `InterfaceEvent`, `Clock` |
| `indras-crypto` | Ed25519 + ML-DSA-65 signing, ML-KEM-768 key exchange, Argon2id |
| `indras-transport` | iroh-based networking adapter (`IrohNetworkAdapter`) |
| `indras-storage` | `CompositeStorage`: redb + blob store + event log |
| `indras-routing` | Prophet-based delay-tolerant routing |
| `indras-gossip` | Gossip protocol over iroh-gossip |
| `indras-sync` | CRDT sync: `InterfaceDocument`, `ArtifactDocument`, `HeadTracker`, `RawSync` |
| `indras-messaging` | Message routing and delivery (`MessagingClient`, `MessageHistory`) |
| `indras-dtn` | Delay-tolerant networking with custody transfer |
| `indras-iot` | IoT device support |
| `indras-logging` | Structured logging |
| `indras-node` | `IndrasNode` — P2P node managing interfaces, peers, transport |
| `indras-network` | **SDK** — single import surface for apps: realms, documents, contacts, artifacts |
| `indras-sync-engine` | App layer: quests, blessings, tokens, attention, humanness attestation |
| `indras-artifacts` | Domain types: `Artifact`, `Vault`, `Story`, `AccessMode`, attention economy |
| `indras-dashboard` | Terminal dashboard UI |
| `indras-workspace` | Workspace management |
| `indras-ui` | Shared UI components |
| `indras-genesis` | Network genesis / bootstrapping |
| viewers | `indras-home-viewer`, `indras-realm-viewer`, `indras-collaboration-viewer` |

## Where to Find Things

| Looking for... | Go to... |
|----------------|----------|
| Public API for apps | `crates/indras-network/src/lib.rs` |
| Domain types (Artifact, Vault, etc.) | `crates/indras-artifacts/src/` |
| CRDT sync internals | `crates/indras-sync/src/` |
| App-layer features (quests, blessings) | `crates/indras-sync-engine/src/` |
| Simulation scenarios | `simulation/scripts/scenarios/` |
| Shell scripts for running | `scripts/` |
| Developer guide | `articles/indras-network-developers-guide.md` |
| Example apps | `examples/chat-app/`, `examples/sync-demo/`, `examples/indras-notes/` |

## Key Architectural Patterns

- **BLAKE3 deterministic IDs**: Realms, inboxes, home realms, artifact sync interfaces all derive IDs via BLAKE3 with domain prefixes
- **Gossip-per-artifact**: Each shared artifact gets its own gossip topic for sync
- **CRDT documents**: `Document<T>` wraps Automerge with typed Rust structs
- **Extension traits**: `indras-sync-engine` adds domain methods to `Realm` via traits like `RealmQuests`, `RealmBlessings`
- **Single-letter peer IDs**: Use A, B, C... in examples and simulations

## Documentation Conventions

- Every public type and function must have a `///` doc comment
- When adding a new module to a crate, update that crate's `AGENTS.md`
- When changing `indras-network` public API, update the developer guide
- `AGENTS.md` files should be 50–150 lines — architectural context, not API reference
