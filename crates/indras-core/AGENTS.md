# indras-core

Foundation crate. Defines the shared abstractions every other crate builds on. Contains no
networking, crypto, or storage logic — only traits, types, and errors that allow simulation
and real-network code to share the same routing and messaging logic.

## Purpose

Decouple peer identity (simulation chars vs. real PublicKey) from all higher-level logic.
The same routing, event, and interface code runs in tests using `SimulationIdentity` and in
production using `IrohIdentity` without modification.

## Module Map

| Module | Contents |
|---|---|
| `identity` | `PeerIdentity` trait, `SimulationIdentity` (char wrapper), marker types |
| `event` | `InterfaceEvent`, `NetworkEvent`, event kind enums |
| `packet` | `Packet` (sealed store-and-forward unit), `EventId` |
| `interface` | `InterfaceId` (UUID-backed), `NInterfaceTrait`, interface membership |
| `routing` | `NetworkTopology` trait, route resolution |
| `transport` | `PacketStore` trait, `Clock` trait for testable time |
| `traits` | `NInterfaceTrait` — the main N-peer shared interface abstraction |
| `mock_transport` | In-memory transport stub for unit tests |
| `error` | `CoreError`, top-level `Result` alias |

## Key Types

- **`PeerIdentity`** — sealed trait abstracting over identity representations; implement for
  sim (`char`) and real (`iroh::PublicKey`).
- **`InterfaceId`** — UUID v4 that uniquely names an N-peer interface across the network.
- **`InterfaceEvent`** — typed events flowing through an interface: messages, membership
  changes, presence updates.
- **`Packet`** — opaque sealed unit for store-and-forward; carries `EventId` + encrypted bytes.
- **`EventId`** — `(sender_index: u32, sequence: u64)` pair; total ordering within a sender.
- **`NetworkTopology`** — trait for querying neighbours, reachability, and routing next-hops.
- **`NInterfaceTrait`** — async trait that higher-level realms implement; append events,
  read history, manage membership.
- **`Clock`** — time abstraction injected into types needing timestamps; test impl uses
  `tokio::time` manual advance.

## Key Patterns

- All public types derive `serde::{Serialize, Deserialize}` + `postcard` for wire encoding.
- `PeerIdentity` is a sealed trait — only crates that impl it (core itself for sim, transport
  for iroh) produce concrete identity values.
- `EventId` ordering: sort by `(sender_index, sequence)` to reconstruct causal order within
  one sender; cross-sender ordering is left to the interface layer.
- `mock_transport` wires up an in-memory channel pair so unit tests can exercise full
  round-trips without any network stack.

## Gotchas

- `SimulationIdentity` accepts only ASCII letters A–Z. Passing other chars returns an error.
- `InterfaceId::generate()` is the only constructor — no deserialization constructor exists
  to prevent accidental ID reuse.
- `PacketStore` is defined here (not in `indras-storage`) so lower-level crates can depend on
  the trait without pulling in the full storage stack.
- All re-exports via `pub use module::*` in `lib.rs` — prefer importing from the crate root
  rather than from submodule paths.

## Dependencies

No other indras-* crates. External deps:

| Crate | Use |
|---|---|
| `serde` + `postcard` | Serialization / wire format |
| `tokio` (sync, time) | Async channels, `Clock` impl |
| `dashmap` | Concurrent hash maps in routing/membership |
| `uuid` | `InterfaceId` backing |
| `chrono` | Timestamps on events |
| `thiserror` | `CoreError` variants |
| `bytes` | Zero-copy `Packet` payload |
| `rand` | `InterfaceId::generate()`, test helpers |

## Testing

```bash
cargo test -p indras-core
```

- Unit tests live alongside each module in `#[cfg(test)]` blocks.
- Use `SimulationIdentity::new('A')` and `mock_transport` to avoid any network I/O.
- `tokio-test` in dev-deps provides `block_on` for synchronous test wrappers.
- No integration tests — higher-level crates test the assembled stack.
