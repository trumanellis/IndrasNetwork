# indras-iot

IoT-specific optimizations for resource-constrained and battery-powered devices. Provides
duty-cycling power state management, a compact binary wire format for low-bandwidth links
(LoRa, BLE), and memory budget enforcement for embedded systems with limited RAM.

This crate is intentionally dependency-light: it does not depend on indras-storage or
indras-sync, only on indras-core, indras-transport, and indras-routing.

## Module Map

| Module | Role |
|---|---|
| `lib.rs` | Re-exports the three public modules |
| `duty_cycle.rs` | `DutyCycleManager`, `DutyCycleConfig`, `PowerState` — wake/sleep scheduling |
| `compact.rs` | `CompactMessage`, `Fragmenter` — bandwidth-efficient binary wire format |
| `low_memory.rs` | `MemoryTracker`, `BufferPool`, `MemoryBudget` — heap budget enforcement |

## Key Types

- **`DutyCycleManager`** — tick-driven state machine cycling `Active → PreSleep → Sleeping → Waking`
- **`DutyCycleConfig`** — `active_duration`, `sleep_duration`, `min_sync_interval`,
  `max_pending_before_wake`, `low_battery_threshold`; presets: `default()` (10% duty),
  `low_power()` (~2% duty), `responsive()` (50% duty)
- **`PowerState`** — `Active | PreSleep | Sleeping | Waking`
- **`CompactMessage`** — binary frame: `[type:1][flags:1][seq:varint][len:varint][payload][crc8:1]`
- **`CompactMessageType`** — `Ping, Pong, Data, Ack, SyncRequest, SyncResponse, Presence`
- **`Fragmenter`** — splits oversized `CompactMessage` payloads for constrained MTUs
- **`MemoryTracker`** — thread-safe atomic budget tracker; returns RAII guards for memory,
  connections, and pending ops
- **`MemoryBudget`** — presets: `default()` (64 KB), `minimal()` (16 KB), `moderate()` (128 KB)
- **`BufferPool`** — pre-allocated fixed-size buffer pool for zero-heap-allocation message handling

## Key Patterns

**Duty cycling:** call `manager.tick()` in your main loop to drive state transitions. The
manager transitions automatically based on elapsed time. Low battery doubles the sleep
duration automatically when `battery_level < low_battery_threshold`. Reaching
`max_pending_before_wake` messages while sleeping triggers a forced `wake()`.

```rust
let mut mgr = DutyCycleManager::new(DutyCycleConfig::default());
loop {
    mgr.tick();
    if mgr.should_allow_operation(is_urgent) {
        // process message
    }
    if mgr.should_sync() {
        // perform sync, then:
        mgr.record_sync();
    }
}
```

**Compact wire format:** `CompactMessage::encode()` / `decode()` for all I/O. CRC-8-CCITT
covers the header + payload for error detection. For MTU-constrained links use `Fragmenter`:
fragment index is packed into high 16 bits of `sequence`, original sequence in low 16 bits.

**Memory budgeting:** acquire RAII guards before allocating; they release automatically on drop.
`MemoryTracker` uses compare-and-swap so it is safe to share as `Arc<MemoryTracker>`.

```rust
let tracker = Arc::new(MemoryTracker::new(MemoryBudget::minimal()));
let _guard = tracker.try_allocate(512)?;   // freed on drop
let _conn  = tracker.try_add_connection()?;
let _op    = tracker.try_queue_op()?;
```

## Gotchas

- `DutyCycleManager` is **not** `Send`/`Sync`. It uses `&mut self` for all state transitions.
  Wrap in `Arc<Mutex<DutyCycleManager>>` to share across threads.
- `PreSleep` state is hardcoded to 5 seconds and `Waking` to 2 seconds; these are not
  configurable via `DutyCycleConfig`.
- `CompactMessage::MAX_PAYLOAD_SIZE` is 65536 bytes. Decode rejects larger claimed sizes to
  prevent memory exhaustion from malicious peers.
- CRC-8 provides **error detection only**, not cryptographic integrity. For security-sensitive
  IoT use cases, layer an HMAC or authenticated encryption on top.
- `Fragmenter::fragment` panics if the message requires more than 65535 fragments or if
  `max_fragment_size` is zero. Validate inputs before calling.
- `BufferPool::try_acquire` takes `&mut self` (not thread-safe). `MemoryTracker` is the
  thread-safe alternative for tracking allocations across threads.
- `too_many_pending` error fires at `2 × max_pending_before_wake`, not at the threshold itself.

## Dependencies

Internal: `indras-core`, `indras-transport`, `indras-routing`

External: `tokio`, `async-trait`, `thiserror`, `tracing`

No serde dependency — the compact format uses hand-rolled binary encoding, not postcard/JSON.

## Testing

All tests are inline (`#[cfg(test)]` blocks). The duty cycle tests use `std::thread::sleep`
for short durations (10–15 ms) to exercise real state transitions; keep these tests isolated
from async test harnesses to avoid executor interference.

```bash
cargo test -p indras-iot
```
