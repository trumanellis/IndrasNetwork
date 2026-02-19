# Plan: Network Immune Response Simulation

## Goal
Create a Lua scenario (`sdk_immune_response.lua`) that demonstrates the full sentiment/trust/blocking lifecycle — the "immune system" of Indra's Network — viewable through the existing realm viewer and omni-viewer infrastructure.

## Narrative
The scenario tells the story of a healthy network detecting and isolating a bad actor through purely local, decentralized mechanisms. It follows five named characters through the full immune response cycle.

### Cast (futuristic baby names)
- **Zephyr** — the first to detect the threat
- **Nova** — quickly corroborates Zephyr's warning
- **Sage** — receives relayed sentiment signals and acts on them
- **Orion** — the bad actor who gets progressively isolated
- **Lyra** — an innocent bystander connected to Orion who must decide what to do

## Architecture

### What exists today
| Layer | Status | Notes |
|-------|--------|-------|
| Viewer event types | ✅ Done | `sentiment_updated`, `contact_blocked`, `relayed_sentiment_received` already in `types.rs` |
| ContactsState tracking | ✅ Done | `contacts_state.rs` processes all 3 new event types, tracks sentiments, blocks, relayed signals |
| app_state.rs log summaries | ✅ Done | Log lines for all 3 events |
| Lua `indras.sdk.contacts` | ⚠️ Partial | Has `add`, `remove`, `contains`, `list`, `count` — but **no** `set_sentiment`, `block`, or `get_sentiment` |
| Lua EVENTS constants | ⚠️ Partial | Has `CONTACT_ADDED`, `CONTACT_REMOVED` — missing sentiment/blocking constants |
| Lua scenario | ❌ Missing | No immune response scenario exists |
| Run script | ❌ Missing | No `run-immune-sim.sh` |

### What needs to be built

#### 1. Extend Lua contacts bindings (`simulation/src/lua/bindings/sdk.rs`)

Add methods to `LuaContacts`:
- `set_sentiment(contact_id, sentiment)` — store i8 sentiment (-1, 0, 1)
- `get_sentiment(contact_id)` → i8 or nil
- `block(contact_id)` → removes contact + returns true
- `is_blocked(contact_id)` → bool
- `sentiments()` → table of {contact_id = sentiment}
- `blocked_list()` → array of blocked contact IDs

**Internal change**: Upgrade `LuaContacts` from `HashSet<String>` to a struct that tracks:
```rust
struct ContactData {
    sentiment: i8,
    blocked: bool,
}
// contacts: Arc<RwLock<HashMap<String, ContactData>>>
// blocked: Arc<RwLock<HashSet<String>>>
```

#### 2. Add event constants to `quest_helpers.lua`

Add to `quest.EVENTS`:
```lua
SENTIMENT_UPDATED = "sentiment_updated",
CONTACT_BLOCKED = "contact_blocked",
RELAYED_SENTIMENT = "relayed_sentiment_received",
MEMBER_JOINED = "member_joined",
MEMBER_LEFT = "member_left",
```

#### 3. Create the Lua scenario (`simulation/scripts/scenarios/sdk_immune_response.lua`)

**7 phases** that map to the immune system analogy:

| Phase | Immune Analogy | Network Action | Events Emitted |
|-------|---------------|----------------|----------------|
| 1. Genesis | Healthy body | 5 peers join, form contacts, create peer-set realms | `contact_added`, `realm_created`, `member_joined` |
| 2. Infection | Pathogen appears | Orion starts misbehaving (spammy messages) | `chat_message` (spam from Orion) |
| 3. Detection | Innate immunity | Zephyr sets sentiment -1 on Orion | `sentiment_updated` (Zephyr→Orion = -1) |
| 4. Signal Propagation | Cytokine cascade | Nova also rates -1; relay signals reach Sage & Lyra | `sentiment_updated` (Nova→Orion = -1), `relayed_sentiment_received` ×2 |
| 5. Graduated Response | Adaptive immunity | Sage sets -1 after receiving relay. Zephyr blocks Orion. | `sentiment_updated` (Sage→Orion = -1), `contact_blocked` (Zephyr blocks Orion, cascade leaves realms) |
| 6. Cascade | Inflammation | Nova blocks Orion too. All shared realms dissolve. Orion isolated. | `contact_blocked` (Nova), `member_left` ×N |
| 7. Recovery | Homeostasis | Remaining 4 peers healthy. Lyra decides to keep/drop Orion. Network stable. | `sentiment_updated` (Lyra→Orion), optional `contact_blocked` |

