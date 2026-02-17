# Plan: Lua Live Node Scenarios for `indras-network` High-Level API

## Context

The simulation crate has Lua bindings (`live_node.rs`) that wrap the low-level `IndrasNode` — exposing only raw interfaces, byte messages, and events. The high-level `indras-network` API (`IndrasNetwork`, `Realm`, `Document<T>`, `HomeRealm`, `ContactsRealm`) is completely untested from Lua. Three existing live scenarios (`live_p2p_sync.lua`, `live_abc_relay.lua`, `live_harmony.lua`) all operate at the interface level, not the realm/document/artifact level.

Goal: Create Lua bindings for the full `indras-network` API and write 8 comprehensive live node scenarios that exercise every major feature.

---

## Phase 1: Dependency & Wiring

### `simulation/Cargo.toml`
Add `indras-network.workspace = true` to dependencies.

### `simulation/src/lua/bindings/mod.rs`
Add module declaration: `pub mod live_network;`

### `simulation/src/lua/mod.rs`
Add after `bindings::live_node::register`: `bindings::live_network::register(lua, &indras)?;`

---

## Phase 2: Rust Binding Layer

### `simulation/src/lua/bindings/live_network.rs` (NEW — ~900 lines)

5 UserData types following `live_node.rs` patterns:

- **LuaNetwork** — wraps `Arc<RwLock<IndrasNetwork>>`, constructors, lifecycle, identity, realm management
- **LuaRealm** — wraps `Realm`, messaging, members, documents, aliases, artifacts
- **LuaDocument** — wraps `Document<serde_json::Value>`, read/update/merge
- **LuaHomeRealm** — wraps `HomeRealm`, artifact upload, access control, tree composition
- **LuaContactsRealm** — wraps `ContactsRealm`, add/remove/sentiment/block

---

## Phase 3: Lua Helper Library

### `simulation/scripts/lib/live_network_helpers.lua` (NEW — ~100 lines)

Helpers: `create_networks`, `connect_all`, `stop_all`, `wait_for`, `assert_eventually`, `dump_network`

---

## Phase 4: Lua Scenarios (8 scripts)

1. `live_network_identity.lua` — Identity, display name, export/import
2. `live_realm_messaging.lua` — Realm creation, join, send, reply, react, search
3. `live_documents_sync.lua` — Document CRDT sync between peers
4. `live_home_artifacts.lua` — Home realm artifacts, access control, tree composition
5. `live_contacts_social.lua` — Contacts realm, sentiment, blocking
6. `live_direct_connect.lua` — DM realms via connect/connect_by_code
7. `live_realm_features.lua` — Aliases, read tracking, unread counts
8. `live_offline_recovery.lua` — Offline/online sync recovery

---

## Implementation Order

1. Cargo.toml + mod.rs wiring (3 edits)
2. live_network.rs — LuaNetwork
3. live_network.rs — LuaRealm
4. live_network.rs — LuaDocument
5. live_network.rs — LuaHomeRealm
6. live_network.rs — LuaContactsRealm
7. cargo build -p indras-simulation — verify compilation
8. live_network_helpers.lua
9. Scenarios 1-8
10. Full verification pass
