# Plan: Lua Scripting for indras-workspace GUI Automation

## Context

indras-workspace is a Dioxus 0.7 desktop app with 12 UI components, P2P networking, and artifact management — but zero automated UI tests. The project already has a mature Lua testing stack in the `simulation` crate (mlua 0.10, 92 scenarios, assertion helpers, structured logging). This plan extends Lua scripting into the workspace app for action-driven GUI testing and multi-instance scenario automation.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Integration model | Embedded Lua runtime | Direct access, no IPC protocol needed, reuses existing mlua infra |
| Action model | Semantic actions via ActionBus | Tests real UI dispatch paths without coupling to DOM/CSS |
| Multi-instance | Shared script, branch on identity | Matches existing `INDRAS_NAME` multi-launch pattern |
| Activation | `--script` flag / `INDRAS_SCRIPT` env | Zero overhead when not testing |

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Dioxus Process (one per instance)                  │
│                                                     │
│  ┌──────────┐    ActionBus     ┌──────────────────┐ │
│  │ Lua      │───(mpsc tx)────▶│ RootApp          │ │
│  │ Runtime  │                  │ dispatcher       │ │
│  │ (thread) │◀──(broadcast)───│ (polls each tick)│ │
│  │          │   EventBus       │                  │ │
│  └──────────┘                  └──────────────────┘ │
│       │                              │              │
│       │ indras.query()               │ fires real   │
│       ▼                              ▼ handlers     │
│  ┌──────────┐                  ┌──────────────────┐ │
│  │ State    │                  │ Signals, Vault,  │ │
│  │ Snapshot │                  │ Network, etc.    │ │
│  └──────────┘                  └──────────────────┘ │
└─────────────────────────────────────────────────────┘
```

### Three Communication Channels

1. **ActionBus** (`tokio::sync::mpsc`): Lua thread → Dioxus main thread. Carries semantic `Action` values like `ClickSidebar("Love")`, `SendMessage("hello")`.

2. **EventBus** (`tokio::sync::broadcast`): Dioxus main thread → Lua thread. Emits observable events like `PeerConnected("Love")`, `MessageReceived { from, text }`. Lua's `wait_for()` subscribes to this.

3. **QueryBus** (`tokio::sync::oneshot` per query): Lua sends a query + oneshot sender → RootApp reads state and replies on the oneshot → Lua receives the snapshot. Used for `indras.query("chat_message_count")`.

### Threading Model

- **Lua thread**: Dedicated OS thread via `std::thread::spawn`. mlua's `Lua` instance is `!Send` so it must stay on one thread. The Lua script runs synchronously on this thread, blocking on `wait_for()` and `query()` via channel recv.
- **Dioxus thread**: Main thread, runs the normal Dioxus event loop. Polls ActionBus in a `use_future` hook (non-blocking `try_recv`).
- **Bridge**: All three channels are created before spawning the Lua thread. The Dioxus side holds `ActionRx`, `EventTx`, `QueryRx`. The Lua side holds `ActionTx`, `EventRx`, `QueryTx`.

### Activation

The Lua runtime is only initialized when the app detects a test script:

```rust
// main.rs
let script_path = std::env::var("INDRAS_SCRIPT").ok()
    .or_else(|| args.iter().find(|a| a.starts_with("--script="))
        .map(|a| a.trim_start_matches("--script=").to_string()));

if let Some(path) = script_path {
    let (action_tx, action_rx) = mpsc::channel(256);
    let (event_tx, _) = broadcast::channel(256);
    let (query_tx, query_rx) = mpsc::channel(64);

    // Spawn Lua thread
    let event_rx = event_tx.subscribe();
    std::thread::spawn(move || {
        let rt = LuaTestRuntime::new(action_tx, event_rx, query_tx);
        rt.exec_file(&path);
    });

    // Pass channels to Dioxus app via context
    app_channels = Some(AppTestChannels { action_rx, event_tx, query_rx });
}
```

When no script is specified, no channels are created, no thread is spawned — zero overhead.

---

## Action Enum

All semantic actions the Lua API can trigger:

```rust
pub enum Action {
    // Navigation
    ClickSidebar(String),          // by label: "Love", "My Journal"
    ClickTab(String),              // "vault", "quest", "settings"
    ClickPeerDot(String),          // by peer name
    ClickBreadcrumb(usize),        // by depth index

    // Contact flow
    OpenContacts,                  // opens the contact invite overlay
    PasteConnectCode(String),      // sets the connect input field
    ClickConnect,                  // presses the connect button
    CloseOverlay,                  // closes any open overlay