**Tick budget**: ~200 ticks for `quick` level, events spread across phases with `sim:step()` calls between them.

**Assertions**:
- Orion ends with 0 or 1 contacts (only Lyra if she doesn't block)
- All other peers maintain mutual contacts
- Realm count decreases as expected from cascade
- No dangling sentiment entries for blocked contacts

#### 4. Create helper module (`simulation/scripts/lib/immune_helpers.lua`)

Small helper with:
- Character name constants (ZEPHYR, NOVA, SAGE, ORION, LYRA)
- `immune.LEVELS` config (quick/medium/full)
- `immune.setup_healthy_network(sim, mesh, logger)` — creates contacts + realms for all 5 peers
- `immune.emit_sentiment(logger, tick, member, contact, sentiment)` — logs `sentiment_updated` event
- `immune.emit_block(logger, tick, member, contact, realms_left)` — logs `contact_blocked` event
- `immune.emit_relay(logger, tick, member, about, sentiment, via)` — logs `relayed_sentiment_received` event

#### 5. Create run script (`scripts/run-immune-sim.sh`)

```bash
#!/bin/bash
# Run the immune response simulation through the realm viewer
STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner \
    --manifest-path simulation/Cargo.toml \
    -- "simulation/scripts/scenarios/sdk_immune_response.lua" \
    | cargo run -p indras-realm-viewer --bin omni-viewer -- "$@"
```

## File Change Summary

| File | Action | Size |
|------|--------|------|
| `simulation/src/lua/bindings/sdk.rs` | Edit | Add ~80 lines (sentiment/block methods to LuaContacts) |
| `simulation/scripts/lib/quest_helpers.lua` | Edit | Add ~5 lines (new EVENTS constants) |
| `simulation/scripts/lib/immune_helpers.lua` | Create | ~120 lines |
| `simulation/scripts/scenarios/sdk_immune_response.lua` | Create | ~350 lines |
| `scripts/run-immune-sim.sh` | Create | ~15 lines |

## Execution Order

1. **Extend Rust bindings** — `sdk.rs` LuaContacts gets sentiment/block support
2. **Build & test** — `cargo build -p indras-simulation` + `cargo test -p indras-simulation`
3. **Add event constants** — `quest_helpers.lua` EVENTS table
4. **Create immune_helpers.lua** — helper module for the scenario
5. **Create scenario** — `sdk_immune_response.lua` with all 7 phases
6. **Create run script** — `run-immune-sim.sh`
7. **Smoke test** — Run `STRESS_LEVEL=quick cargo run --bin lua_runner --manifest-path simulation/Cargo.toml -- simulation/scripts/scenarios/sdk_immune_response.lua` and verify JSONL output
8. **Viewer test** — Pipe through omni-viewer to verify visualization

## Viewer Integration

The scenario outputs JSONL events to stdout via `logger.event()`. The existing viewer infrastructure already handles:
- `contact_added` / `contact_removed` → ContactsState graph updates
- `sentiment_updated` → sentiment tracking with color-coded edges
- `contact_blocked` → block tracking + contact removal + realm leave cascade
- `relayed_sentiment_received` → relay signal display
- `realm_created` / `member_joined` / `member_left` → realm membership tracking
- `chat_message` → chat display (Orion's spam will be visible)
- `info` → phase markers in the log

No viewer changes needed — all event types are already wired up.
