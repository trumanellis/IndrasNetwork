# indras-simulation

## Purpose

Core mesh-network simulation engine and Lua scenario runner. Models named peers (A–Z) with
store-and-forward routing across a configurable topology. Provides a discrete-time tick engine,
topology builders (ring, full, random, line, star), and a full Lua scripting layer
(`LuaRuntime`) that exposes the entire IndrasNetwork stack to scenario scripts. Contains 100+
Lua scenario scripts under `simulation/scripts/scenarios/`. Also ships a legacy interactive
binary (`indras-network`) for manual exploration.

Two binaries:
- `lua_runner` — primary entry point; runs a `.lua` scenario and emits JSONL events to stdout
  (pipe to any `*-viewer` crate)
- `indras-network` — legacy CLI with subcommands (abc, line, broadcast, chaos, partition,
  topology, interactive); used for low-level mesh inspection

## Module Map

```
src/
  lib.rs                 — Crate root; module declarations; re-exports of all public types
  types.rs               — PeerId, SealedPacket, PacketId, PeerInterface, PeerState, EventLog,
                           NetworkEvent, BackPropRecord, DropReason
  topology.rs            — Mesh, MeshBuilder; constructors: ring(), full_mesh(), random(),
                           line(), star(); visualize()
  simulation.rs          — Simulation, SimConfig, SimStats; run_ticks(), force_online(),
                           force_offline(), send_message(), state_summary()
  scenarios.rs           — Pre-built Rust scenarios: run_abc_scenario(), run_line_relay_scenario(),
                           run_broadcast_scenario(), run_random_chaos_scenario(),
                           run_partition_scenario()
  bridge.rs              — MeshBridge, SimulationRouter; connects simulation engine to
                           indras-network live networking layer
  main.rs                — indras-network binary; clap CLI (abc/line/broadcast/chaos/partition/
                           topology/interactive subcommands); indras-logging setup
  integration_scenarios.rs  — #[cfg(test)] integration test scenarios
  lua/
    mod.rs               — LuaRuntime re-export
    runtime.rs           — LuaRuntime: mlua Lua54 VM; registers all binding modules;
                           run_file() / run_string() entry points
    hooks.rs             — Lua lifecycle hooks (on_tick, on_event, etc.)
    assertions.rs        — Lua assert helpers for scenario verification
    bindings/
      mod.rs             — Registers all binding tables into the Lua globals
      simulation.rs      — lua.simulation.*: create_simulation, tick, get_stats
      live_network.rs    — lua.live_network.*: spawn live iroh/QUIC network
      live_node.rs       — lua.live_node.*: per-node operations on live network
      mesh.rs            — lua.mesh.*: topology construction from Lua
      routing.rs         — lua.routing.*: route inspection and manipulation
      sync_engine.rs     — lua.sync_engine.*: indras-sync document operations
      iot.rs             — lua.iot.*: IoT device simulation bindings
      pass_story.rs      — lua.pass_story.*: cryptographic story/credential bindings
      logging.rs         — lua.log.*: structured logging from Lua scripts
      correlation.rs     — lua.correlation.*: event correlation and tracing
      stats.rs           — lua.stats.*: metrics collection and reporting
      events.rs          — lua.events.*: JSONL event emission to stdout
      types.rs           — Shared Lua↔Rust type conversion helpers
  bin/
    lua_runner.rs        — lua_runner binary; loads and executes a .lua scenario file;
                           all JSONL output goes to stdout for viewer piping
```

## Key Types

