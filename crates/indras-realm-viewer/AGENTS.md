# indras-realm-viewer

## Purpose

Standalone Dioxus desktop app for replaying and visualizing realm-feature scenarios. Accepts
JSONL event streams from `lua_runner` via stdin or a `--file` path, then animates the full
state of a single realm in real time. Provides play/pause/step/reset playback controls and
a switchable skin (quiet-protocol / Botanical).

Two binaries:
- `realm-viewer` — single-realm dashboard (main entry point)
- `omni-viewer` — multi-scenario picker that lets you choose among multiple scenario outputs

## Module Map

```
src/
  main.rs          — realm-viewer entry point; clap args (--file, --theme); Dioxus launch;
                     two-phase stream loop (live ingestion → replay mode)
  omni_main.rs     — omni-viewer entry point; scenario picker UI
  lib.rs           — module declarations
  theme.rs         — Skin enum + CURRENT_SKIN global RwSignal; CSS class helpers
  playback.rs      — Global atomic playback controls: pause/play, step, reset, speed (delay_ms),
                     shutdown flag
  events/
    stream.rs      — StreamConfig (stdin | file), start_stream() → tokio channel of StreamEvent
  state/
    mod.rs         — AppState root; process_event() dispatch; reset()
    realm_state.rs — Realm membership, peer list, realm metadata
    quest_state.rs — Quest lifecycle: creation, claims, proof-of-service
    chat_state.rs  — Chat messages per realm
    contacts_state.rs   — Contact graph and trust relationships
    artifact_state.rs   — Artifact uploads and references
    attention_state.rs  — Attention scores and rankings
    document_state.rs   — Collaborative document sections
    proof_folder_state.rs — Proof folder contents and verification status
    token_state.rs      — Token balances and transfer events
  components/
    mod.rs         — App root component; tab/panel layout
    omni.rs        — OmniViewer root component
    scenario_picker.rs — Scenario selection list
```

## Key Patterns

- **Two-phase stream loop**: Phase 1 ingests live events from the stream into a `Mutex<Vec<StreamEvent>>`
  buffer while driving state forward. Phase 2 enters replay mode once the stream closes, replaying
  from an index with play/pause/step semantics.
- **Global playback state** (`playback.rs`): atomic booleans and a speed value shared between the
  async stream task and UI event handlers. No channels — just atomics polled at 50 ms intervals.
- **`OnceLock` for CLI args**: `FILE_PATH` and theme are stored in `OnceLock<_>` globals so the
  Dioxus async task can access them without prop-drilling.
- **Skin switching**: `CURRENT_SKIN` is a `RwSignal<Skin>` from `indras-ui`; CSS classes are
  applied at the root element so the entire tree re-themes reactively.
- **`indras-ui` shared CSS**: `SHARED_CSS` is imported from `indras-ui` and injected alongside the
  crate-local `styles.css` via `with_custom_head`.

## Dependencies

| Crate | Role |
|-------|------|
| `indras-ui` | Shared CSS tokens, Skin type, SHARED_CSS constant |
| `dioxus` (desktop) | UI framework and async runtime bridge |
| `tokio` | Async event stream reading (stdin / file) |
| `clap` | CLI argument parsing |
| `serde` / `serde_json` | JSONL event deserialization |
| `pulldown-cmark` | Markdown rendering in document/artifact views |
| `base64` | Artifact binary data display |
| `tracing` / `tracing-subscriber` | Structured logging to stderr |

## Testing

No unit tests in this crate — correctness is validated by running the viewer against simulation
output:

```bash
cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
    -- scripts/scenarios/<scenario>.lua | cargo run -p indras-realm-viewer --bin realm-viewer
```

Use `--file events.jsonl` for repeatable replay of a captured scenario.
