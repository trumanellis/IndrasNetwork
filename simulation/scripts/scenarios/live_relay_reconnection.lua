-- Live Relay Reconnection — data persists across owner sessions
--
-- Tests that events stored by one owner session are visible to a fresh
-- owner session created after the first closes. Uses new_as_owner() which
-- always loads the same signing key, so all sessions share the same identity.
--
-- Sequence:
-- 1. Create relay with owner
-- 2. Session A: authenticate, register interface, store "e1"
-- 3. Close session A
-- 4. Session B (new_as_owner): authenticate, retrieve — verify "e1" present
-- 5. Session B: store "e2"
-- 6. Close session B
-- 7. Session C (new_as_owner): authenticate, retrieve — verify both "e1" and "e2"
-- 8. Shutdown
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_reconnection.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_reconnection")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live Relay Reconnection Test", {
    description = "Owner reconnection: data persists across independent sessions",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE RELAY WITH OWNER
-- ============================================================================

indras.narrative("A relay starts — the owner key is minted once, used forever")
logger.info("Phase 1: Creating relay with owner", { phase = 1 })

local relay = indras.RelayNode.new({ owner = true })
local owner_id = relay:owner_player_id()

indras.assert.not_nil(owner_id, "Relay should have an owner player_id")

local iface_hex = string.rep("cc", 32)  -- shared 64-char hex interface ID

advance()
logger.info("Relay created", {
    tick = tick,
    owner_id = owner_id,
    data_dir = relay:data_dir(),
})

-- ============================================================================
-- PHASE 2: SESSION A — authenticate, register, store "e1"
-- ============================================================================

indras.narrative("Session A opens — registers the interface and writes the first event")
logger.info("Phase 2: Session A — authenticate, register, store e1", { phase = 2 })

local session_a = indras.RelayClient.new_as_owner(relay)
local auth_a = session_a:authenticate()

indras.assert.true_(auth_a.authenticated, "Session A should authenticate")

local reg_a = session_a:register({ iface_hex })
indras.assert.true_(#reg_a.accepted == 1, "Interface should be accepted in session A")

local ack_e1 = session_a:store_event("Self_", iface_hex, "e1")
indras.assert.true_(ack_e1.accepted, "Event e1 should be accepted")

advance()
logger.event("session_a_stored", {
    tick = tick,
    event = "e1",
    accepted = ack_e1.accepted,
    player_id = session_a:player_id(),
})

-- ============================================================================
-- PHASE 3: CLOSE SESSION A
-- ============================================================================

indras.narrative("Session A closes — the connection drops, the data stays")
logger.info("Phase 3: Closing session A", { phase = 3 })

session_a:close()

advance()
logger.info("Session A closed", { tick = tick })

-- ============================================================================
-- PHASE 4: SESSION B — reconnect and verify "e1" is present
-- ============================================================================

indras.narrative("Session B reconnects — does e1 survive the disconnect?")
logger.info("Phase 4: Session B — authenticate and retrieve", { phase = 4 })

local session_b = indras.RelayClient.new_as_owner(relay)
local auth_b = session_b:authenticate()

indras.assert.true_(auth_b.authenticated, "Session B should authenticate")

-- Re-register the interface (new session, same relay)
local reg_b = session_b:register({ iface_hex })
indras.assert.true_(#reg_b.accepted == 1, "Interface should be accepted in session B")

local del_b = session_b:retrieve(iface_hex, "Self_")

indras.assert.true_(#del_b.events >= 1, "Session B should find at least 1 event (e1)")

local found_e1 = false
for _, ev in ipairs(del_b.events) do
    if ev.data == "e1" then found_e1 = true end
end
indras.assert.true_(found_e1, "Session B should retrieve event e1 stored by session A")

advance()
logger.event("session_b_retrieved", {
    tick = tick,
    event_count = #del_b.events,
    found_e1 = found_e1,
    has_more = del_b.has_more,
})

-- ============================================================================
-- PHASE 5: SESSION B STORES "e2"
-- ============================================================================

logger.info("Phase 5: Session B stores e2", { phase = 5 })

local ack_e2 = session_b:store_event("Self_", iface_hex, "e2")
indras.assert.true_(ack_e2.accepted, "Event e2 should be accepted by session B")

advance()
logger.event("session_b_stored", {
    tick = tick,
    event = "e2",
    accepted = ack_e2.accepted,
})

-- ============================================================================
-- PHASE 6: CLOSE SESSION B
-- ============================================================================

indras.narrative("Session B closes — e1 and e2 now rest in the relay")
logger.info("Phase 6: Closing session B", { phase = 6 })

session_b:close()

advance()
logger.info("Session B closed", { tick = tick })

-- ============================================================================
-- PHASE 7: SESSION C — verify both "e1" and "e2" present
-- ============================================================================

indras.narrative("Session C arrives — the full history should be waiting")
logger.info("Phase 7: Session C — authenticate and retrieve all events", { phase = 7 })

local session_c = indras.RelayClient.new_as_owner(relay)
local auth_c = session_c:authenticate()

indras.assert.true_(auth_c.authenticated, "Session C should authenticate")

local reg_c = session_c:register({ iface_hex })
indras.assert.true_(#reg_c.accepted == 1, "Interface should be accepted in session C")

local del_c = session_c:retrieve(iface_hex, "Self_")

indras.assert.true_(#del_c.events >= 2, "Session C should find at least 2 events (e1 and e2)")

local found_e1_c = false
local found_e2_c = false
for _, ev in ipairs(del_c.events) do
    if ev.data == "e1" then found_e1_c = true end
    if ev.data == "e2" then found_e2_c = true end
end

indras.assert.true_(found_e1_c, "Session C should retrieve e1 (stored by session A)")
indras.assert.true_(found_e2_c, "Session C should retrieve e2 (stored by session B)")

advance()
logger.event("session_c_retrieved", {
    tick = tick,
    event_count = #del_c.events,
    found_e1 = found_e1_c,
    found_e2 = found_e2_c,
    has_more = del_c.has_more,
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

session_c:close()
relay:shutdown()

advance()
logger.info("Relay shut down cleanly", { tick = tick })

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_relay_reconnection")

result:add_metrics({
    sessions_used = 3,
    events_stored = 2,
    session_b_event_count = #del_b.events,
    session_c_event_count = #del_c.events,
    e1_survived_reconnect = found_e1,
    e1_in_final = found_e1_c,
    e2_in_final = found_e2_c,
    final_tick = tick,
})

result:record_assertion("relay_created", true, true, true)
result:record_assertion("session_a_authenticated", auth_a.authenticated, true, auth_a.authenticated)
result:record_assertion("e1_stored", ack_e1.accepted, true, ack_e1.accepted)
result:record_assertion("session_b_authenticated", auth_b.authenticated, true, auth_b.authenticated)
result:record_assertion("e1_survives_disconnect", found_e1, true, found_e1)
result:record_assertion("e2_stored", ack_e2.accepted, true, ack_e2.accepted)
result:record_assertion("session_c_authenticated", auth_c.authenticated, true, auth_c.authenticated)
result:record_assertion("both_events_in_session_c", found_e1_c and found_e2_c, true, found_e1_c and found_e2_c)

local final_result = result:build()

logger.info("Live Relay Reconnection completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    persistence_verified = found_e1_c and found_e2_c,
    final_tick = tick,
})

return final_result
