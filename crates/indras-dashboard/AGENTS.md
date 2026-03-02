# indras-dashboard

Real-time Dioxus desktop dashboard for monitoring simulation stress tests. Watches JSONL
log files via the `notify` file-watcher, parses structured log entries, and displays
charts, sync-engine metrics, discovery statistics, and document counts. Includes a
control bar for triggering simulation scenarios. Binary only — no library target.

## Module Map

```
src/
  main.rs           — Dioxus desktop launch, CSS injection, window config
  app.rs            — App root component; composes layout and top-level state signal
  layout.rs         — Layout — panel grid, responsive column arrangement
  theme.rs          — theme CSS constant (loaded from assets/themes.css)

  state/
    mod.rs          — re-exports all state types; UnifiedState aggregates sub-states
    unified.rs      — UnifiedState: master signal merging all sub-state structs
    instance.rs     — InstanceState: per-node metrics (peer count, uptime, memory)
    discovery.rs    — DiscoveryState: pkarr/DNS stats, peer discovery events
    document.rs     — DocumentState: document counts, sync round-trip times
    sync_engine.rs  — SyncEngineState: operation counts, conflict rates, latency histograms

  components/
    mod.rs          — re-exports all components
    panels.rs       — Panels — top-level panel switcher / tab bar
    charts.rs       — Charts — time-series line charts for metrics
    control_bar.rs  — ControlBar — scenario selector + run/stop buttons
    discovery.rs    — DiscoveryPanel — discovery event feed and peer map
    documents.rs    — DocumentsPanel — document count table and sync latency
    sync_engine.rs  — SyncEnginePanel — operation rate, conflict rate, latency histogram

  runner/
    mod.rs          — re-exports runner types
    document_runner.rs — DocumentRunner: spawns scenario subprocesses, captures stdout,
                         feeds parsed events into UnifiedState

assets/
  themes.css        — dark/light theme CSS variables
  style.css         — dashboard layout and component styles
```

## Key Types

- `UnifiedState` — single Dioxus signal wrapping all sub-state structs; updated by the
  file-watcher loop and `DocumentRunner`; components read from it reactively
- `InstanceState` — per-simulation-node snapshot: peer count, CPU/memory if available,
  uptime seconds
- `DiscoveryState` — rolling window of pkarr lookup durations, DNS query counts,
  newly discovered peer events
- `SyncEngineState` — operation throughput (ops/sec), conflict count, latency percentiles
  (p50/p95/p99) computed from the JSONL stream
- `DocumentState` — document creation/update counts and per-document sync round-trip times
- `DocumentRunner` — manages child processes for simulation scenarios; reads their stdout
  as JSONL and dispatches parsed `LogEntry` values into state signals
- `ControlBar` component — lets the user select a scenario from a dropdown (populated from
  `indras-simulation`) and start/stop the runner

## Key Patterns

- File watching: `notify` watcher runs in a Tokio task, emits file-change events;
  the task reads new lines from the JSONL file and parses them with `serde_json`
- State update: parsed log entries are pattern-matched on their `kind` field and routed
  to the appropriate sub-state struct; all writes go through the `UnifiedState` signal
- Chart rendering: `Charts` component reads time-series vecs from state and renders SVG
  paths directly in Dioxus markup (no chart library dependency)
- Scenario integration: `indras-simulation` crate provides the list of available Lua
  scenarios; `DocumentRunner` invokes them via `cargo run --bin lua_runner`
- CSS: two CSS files (`themes.css`, `style.css`) are embedded at compile time with
  `include_str!` and injected via `with_custom_head` in `main.rs`

## Dependencies

| Crate | Role |
|---|---|
| `dioxus` (0.7, desktop) | UI framework |
| `tokio` | Async runtime, file-watch task |
| `notify` (v6) | File system event watcher for JSONL logs |
| `serde` / `serde_json` | JSONL log entry deserialisation |
| `regex` | Log line pattern matching / field extraction |
| `glob` | Discover log files matching a path pattern |
| `chrono` | Timestamp parsing and display |
| `image` (png) | App icon loading |
| `indras-logging` | Shared log entry type definitions |
| `indras-simulation` | Scenario list and Lua runner integration |

## Testing

No automated tests. Run `cargo run -p indras-dashboard` from the repo root while a
simulation scenario is active (e.g. via `scripts/run-home-viewer.sh`) and verify that
metrics update in real time, charts scroll, and the control bar can start/stop scenarios.
Check that the file-watcher picks up new JSONL entries within ~500 ms of the log write.