    // Messaging
    TypeMessage(String),           // sets the compose text
    SendMessage,                   // clicks send
    // Future: EditMessage(id, text), DeleteMessage(id)

    // Document editing
    ClickBlock(usize),             // click block at index
    TypeInBlock(usize, String),    // set block content
    AddBlock(String),              // add block of type: "text", "heading", "code"

    // Slash menu
    OpenSlashMenu,
    SelectSlashAction(String),     // "new-story", "new-quest", etc.

    // Setup / onboarding
    SetDisplayName(String),
    ClickCreateIdentity,

    // Utility
    Wait(f64),                     // seconds
    Screenshot(String),            // save screenshot to path (future)
}
```

### Dispatcher (in RootApp)

The dispatcher runs in a `use_future` that polls the ActionBus:

```rust
// In RootApp, after all signals are created:
if let Some(channels) = use_context::<AppTestChannels>() {
    use_future(move || {
        let mut action_rx = channels.action_rx;
        let event_tx = channels.event_tx;
        async move {
            while let Some(action) = action_rx.recv().await {
                match action {
                    Action::ClickSidebar(label) => {
                        // Find node by label, call on_tree_click
                        if let Some(node) = workspace.read().nav.vault_tree
                            .iter().find(|n| n.label == label) {
                            on_tree_click(node.id.clone());
                        }
                    }
                    Action::OpenContacts => {
                        contact_invite_open.set(true);
                    }
                    Action::PasteConnectCode(code) => {
                        contact_invite_input.set(code);
                    }
                    Action::ClickConnect => {
                        // Trigger the on_connect handler with current input
                        let uri = contact_invite_input.read().clone();
                        on_connect(uri);
                    }
                    Action::TypeMessage(text) => {
                        // Set compose text signal
                        compose_text.set(text);
                    }
                    Action::SendMessage => {
                        // Trigger send handler
                        on_send(compose_text.read().clone());
                    }
                    Action::Wait(secs) => {
                        tokio::time::sleep(Duration::from_secs_f64(secs)).await;
                    }
                    // ... etc
                }
            }
        }
    });
}
```

---

## Event Bus

Events emitted by the app that Lua can wait on:

```rust
pub enum AppEvent {
    // Lifecycle
    AppReady,                              // workspace phase reached
    IdentityCreated(String),               // display name

    // Peers
    PeerConnected(String),                 // peer display name
    PeerDisconnected(String),

    // Navigation
    ViewChanged(String),                   // "document", "story", "quest", "settings"
    SidebarItemActive(String),             // label of active item

    // Messaging
    MessageReceived { from: String, text: String },
    MessageSent { text: String },

    // Overlay
    OverlayOpened(String),                 // "contacts", "preview", "pass_story"
    OverlayClosed(String),

    // Errors
    ActionFailed { action: String, error: String },
}
```

### Emission Points

Events are emitted at the same points where `log_event()` is currently called, plus a few new ones:

| Emission Point | Event | Location |
|----------------|-------|----------|
| After `ws.phase = AppPhase::Workspace` | `AppReady` | app.rs boot effect |
| After `connect_by_code()` succeeds | `PeerConnected(name)` | on_connect handler |
| Polling loop detects new contact | `PeerConnected(name)` | polling effect |
| After `realm.send_chat()` | `MessageSent { text }` | ChatPanel send |
| Chat stream receives message | `MessageReceived { from, text }` | ChatPanel stream |
| `workspace.write().ui.active_view = vt` | `ViewChanged(type)` | on_tree_click |

---

## Query System

Lua can synchronously query app state for assertions:

```rust
pub enum Query {
    Identity,              // → { name: String, id_short: String }
    ActiveView,            // → String ("document", "story", etc.)
    ActiveSidebarItem,     // → String (label)
    PeerCount,             // → u64
    PeerNames,             // → Vec<String>
    ChatMessageCount,      // → u64
    ChatMessages,          // → Vec<{ from: String, text: String }>
    SidebarItems,          // → Vec<{ label: String, icon: String }>
    EventLog,              // → Vec<{ direction: String, text: String }>
    OverlayOpen,           // → Option<String>
    Custom(String),        // extensible
}

