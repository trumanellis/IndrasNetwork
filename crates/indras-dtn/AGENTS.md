# indras-dtn

Delay-Tolerant Networking for sparse and intermittent networks. Extends base Indras routing with bundle protocol semantics, probabilistic routing, custody transfer, and age-based priority demotion.

## Purpose

DTN handles network conditions where normal store-and-forward is insufficient: long delays, frequent disconnections, or no contemporaneous end-to-end path. Packets are wrapped in `Bundle`s with lifetime metadata and routed using epidemic, spray-and-wait, or PRoPHET strategies.

## Module Map

```
src/
  lib.rs        — DtnConfig, ConfigWarning, module declarations, re-exports
  bundle.rs     — Bundle, BundleId, BundleSummary, ClassOfService, CustodyTransfer
  custody.rs    — CustodyManager, CustodyConfig, CustodyMessage, CustodyRecord,
                  CustodyTransferResult, PendingCustodyTransfer, RefuseReason, ReleaseReason
  epidemic.rs   — EpidemicRouter, EpidemicConfig, EpidemicDecision, SuppressReason
  expiration.rs — AgeManager, ExpirationConfig, ExpirationRecord
  prophet.rs    — ProphetState, ProphetConfig, ProphetSummary
  strategy.rs   — StrategySelector, StrategyRule, StrategyCondition, DtnStrategy
  error.rs      — DtnError, BundleError, CustodyError, DtnResult
```

## Key Types

- `Bundle` — wraps a `Packet` with lifetime, class-of-service, and custody metadata; created via `Bundle::from_packet(packet, lifetime)`
- `BundleId` / `BundleSummary` — identity and compact summary for routing decisions
- `EpidemicRouter` — flood-based routing with spray-and-wait mode; tracks seen bundles to suppress duplicates
- `CustodyManager` — explicit responsibility transfer; a node that accepts custody promises to deliver or re-transfer the bundle
- `ProphetState` — maintains per-peer delivery probability estimates using encounter history; updated on each contact
- `AgeManager` — periodic cleanup of expired bundles; demotes priority based on age thresholds
- `StrategySelector` — picks routing strategy (`Epidemic`, `SprayAndWait`, `StoreAndForward`) based on `StrategyRule` conditions
- `DtnConfig` — top-level config combining custody, epidemic, and expiration configs; four presets available

## DtnConfig Presets

| Preset | Strategy | Use Case |
|--------|----------|----------|
| `DtnConfig::default()` | SprayAndWait(4) | General P2P |
| `DtnConfig::low_latency()` | StoreAndForward | Connected networks |
| `DtnConfig::challenged_network()` | Epidemic, 16 copies | Sparse/intermittent |
| `DtnConfig::resource_constrained()` | SprayAndWait(2) | Low-memory nodes |

Call `config.validate()` to get a `Vec<ConfigWarning>` before using a custom config.

## Routing Strategies

- **Epidemic** — floods to every encountered peer; maximizes delivery probability at the cost of bandwidth
- **SprayAndWait** — distributes exactly N copies, then waits; `spray_count` copies on first contact, remaining copies forwarded one-by-one
- **StoreAndForward** — holds until a direct path to destination is available; conservative

Strategy selection can be static (set `default_strategy` in config) or dynamic via `StrategySelector` with rules evaluated against network conditions.

## Custody Transfer

`CustodyManager` implements explicit handoff:

1. Sender offers custody to a relay node (`CustodyMessage::Request`)
2. Relay accepts (`CustodyTransferResult::Accepted`) or refuses with `RefuseReason`
3. Accepting node takes responsibility; original sender can delete its copy
4. On delivery, custody is released (`ReleaseReason`) back up the chain

`accept_from_unknown: false` in `CustodyConfig` makes resource-constrained nodes selective about which peers they accept custody from.

## Age-Based Expiration

`AgeManager` runs on a `cleanup_interval` and:
- Expires bundles past their lifetime (`ExpirationRecord` removed)
- Demotes `Priority` at configurable age thresholds (e.g., after 5 min → Normal, after 15 min → Low)

Default demotion thresholds (300s → Normal, 900s → Low) prevent stale bundles from blocking fresh ones.

## Dependencies

- `indras-core` — `Packet`, `PeerIdentity`, `Priority`
- `serde` — bundle serialization
- `dashmap` — concurrent bundle and custody maps
- `chrono` — lifetime and age tracking
- `thiserror` / `tracing`

Dev dependencies:
- `indras-routing` — integration tests that combine DTN and base routing
- `criterion` — benchmarks (`benches/dtn_benchmarks.rs`)

## Testing

```bash
# Unit and integration tests
cargo test -p indras-dtn

# Benchmarks
cargo bench -p indras-dtn
```

Benchmarks cover epidemic routing throughput and custody handoff latency. Fault-injection tests (in `tests/`) simulate node crashes mid-custody-transfer.

## Gotchas

- `EpidemicRouter` tracks seen bundle IDs in memory; the `seen_timeout` prevents unbounded growth but must be longer than maximum network round-trip or bundles get re-flooded
- `spray_count` must be ≤ `max_copies`; `DtnConfig::validate()` catches this but only at runtime, not compile time
- `AgeManager` cleanup runs on a timer — it does not expire bundles on read; a bundle past its lifetime may still be returned until the next cleanup tick
- PRoPHET (`ProphetState`) requires encounter history to be meaningful; fresh nodes with no history fall back to epidemic behavior
- `Bundle::from_packet` uses `chrono::Duration`, not `std::time::Duration` — watch for the type mismatch
