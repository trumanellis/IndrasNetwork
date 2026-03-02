# indras-collaboration-viewer

## Purpose

Standalone Dioxus desktop app that visualizes the `collaboration_trio` scenario step-by-step.
The scenario is hard-coded as a 50-tick state machine (no external event stream). Peers A, B,
and C move through five phases: Setup, Quest Creation, Document Collaboration, Quest Updates,
and Verification. An auto-play loop advances ticks at configurable speed; the user can also
step manually, pause, or reset. Supports a POV dashboard mode where clicking a peer switches
to a first-person view of that peer's state.

One binary: `collaboration-viewer`

## Module Map

```
src/
  main.rs          — Entry point; Dioxus desktop launch (1400×900); auto-play loop via
                     use_future (50 ms poll, speed-scaled tick accumulator); step_simulation()
                     dispatch table (50 ticks → 5 phases); update_quest_status() helper;
                     event handlers for step/play-pause/reset/speed/POV selection
  lib.rs           — Module declarations (components, state, theme)
  theme.rs         — Skin enum; ThemedRoot wrapper component; SkinSwitcher button
  state/
    mod.rs         — CollaborationState, Phase, Quest, QuestStatus, PlanSection, Peer,
                     PeerState, PacketAnimation, EventEntry, EventType, ScenarioData;
                     CollaborationState::default/reset/add_event/send_packet/update_animations
  components/
    mod.rs         — Re-exports for all components
    (inline in main.rs for small components; larger panels in mod.rs)
```

### Key Types (state/mod.rs)

| Type | Description |
|------|-------------|
| `CollaborationState` | Full simulation state: tick, phase, peers, quests, plan sections, events, animations, speed, paused, selected_pov |
| `Phase` | Setup / QuestCreation / DocumentCollaboration / QuestUpdates / Verification / Complete |
| `Peer` | A, B, C — implements `display_name()`, `all()` iterator |
| `PeerState` | online, quests_created, sections_written, messages_sent |
| `Quest` | id, title, creator: Peer, assignee: Peer, status: QuestStatus |
| `QuestStatus` | Pending / InProgress / Completed |
| `PlanSection` | id, author: Peer, content |
| `PacketAnimation` | Animated sync packet with from/to/label/progress fields |
| `ScenarioData` | Static data: `quests()` and `plan_sections()` slice constructors |

### Key Components

| Component | Description |
|-----------|-------------|
| `Header` | Phase banner, tick counter, progress bar |
| `PeerPanel` | Three peer cards with status, stats, click-to-POV |
| `VisualizationPanel` | Network diagram with animated sync packets |
| `RightPanel` | Quest list + project plan sections |
| `POVDashboard` | First-person view for a selected peer; peer-switch nav |
| `ControlBar` | Floating play/pause, step, reset, speed slider; always visible |
| `SkinSwitcher` | Theme toggle button rendered in ThemedRoot |

## Tick Dispatch (step_simulation)

The 50-tick scenario is a match table in `main.rs`:

| Ticks | Phase | Events |
|-------|-------|--------|
| 1–5 | Setup | Peers A, B, C come online |
| 6–17 | QuestCreation | 6 quests created (ticks 7–12), sync packets broadcast |
| 18–32 | DocumentCollaboration | 3 plan sections added (ticks 20, 24, 28), doc_sync packets |
| 33–42 | QuestUpdates | Quests 1, 3, 5 → InProgress; quests 1, 5 → Completed |
| 43–50 | Verification | Convergence checks; Phase::Complete at tick 50 |

## Key Patterns

- **Self-contained scenario**: no stdin, no file I/O, no workspace crate deps. All data comes
  from `ScenarioData` static methods. Easy to run as a demo with zero setup.
- **Speed-scaled accumulator**: the auto-play loop accumulates `0.05 * speed` per 50 ms tick;
  a simulation step fires when the accumulator reaches 0.5. Speed=1 → ~1 step/sec; speed=10
  → ~10 steps/sec.
- **POV mode**: `CollaborationState::selected_pov: Option<Peer>` toggles between overview and
  first-person layouts. The root `App` conditionally renders `POVDashboard` or the overview grid.
- **Packet animations**: `send_packet()` appends a `PacketAnimation` to the state;
  `update_animations(dt)` advances progress fields each frame and removes completed packets.

## Dependencies

| Crate | Role |
|-------|------|
| `dioxus` (desktop) | UI framework |
| `tokio` | Async runtime for auto-play loop |
| `serde` / `serde_json` | Serialization (state snapshots if needed) |
| `tracing` / `tracing-subscriber` | Logging |

No workspace crates referenced — fully standalone.

## Testing

Run directly; no external input needed:

```bash
cargo run -p indras-collaboration-viewer
```

Verify all 5 phases complete (tick 50 reaches Phase::Complete) by stepping through manually
or letting autoplay run to completion.