pub enum QueryResult {
    String(String),
    Number(f64),
    StringList(Vec<String>),
    Json(serde_json::Value),  // for complex results
    Error(String),
}
```

The dispatcher reads the relevant signals and replies on the oneshot:

```rust
Query::PeerCount => {
    let count = workspace.read().peers.entries.len() as f64;
    reply.send(QueryResult::Number(count)).ok();
}
Query::ChatMessages => {
    let msgs = chat_state.read().messages.iter().map(|m| {
        serde_json::json!({ "from": m.sender, "text": m.content })
    }).collect();
    reply.send(QueryResult::Json(serde_json::Value::Array(msgs))).ok();
}
```

---

## Lua API Surface

### Global Table: `indras`

```lua
-- Identity
indras.identity()          -- returns { name = "Joy", id = "a1b2c3d4" }
indras.my_name()           -- shorthand: returns "Joy"

-- Actions (async — blocks until dispatched)
indras.action(name, ...)   -- dispatch a semantic action
-- Convenience wrappers:
indras.click_sidebar(label)
indras.click_tab(name)
indras.click_peer(name)
indras.open_contacts()
indras.paste_code(uri)
indras.click_connect()
indras.type_message(text)
indras.send_message()
indras.set_name(name)
indras.create_identity()
indras.wait(seconds)

-- Events (blocking wait)
indras.wait_for(event_name, filter, timeout_secs)
-- Examples:
indras.wait_for("app_ready")
indras.wait_for("peer_connected", "Love")
indras.wait_for("message_received", { from = "Joy" }, 10)

-- Queries (synchronous)
indras.query(name)
-- Examples:
indras.query("peer_count")          -- → 1
indras.query("active_view")         -- → "story"
indras.query("chat_messages")       -- → [{from="Joy", text="hello"}]
indras.query("sidebar_items")       -- → [{label="My Journal", icon="📖"}, ...]

-- Assertions (reuses simulation pattern)
indras.assert.eq(actual, expected, msg)
indras.assert.ne(actual, expected, msg)
indras.assert.gt(actual, expected, msg)
indras.assert.contains(list, item, msg)
indras.assert.truthy(val, msg)