| Type | Location | Description |
|------|----------|-------------|
| `Simulation` | `simulation.rs` | Discrete-time engine; holds Mesh + peer states + event log |
| `SimConfig` | `simulation.rs` | wake/sleep probabilities, trace_routing flag, tick limits |
| `SimStats` | `simulation.rs` | messages_sent/delivered/dropped, direct/relayed deliveries, backprops |
| `Mesh` | `topology.rs` | Adjacency map of PeerId → Peer; peer_ids(), visualize() |
| `MeshBuilder` | `topology.rs` | Fluent builder: `MeshBuilder::new(n).ring()` etc. |
| `LuaRuntime` | `lua/runtime.rs` | mlua Lua54 VM with all bindings registered |
| `MeshBridge` | `bridge.rs` | Bridges simulation mesh to live indras-network transport |
| `SimulationRouter` | `bridge.rs` | Implements indras-routing traits for simulated peers |
| `PeerId` | `types.rs` | Newtype over `char` (A–Z); `PeerId::new(c)` validates range |
| `SealedPacket` | `types.rs` | Encrypted packet held by a relay for an offline destination |

## Lua Scripting Layer

Scenarios are `.lua` files that call into the bindings via global tables:

```lua
-- Typical scenario structure
local sim = simulation.create({ peers = {"A","B","C"}, topology = "ring" })
simulation.force_online(sim, "A")
simulation.force_online(sim, "B")
sync_engine.create_document(sim, "A", { title = "Quest 1" })
simulation.tick(sim, 10)
local stats = stats.get(sim)
assert(stats.messages_delivered > 0, "expected delivery")
events.emit("scenario_complete", { convergence = true })
```

All `events.emit()` calls write JSONL lines to stdout, which the viewer crates consume.

## Key Patterns

- **Peer naming convention**: always single uppercase letters (A, B, C, …). `PeerId('A')`.
- **JSONL to stdout**: `lua_runner` scenarios emit structured events via `events.emit()`;
  viewers read these via stdin pipe. stderr is reserved for logs.
- **Store-and-forward**: messages to offline peers are sealed at the sender and held in a
  relay queue; delivered when the destination peer comes online.
- **Back-propagation**: delivery confirmations travel back through the relay chain and are
  recorded as `BackPropRecord` entries.
- **Topology builders**: always use `MeshBuilder`; never construct `Mesh` directly.
- **Lua 5.4**: `mlua` is configured with `features = ["lua54", "vendored", "serialize", "async"]`.
  Async Lua coroutines are supported for live-network scenarios.

## Dependencies

| Crate | Role |
|-------|------|
| `indras-core` | `SimulationIdentity` and core traits |
| `indras-routing` | Routing traits implemented by `SimulationRouter` |
| `indras-storage` | Storage backend for simulated nodes |
| `indras-crypto` | Cryptographic primitives for sealed packets |
| `indras-sync` | CRDT document sync (exposed via `sync_engine` Lua bindings) |
| `indras-logging` | `IndrasSubscriberBuilder`, JSONL file logging |
| `indras-dtn` | Delay-tolerant networking primitives |
| `indras-node` | Node lifecycle management |
| `indras-network` | Live iroh/QUIC network (used by `live_network` bindings) |
| `indras-iot` | IoT device simulation (exposed via `iot` Lua bindings) |
| `mlua` | Lua 5.4 VM (vendored) |
| `iroh` / `iroh-gossip` | Real networking substrate for live-network scenarios |
| `automerge` | CRDT for synced peer interfaces |
| `tokio` | Async runtime |
| `clap` | CLI argument parsing |
| `serde` / `serde_json` / `postcard` | Serialization |
| `uuid`, `blake3`, `hex`, `base64` | Crypto and ID utilities in Lua bindings |

## Testing

```bash
# Run all simulation tests
cargo test -p indras-simulation

# Run a specific Lua scenario and view output
cargo run --bin lua_runner -- scripts/scenarios/sync_engine_home_realm_stress.lua

# Pipe to a viewer
cargo run --bin lua_runner -- scripts/scenarios/realm_quest_cycle.lua \
    | cargo run -p indras-realm-viewer --bin realm-viewer

# Run a built-in Rust scenario
cargo run -p indras-simulation -- abc
cargo run -p indras-simulation -- chaos --ticks 200
```

Integration scenarios live in `src/integration_scenarios.rs` and are gated with `#[cfg(test)]`.
