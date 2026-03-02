# indras-logging

Multi-instance structured logging with JSONL output, optional pretty console, file rotation, per-peer context injection, cross-instance correlation IDs, and OpenTelemetry integration.

## Purpose

Provides the logging infrastructure for distributed Indras nodes. Because many peer instances run concurrently (often in the same process during simulation), the crate injects peer identity into every log line and provides correlation IDs to trace a single packet's path across multiple hops.

## Module Map

```
src/
  lib.rs         — IndrasSubscriberBuilder, init_default/development/testing helpers,
                   shutdown(), top-level re-exports
  config.rs      — LogConfig, ConsoleConfig, FileConfig, JsonlConfig, OtelConfig,
                   RotationStrategy
  context.rs     — PeerContextGuard, PeerContextData, PeerType: per-peer thread-local state
  correlation.rs — CorrelationContext, CorrelationExt, fields mod, spans mod
  layers.rs      — PeerContextLayer: tracing-subscriber layer that injects peer fields
  otel.rs        — init_otel_layer(), shutdown_otel(): OpenTelemetry OTLP setup
```

## Key Types

- `IndrasSubscriberBuilder` — fluent builder; configure then call `.init()` to set the global subscriber
- `LogConfig` — combines `ConsoleConfig`, `FileConfig` (optional), `JsonlConfig`, `OtelConfig`; three presets: `default()`, `development()`, `testing()`
- `PeerContextGuard` — RAII guard; set before any logging in a peer's task scope; dropped automatically at scope exit
- `PeerContextData` / `PeerType` — data stored in the guard: peer ID string, peer type tag
- `CorrelationContext` — create at message origin with `new_root()`; call `.child()` when relaying; carries `trace_id` and `span_id`
- `CorrelationExt` — extension trait providing correlation helpers on spans/events
- `OtelConfig` — endpoint, service name, sampling ratio for OTLP export
- `RotationStrategy` — `Never` (single file, truncated on start), `Daily`, `Hourly`

## Usage

### Quick setup

```rust
// Production: JSONL to console (default)
IndrasSubscriberBuilder::new().init();

// Development: compact pretty console, debug level
IndrasSubscriberBuilder::new()
    .with_config(LogConfig::development())
    .init();

// Testing: minimal, won't panic on double-init
indras_logging::init_testing();
```

### Per-peer context

```rust
let peer = SimulationIdentity::new('A').unwrap();
let _guard = PeerContextGuard::new(&peer);
// All tracing calls in this scope include peer_id = "A"
tracing::info!("Processing packet");
```

### Correlation across hops

```rust
// At origin
let ctx = CorrelationContext::new_root().with_packet_id("0041#3");
tracing::info!(trace_id = %ctx.trace_id, span_id = %ctx.span_id, "Sending");

// At relay
let child = ctx.child();
tracing::info!(trace_id = %child.trace_id, parent_span_id = %ctx.span_id, "Relaying");
```

### File output with rotation

```rust
IndrasSubscriberBuilder::new()
    .with_file_output(FileConfig {
        directory: PathBuf::from("logs"),
        prefix: "indras".into(),
        rotation: RotationStrategy::Daily,
    })
    .init();
```

## Output Modes

The builder selects a layer combination at init time based on (console_enabled, pretty, file, otel):

| Console | Pretty | File | OTel | Result |
|---------|--------|------|------|--------|
| true | false | false | false | **Default**: JSONL to stdout |
| true | true | false | false | Compact pretty to stdout |
| true | * | true | * | Console + JSONL file |
| * | * | * | true | Adds OTel OTLP export layer |
| false | * | true | false | File only |

## WorkerGuard Lifetime

`IndrasSubscriberBuilder::init()` returns `Option<WorkerGuard>`. The guard must be kept alive for the entire program lifetime when file output is enabled — dropping it flushes and closes the file writer. Assign it to a variable in `main`:

```rust
let _guard = IndrasSubscriberBuilder::new()
    .with_file_output(file_config)
    .init();
```

## Dependencies

- `indras-core` — `PeerIdentity`, `SimulationIdentity`
- `tracing` / `tracing-subscriber` (env-filter, json features)
- `tracing-appender` 0.2 — `RollingFileAppender`, `NonBlocking`
- `tracing-opentelemetry` 0.28 + `opentelemetry` 0.27 + `opentelemetry_sdk` (rt-tokio) + `opentelemetry-otlp` (tonic)
- `uuid` 1.0 (v4, serde) — correlation ID generation
- `parking_lot` — fast RwLock for peer context storage
- `serde` / `serde_json`, `chrono`, `thiserror`

## Testing

```bash
cargo test -p indras-logging
```

Use `init_testing()` in tests — it calls `try_init()` so double-initialization across tests doesn't panic. Stress tests verify concurrent peer context injection doesn't bleed between tasks.

## Gotchas

- `init()` panics on double-call; use `try_init()` or `init_testing()` in tests and multi-crate integration setups
- `RotationStrategy::Never` truncates the log file on startup — intentional for clean test runs, but destructive in production restarts; use `Daily` or `Hourly` for production
- OTel initialization failure is non-fatal: the builder logs a warning to stderr and continues without the OTel layer — check startup logs if traces aren't appearing in Jaeger/Zipkin
- `PeerContextLayer` injects fields into every log event in the scope; forgetting to create a `PeerContextGuard` results in logs with no `peer_id` field, making multi-instance correlation impossible
- `shutdown()` must be called before process exit to flush OTel spans; spans buffered in the OTLP exporter are lost if the process exits without it
