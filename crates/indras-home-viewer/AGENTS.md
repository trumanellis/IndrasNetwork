# indras-home-viewer

## Purpose

Standalone Dioxus desktop app that plays back home-realm scenario events from a single user's
perspective. Reads JSONL events from stdin (pipe from `lua_runner`) or a `--file` path.
Presents quests, notes, artifacts, session stats, and an activity feed in a sidebar/panel
layout. Supports member filtering (`--member`) to isolate one user's view, adjustable playback
speed, and an optional `--autoplay` flag (defaults to starting paused).

One binary: `indras-home-viewer`

## Module Map

```
src/
  main.rs          — Entry point; clap args (--file, --member, --speed, --autoplay);
                     OnceLock globals for file path and member filter; Dioxus desktop launch;
                     two-phase stream loop (live ingestion → replay mode); process_event() shim
  lib.rs           — Module declarations
  theme.rs         — Skin enum and theming helpers (Cormorant Garamond + JetBrains Mono fonts)
  playback.rs      — Atomic playback controls: pause/play, step, reset, speed (delay_ms),
                     shutdown flag
  events/
    stream.rs      — HomeRealmEvent enum (JSONL variants); StreamConfig {file_path, member_filter};
                     start_stream() → tokio channel; event_buffer() global Mutex<Vec<HomeRealmEvent>>
  state/
    mod.rs         — AppState root; process_event() dispatch; reset()
    app_state.rs   — Top-level state aggregating all sub-states
    session_state.rs   — Active session metadata (member, realm, start time)
    quests_state.rs    — Quest list with status, progress, and completion tracking
    notes_state.rs     — Notes/journal entries
    artifacts_state.rs — Uploaded artifacts with mime type and size
  components/
    mod.rs         — Module re-exports; App root component
    app.rs         — App layout: sidebar + main content area
    sidebar.rs     — Left sidebar: member identity, nav links, quick stats
    quests_panel.rs — Quest list with status badges and detail expansion
    notes_panel.rs  — Notes/journal panel with markdown preview
    artifacts.rs   — Artifact grid with type icons and metadata
    stats_panel.rs — Session statistics (messages, storage, uptime)
    activity.rs    — Chronological activity feed of all events
    content.rs     — Main content routing between panels
```

## Key Patterns

- **Two-phase stream loop**: identical pattern to `indras-realm-viewer` — Phase 1 ingests live
  events into a `Mutex<Vec<HomeRealmEvent>>` buffer and drives state; Phase 2 replays from an
  index once the stream closes, with play/pause/step/reset semantics at 50 ms poll intervals.
- **Member filter**: `StreamConfig::member_filter` causes the stream reader to skip events not
  belonging to the specified member, giving a true first-person view.
- **OnceLock globals**: `FILE_PATH` and `MEMBER_FILTER` stored in `OnceLock<Option<_>>` so the
  Dioxus async resource can read them without prop-drilling.
- **Starts paused by default**: `playback::set_paused(!args.autoplay)` — the viewer waits for
  the user to press play unless `--autoplay` is passed.
- **No internal crate deps**: completely standalone — only external crates (dioxus, tokio, etc.).
  All event types are defined locally in `events/stream.rs`.

## Dependencies

| Crate | Role |
|-------|------|
| `dioxus` (desktop) | UI framework |
| `tokio` | Async stdin/file reading |
| `clap` | CLI argument parsing (`--file`, `--member`, `--speed`, `--autoplay`) |
| `serde` / `serde_json` | JSONL event deserialization |
| `tracing` / `tracing-subscriber` | Structured logging |

No workspace crates are referenced — this viewer is fully self-contained.

## Testing

Correctness validated by running against simulation output:

```bash
# Pipe directly from lua_runner
cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
    -- scripts/scenarios/home_realm.lua | cargo run -p indras-home-viewer

# Filter to a single member
cargo run -p indras-home-viewer -- --member A

# Read from file, autoplay at 2x speed
cargo run -p indras-home-viewer -- --file events.jsonl --speed 2.0 --autoplay
```

See also `scripts/run-home-viewer.sh` for a convenience wrapper.