-- Logging (reuses simulation pattern)
indras.log.info(msg, fields)
indras.log.debug(msg, fields)
indras.log.warn(msg, fields)
indras.log.error(msg, fields)
```

### Registration (in Rust)

```rust
impl LuaTestRuntime {
    pub fn new(action_tx: Sender<Action>, event_rx: broadcast::Receiver<AppEvent>,
               query_tx: Sender<(Query, oneshot::Sender<QueryResult>)>) -> Self {
        let lua = mlua::Lua::new();

        let indras = lua.create_table().unwrap();

        // indras.action(name, ...)
        let tx = action_tx.clone();
        indras.set("action", lua.create_function(move |_, (name, arg): (String, Option<mlua::Value>)| {
            let action = parse_action(&name, arg)?;
            tx.blocking_send(action).map_err(mlua::Error::external)?;
            Ok(())
        }).unwrap()).unwrap();

        // indras.wait_for(event, filter, timeout)
        let rx = Mutex::new(event_rx);
        indras.set("wait_for", lua.create_function(move |_, (event, filter, timeout): (String, Option<mlua::Value>, Option<f64>)| {
            let timeout = Duration::from_secs_f64(timeout.unwrap_or(30.0));
            let deadline = Instant::now() + timeout;
            loop {
                match rx.lock().unwrap().try_recv() {
                    Ok(evt) if matches_event(&evt, &event, &filter) => return Ok(true),
                    Err(broadcast::error::TryRecvError::Empty) => {
                        if Instant::now() > deadline {
                            return Err(mlua::Error::external(format!(
                                "Timeout waiting for event '{}' after {:.1}s", event, timeout.as_secs_f64()
                            )));
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    _ => continue,
                }
            }
        }).unwrap()).unwrap();

        // indras.query(name)
        let qtx = query_tx.clone();
        indras.set("query", lua.create_function(move |lua, name: String| {
            let (tx, rx) = oneshot::channel();
            let query = parse_query(&name)?;
            qtx.blocking_send((query, tx)).map_err(mlua::Error::external)?;
            let result = rx.blocking_recv().map_err(mlua::Error::external)?;
            query_result_to_lua(lua, result)
        }).unwrap()).unwrap();

        lua.globals().set("indras", indras).unwrap();
        Self { lua }
    }
}
```

---

## Example Scenarios

### Scenario 1: Two-peer connect and chat

```lua
-- scripts/scenarios/two_peer_chat.lua
local me = indras.my_name()
indras.log.info("Starting as " .. me)

-- Both instances wait for app to be ready
indras.wait_for("app_ready")

if me == "Joy" then
    -- Joy has Love's code (passed via env or hardcoded for test)
    local love_code = os.getenv("LOVE_CODE") or "indras://test-love-code"

    indras.wait(2)  -- let Love's instance fully start
    indras.open_contacts()
    indras.paste_code(love_code)
    indras.click_connect()
    indras.wait_for("peer_connected", "Love", 15)
    indras.log.info("Connected to Love!")

    -- Navigate to Love's contact and send a message
    indras.click_sidebar("Love")
    indras.wait(1)
    indras.type_message("Hello from Joy!")
    indras.send_message()
    indras.wait_for("message_sent", nil, 5)

    -- Verify
    local count = indras.query("chat_message_count")
    indras.assert.ge(count, 1, "Should have at least 1 message")
    indras.log.info("Test passed: Joy sent message")

elseif me == "Love" then
    -- Love waits for Joy to connect
    indras.wait_for("peer_connected", "Joy", 20)
    indras.log.info("Joy connected!")

    -- Navigate to Joy's contact
    indras.click_sidebar("Joy")

    -- Wait for Joy's message to arrive
    indras.wait_for("message_received", { from = "Joy" }, 15)

    -- Verify
    local msgs = indras.query("chat_messages")
    indras.assert.eq(#msgs, 1, "Should have 1 message")
    indras.assert.eq(msgs[1].text, "Hello from Joy!", "Message content should match")
    indras.log.info("Test passed: Love received message")
end
```

### Scenario 2: Onboarding flow

```lua
-- scripts/scenarios/onboarding.lua
indras.wait_for("app_ready")

-- Should start in Setup phase
local view = indras.query("app_phase")
indras.assert.eq(view, "setup", "New user should see setup")

-- Create identity
indras.set_name("Zephyr")
indras.create_identity()
indras.wait_for("identity_created", "Zephyr", 10)

-- Should now be in Workspace phase
local phase = indras.query("app_phase")
indras.assert.eq(phase, "workspace", "Should enter workspace after identity creation")

-- Sidebar should have default items
local items = indras.query("sidebar_items")
indras.assert.gt(#items, 0, "Should have at least one sidebar item")

indras.log.info("Onboarding test passed")
```

### Scenario 3: Navigation smoke test

```lua
-- scripts/scenarios/navigation_smoke.lua
indras.wait_for("app_ready")

local items = indras.query("sidebar_items")
indras.log.info("Found " .. #items .. " sidebar items")

-- Click each sidebar item and verify view changes
for _, item in ipairs(items) do
    indras.click_sidebar(item.label)
    indras.wait(0.3)
    local active = indras.query("active_sidebar_item")
    indras.assert.eq(active, item.label, "Active item should be " .. item.label)
    indras.log.info("Verified: " .. item.label .. " (" .. item.icon .. ")")
end

indras.log.info("Navigation smoke test passed")
```

---

## File Structure

```
crates/indras-workspace/
├── src/
│   ├── main.rs                    # Add --script flag, channel setup, Lua thread spawn
│   ├── lib.rs                     # Add `scripting` module
│   ├── scripting/
│   │   ├── mod.rs                 # Module exports
│   │   ├── action.rs              # Action enum + parser
│   │   ├── event.rs               # AppEvent enum + matcher
│   │   ├── query.rs               # Query enum + QueryResult
│   │   ├── channels.rs            # AppTestChannels struct, channel constructors
│   │   ├── dispatcher.rs          # Action dispatcher (called from RootApp)
│   │   └── lua_runtime.rs         # LuaTestRuntime — mlua setup, bindings
│   ├── components/
│   │   └── app.rs                 # Add dispatcher hook, event emissions
│   └── ...
├── Cargo.toml                     # Add mlua dependency (optional feature)
└── scripts/
    └── scenarios/
        ├── two_peer_chat.lua      # Multi-instance chat test
        ├── onboarding.lua         # Setup flow test
        └── navigation_smoke.lua   # Sidebar navigation test

scripts/
└── run-lua-test.sh               # Helper to launch N instances with a shared script
```

### Cargo.toml Changes

```toml
[features]
default = []
lua-scripting = ["mlua"]

[dependencies]
mlua = { version = "0.10", features = ["lua54", "vendored", "serialize"], optional = true }
```

The `lua-scripting` feature keeps mlua out of production builds. Test builds use:
```bash
cargo build -p indras-workspace --features lua-scripting
```

---

## Test Runner Script

```bash
#!/bin/bash
# scripts/run-lua-test.sh
# Usage: ./scripts/run-lua-test.sh scripts/scenarios/two_peer_chat.lua

SCRIPT="${1:?Usage: run-lua-test.sh <script.lua>}"
FEATURES="--features lua-scripting"

# Build once
cargo build -p indras-workspace $FEATURES 2>&1

# Export identity codes for multi-instance scenarios
export JOY_CODE="indras://joy-test-code"
export LOVE_CODE="indras://love-test-code"

# Launch instances in parallel
INDRAS_NAME=Joy INDRAS_SCRIPT="$SCRIPT" \
  INDRAS_WIN_X=100 INDRAS_WIN_Y=100 \
  cargo run -p indras-workspace $FEATURES &
PID_JOY=$!

INDRAS_NAME=Love INDRAS_SCRIPT="$SCRIPT" \
  INDRAS_WIN_X=700 INDRAS_WIN_Y=100 \
  cargo run -p indras-workspace $FEATURES &
PID_LOVE=$!

# Wait for both to finish
wait $PID_JOY
EXIT_JOY=$?
wait $PID_LOVE
EXIT_LOVE=$?

if [ $EXIT_JOY -eq 0 ] && [ $EXIT_LOVE -eq 0 ]; then
    echo "ALL TESTS PASSED"
    exit 0
else
    echo "TESTS FAILED (Joy=$EXIT_JOY, Love=$EXIT_LOVE)"
    exit 1
fi
```

---

## Implementation Phases

### Phase 1: Foundation (ActionBus + Dispatcher)

**Goal**: Lua scripts can trigger UI actions in a single instance.

1. Add `scripting/` module with `Action`, `AppEvent`, `Query`, `channels.rs`
2. Add `mlua` optional dependency
3. Create `LuaTestRuntime` with `indras.action()` and `indras.wait()`
4. Wire `--script` / `INDRAS_SCRIPT` in `main.rs`
5. Add dispatcher `use_future` in `RootApp`
6. Implement 5 core actions: `ClickSidebar`, `ClickTab`, `Wait`, `OpenContacts`, `SetDisplayName`
7. Write `navigation_smoke.lua` scenario

**Verify**: Run single instance with `--script`, sidebar clicks work.

### Phase 2: Events + Queries

**Goal**: Lua scripts can wait for events and assert state.

1. Add `EventBus` (broadcast channel)
2. Emit events at key points in `app.rs` (PeerConnected, ViewChanged, etc.)
3. Implement `indras.wait_for()` in Lua bindings
4. Add `QueryBus` (oneshot per query)
5. Implement `indras.query()` for: `active_view`, `peer_count`, `sidebar_items`, `app_phase`
6. Port assertion helpers from `simulation/src/lua/assertions.rs`
7. Write `onboarding.lua` scenario

**Verify**: Onboarding test passes — create identity, verify workspace phase.

### Phase 3: Contact Flow Actions

**Goal**: Lua can drive the full connect flow.

1. Add actions: `PasteConnectCode`, `ClickConnect`, `CloseOverlay`
2. Add events: `PeerConnected`, `OverlayOpened`, `OverlayClosed`
3. Add queries: `peer_names`, `overlay_open`
4. Create `run-lua-test.sh` runner script
5. Write single-instance connect test (mock peer code)

**Verify**: Connect flow scripted end-to-end in single instance.

### Phase 4: Chat Actions + Multi-Instance

**Goal**: Two instances can script a full chat conversation.

1. Add actions: `TypeMessage`, `SendMessage`
2. Add events: `MessageSent`, `MessageReceived`
3. Add queries: `chat_message_count`, `chat_messages`
4. Emit chat events from ChatPanel (requires threading EventTx through)
5. Write `two_peer_chat.lua` scenario
6. Test with `run-lua-test.sh`

**Verify**: Joy sends message, Love receives it — fully automated.

### Phase 5: Polish + CI

**Goal**: Reliable test suite ready for CI.

1. Add timeout handling (script-level timeout, per-action timeout)
2. Add JSONL structured logging (reuse simulation's logging infra)
3. Add `--headless` mode (skip window creation, run faster)
4. Exit codes: 0 = pass, 1 = assertion failure, 2 = timeout, 3 = error
5. Add CI script that runs all scenarios
6. Document the Lua API in `docs/lua-testing.md`

**Verify**: `./scripts/run-all-lua-tests.sh` runs suite, reports pass/fail.

---

## Security

### Threat Model

This is **test infrastructure**, not a user-facing plugin system. The Lua runtime only exists in test builds and only activates with an explicit flag. The threat model is accidental misuse, not adversarial attack.

### Defense in Depth (Three Layers)

**Layer 1: Compile-time gate (feature flag)**

The `lua-scripting` feature excludes all Lua code from production binaries:

```toml
[features]
lua-scripting = ["mlua"]
```

No feature flag = no Lua code in the binary. Zero attack surface.

**Layer 2: Runtime gate (debug assertions)**

Even if someone enables the feature in a release build, the scripting module refuses to initialize:

```rust
// In main.rs, before spawning Lua thread:
#[cfg(not(debug_assertions))]
{
    eprintln!("WARNING: Lua scripting is only available in debug builds");
    std::process::exit(1);
}
```

**Layer 3: Lua sandbox (stripped globals)**

The Lua environment removes dangerous standard libraries at initialization:

```rust
fn sandbox_lua(lua: &mlua::Lua) {
    let globals = lua.globals();

    // Remove dangerous standard libraries
    globals.set("os", mlua::Nil).ok();       // no os.execute()
    globals.set("io", mlua::Nil).ok();       // no file I/O
    globals.set("loadfile", mlua::Nil).ok(); // no loading arbitrary Lua files
    globals.set("dofile", mlua::Nil).ok();   // no executing arbitrary Lua files
    globals.set("require", mlua::Nil).ok();  // no C module loading

    // Re-add safe os functions only
    let safe_os = lua.create_table().unwrap();
    safe_os.set("getenv", lua.create_function(|_, key: String| {
        Ok(std::env::var(key).ok())
    }).unwrap()).unwrap();
    safe_os.set("clock", lua.create_function(|_, ()| {
        Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_secs_f64())
    }).unwrap()).unwrap();
    globals.set("os", safe_os).ok();
}
```

Scripts can only interact with the app through the `indras.*` API — no filesystem access, no process spawning, no arbitrary code loading.

### Activation: CLI flag only (no env var)

To prevent env var injection from sibling processes, activation requires a CLI flag:

```rust
// Only --script flag, NOT INDRAS_SCRIPT env var
let script_path = args.iter()
    .find(|a| a.starts_with("--script="))
    .map(|a| a.trim_start_matches("--script=").to_string());
```

The `INDRAS_SCRIPT` env var mentioned elsewhere in this plan is removed in favor of `--script` only.

### What this does NOT protect against

| Scenario | Protected? | Why |
|----------|------------|-----|
| Production user runs malicious Lua | Yes | Feature flag + debug gate = code doesn't exist |
| Dev runs untrusted `.lua` file | Partially | Sandbox blocks `os`/`io`, but `indras.*` API can still send messages, connect to peers |
| Attacker with local shell access | No | They don't need Lua — they already own the machine |

### Future: If Lua becomes a plugin system

If Lua scripting is ever exposed to end users (plugins, automation), it would need:
- Capability-based permissions (`indras.request_permission("network")`)
- Resource limits (memory, CPU time via `mlua::HookTriggers`)
- Script signing or allowlisting
- Per-action user consent prompts

That is a fundamentally different design. This plan is strictly test infrastructure.

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| mlua `!Send` constraint | Can't share Lua instance across threads | Dedicated Lua thread with channel bridge (already in design) |
| Dioxus signal access from dispatcher | Dispatcher needs access to all signals | Dispatcher runs inside RootApp's scope where signals are available |
| Timing sensitivity in multi-instance | Tests may be flaky if events arrive late | `wait_for` with configurable timeouts + retry in runner script |
| Feature-gated mlua bloats compile | Slower CI builds | Only build with `--features lua-scripting` in test CI jobs |
| Action enum grows large | Maintenance burden | Group actions by module, use trait-based dispatch |
| ChatPanel event emission | ChatPanel is in `indras-ui`, separate from workspace | Pass `EventTx` via Dioxus context, ChatPanel checks for it optionally |

---

## Open Questions

1. **Headless mode**: Can Dioxus desktop skip window creation? If not, we may need a `--minimized` flag or run under Xvfb on CI.
2. **Identity exchange**: For multi-instance tests, how do we pass identity codes between instances? Options: shared file or a coordination temp directory.
3. **Screenshot capture**: Worth adding `indras.screenshot("path")` for visual regression? Dioxus desktop uses webview which supports screenshot APIs.
4. **Reuse simulation's LuaRuntime**: Should we extend `simulation::lua::LuaRuntime` or create a new one? The simulation bindings are network-sim specific, so a new runtime that shares assertion/logging helpers is cleaner.
