# indras-genesis

First-run onboarding desktop app (binary + library). Guides a new user through the
complete Indras Network setup: welcome screen → display name → pass-story mnemonic
key generation → home realm exploration (quests, notes, artifacts, contacts) →
peer realm chat demo. Produces a fully initialized node identity by the end of the flow.

## Module Map

```
src/
  lib.rs                    — pub mod components, pub mod state
  main.rs                   — Dioxus desktop launch entry point

  state/
    mod.rs                  — GenesisState, GenesisStep enum, PassStoryState

  components/
    mod.rs                  — re-exports all components
    app.rs                  — App root; reads GenesisStep, renders active screen
    welcome.rs              — WelcomeScreen — splash + "Get Started" CTA
    display_name.rs         — DisplayName — text input for choosing a display name
    pass_story_flow.rs      — PassStoryFlow — mnemonic generation and word-by-word entry
    story_stage.rs          — StoryStage — individual word-reveal step in pass-story
    story_review.rs         — StoryReview — full mnemonic review before confirmation
    home_realm.rs           — HomeRealm — simulated home realm with tabs
    note_editor.rs          — NoteEditor — inline note creation within onboarding
    quest_editor.rs         — QuestEditor — inline quest creation within onboarding
    peer_realm.rs           — PeerRealm — demo peer realm with chat
```

## Key Types

- `GenesisStep` — enum driving the top-level wizard: `Welcome`, `DisplayName`,
  `PassStory`, `HomeRealm`, `PeerRealm` (and sub-steps within each)
- `GenesisState` — global Dioxus signal holding current step + all collected data
  (display name, generated mnemonic, confirmed words, created realm ID)
- `PassStoryState` — tracks mnemonic generation progress: word list, current index,
  user-typed confirmations, validation errors

## Key Patterns

- Pure wizard flow: `App` reads `GenesisState` signal and switches on `GenesisStep`;
  each screen advances the step on completion
- Pass-story uses `arboard` for clipboard copy of the mnemonic
- `indras-node` is called at the end to materialise the keypair from the mnemonic and
  create the home realm; all earlier steps are UI-only
- `indras-ui` provides `ThemedRoot`, `SHARED_CSS`, `ChatPanel`, and identity helpers —
  genesis does not define its own design tokens
- `indras-logging` is initialised via `tracing-subscriber` in `main.rs` before launch

## Dependencies

| Crate | Role |
|---|---|
| `dioxus` (0.7, desktop) | UI framework |
| `indras-network` | Network handle, peer identity |
| `indras-crypto` | Mnemonic generation, keypair derivation |
| `indras-node` | Node initialisation, home realm creation |
| `indras-sync-engine` | Realm sync after node creation |
| `indras-ui` | Shared components and theme |
| `indras-logging` | Structured log initialisation |
| `arboard` | Clipboard access for mnemonic copy |
| `tokio` | Async runtime (multi-thread) |
| `chrono` | Timestamp display |

## Testing

No automated tests. Run `cargo run -p indras-genesis` from the repo root and walk
the full wizard to verify: display name acceptance, mnemonic generation and confirmation,
home realm rendering, note/quest editors, peer realm chat. Check that completing the
flow produces a valid node identity (logs will show key fingerprint).
