# indras-workspace

Full collaborative document editor desktop app. Provides block-based document editing
(text, headings, code, callout, image, todo, divider), an artifact browser, quest panel,
pass-story view, event log, and settings. Embeds `indras-chat` for in-app messaging.
Optional `lua-scripting` feature enables a Lua 5.4 runtime for test automation.

## Module Map

```
src/
  lib.rs              — pub mod state, components, bridge, scripting
  main.rs             — Dioxus desktop launch entry point

  state/
    mod.rs            — re-exports state types
    editor.rs         — EditorState: active document, block list, selection, dirty flag
    workspace.rs      — WorkspaceState: open documents, realm membership, sidebar nav
    navigation.rs     — NavigationState: current panel/view enum

  components/
    mod.rs            — re-exports all components
    app.rs            — App root; reads navigation state, renders active panel
    topbar.rs         — Topbar — title bar with realm name, controls, user avatar
    document.rs       — Document — full document view, renders block list
    blocks/           — one file per block type:
      editor.rs       — BlockEditor — dispatches to per-type editors
      (text, heading, code, callout, image, todo, divider block components)
    artifact_browser.rs — ArtifactBrowser — browse and open realm artifacts
    quest.rs          — QuestPanel — quest list and completion tracking
    pass_story.rs     — PassStoryView — display and copy the node mnemonic
    event_log.rs      — EventLog — live stream of network/sync events
    settings.rs       — Settings — node config, skin switcher, relay config
    setup.rs          — Setup — first-launch node initialisation flow

  bridge/
    mod.rs            — re-exports bridge types
    network_bridge.rs — connects Dioxus signals to indras-network events
    vault_bridge.rs   — connects Dioxus signals to artifact vault operations

  scripting/          — (lua-scripting feature only)
    mod.rs            — re-exports scripting types
    lua_runtime.rs    — LuaRuntime: embeds mlua, loads and executes scripts
    action.rs         — ScriptAction enum: commands scripts can issue
    event.rs          — ScriptEvent enum: events forwarded to running scripts
    query.rs          — query functions exposed to Lua (document state, peer list)
    channels.rs       — async channels connecting runtime to Dioxus bridge
    dispatcher.rs     — routes ScriptActions to the appropriate bridge
```

## Key Types

- `EditorState` — holds the active document's block list as a Dioxus signal; mutations
  go through action dispatch to keep undo history consistent
- `WorkspaceState` — top-level state signal; owns open document set and realm info
- `NavigationState` — enum-driven panel routing (`Document`, `Artifacts`, `Quests`,
  `PassStory`, `EventLog`, `Settings`, `Chat`)
- `network_bridge::NetworkBridge` / `vault_bridge::VaultBridge` — async adapters that
  subscribe to network/sync events and write into Dioxus signals
- `LuaRuntime` (feature-gated) — wraps `mlua`; scripts call query functions and emit
  `ScriptAction`s which drive the app state programmatically

## Key Patterns

- Block editing: each block type is its own component; `BlockEditor` dispatches on block
  kind and renders the appropriate editor; mutations write to `EditorState` signal
- Bridge pattern: `NetworkBridge` and `VaultBridge` are initialised once in `main.rs`,
  spawn Tokio tasks, and communicate with Dioxus via signals — no direct async calls
  inside components
- Embedded chat: `indras-chat` `App` component is rendered inside the `Chat` navigation
  panel; `CHAT_CSS` is injected alongside workspace CSS in `with_custom_head`
- Lua scripting (test automation only): enabled with `--features lua-scripting`; scripts
  are not used in production builds; the feature keeps `mlua` out of default binaries
- File dialogs: `rfd` is used for image insertion and artifact import; always called from
  a Tokio `spawn_blocking` context

## Dependencies

| Crate | Role |
|---|---|
| `dioxus` (0.7, desktop) | UI framework |
| `indras-artifacts` | Artifact CRUD and vault access |
| `indras-crypto` | Key formatting, mnemonic display |
| `indras-network` | Network handle, peer events |
| `indras-ui` | Shared theme, sidebar, detail panel, SHARED_CSS |
| `indras-chat` | Embedded chat panel and CHAT_CSS |
| `mlua` (optional) | Lua 5.4 scripting runtime |
| `tokio` | Async runtime (multi-thread) |
| `rfd` | Native file open/save dialogs |
| `arboard` | Clipboard (mnemonic copy) |
| `serde` / `serde_json` | Document serialisation |
| `rand` | Block ID generation |
| `chrono` | Timestamp display |

## Testing

No automated tests in the crate itself. Lua scripting feature enables programmatic
test automation via scripts in `simulation/scripts/`. Build with lua support:
`cargo build -p indras-workspace --features lua-scripting`. For manual testing, run
`cargo run -p indras-workspace` and verify: document editing (all block types), artifact
upload/browse, quest creation, settings/skin switching, embedded chat.
