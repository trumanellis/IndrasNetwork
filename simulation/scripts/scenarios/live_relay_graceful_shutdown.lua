-- Live Relay Graceful Shutdown — relay terminates cleanly under load
--
-- Tests that relay:shutdown() completes without hanging or crashing, and that
-- subsequent client operations correctly fail with an error after shutdown.
--
-- Sequence:
-- 1. Create relay with owner
-- 2. Owner authenticates, registers interface, stores an event
-- 3. Stranger authenticates
-- 4. Shutdown relay
-- 5. Owner ping — should error (connection closed); verify via pcall
-- 6. Stranger ping — should error; verify via pcall
-- 7. Log that shutdown was graceful (no hang, no crash)
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_graceful_shutdown.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_graceful_shutdown")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live Relay Graceful Shutdown Test", {
    description = "Relay shuts down cleanly; post-shutdown ops fail without crashing",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE RELAY WITH OWNER
-- ============================================================================

indras.narrative("A relay comes online — owner and a stranger both connect")
logger.info("Phase 1: Creating relay with owner", { phase = 1 })

local relay = indras.RelayNode.new({ owner = true })
local owner_id = relay:owner_player_id()

indras.assert.not_nil(owner_id, "Relay should have an owner player_id")

advance()
logger.info("Relay created", {
    tick = tick,
    owner_id = owner_id,
    data_dir = relay:data_dir(),
})

-- ============================================================================
-- PHASE 2: OWNER AUTHENTICATES, REGISTERS, STORES EVENT
-- ============================================================================

logger.info("Phase 2: Owner authenticates, registers interface, stores event", { phase = 2 })

local owner = indras.RelayClient.new_as_owner(relay)
local auth_owner = owner:authenticate()

indras.assert.true_(auth_owner.authenticated, "Owner should authenticate")

local iface_hex = string.rep("dd", 32)  -- 64-char hex interface ID
local reg = owner:register({ iface_hex })
indras.assert.true_(#reg.accepted == 1, "Interface should be accepted")

local ack = owner:store_event("Self_", iface_hex, "pre-shutdown-event")
indras.assert.true_(ack.accepted, "Pre-shutdown event should be stored")

-- Verify the event round-trips before shutdown
local del = owner:retrieve(iface_hex, "Self_")
indras.assert.true_(#del.events == 1, "Should retrieve 1 event before shutdown")

advance()
logger.event("owner_setup_complete", {
    tick = tick,
    authenticated = auth_owner.authenticated,
    event_stored = ack.accepted,
    event_retrieved = #del.events,
})

-- ============================================================================
-- PHASE 3: STRANGER AUTHENTICATES
-- ============================================================================

logger.info("Phase 3: Stranger authenticates", { phase = 3 })

local stranger = indras.RelayClient.new(relay)
local auth_stranger = stranger:authenticate()

indras.assert.true_(auth_stranger.authenticated, "Stranger should authenticate")

advance()
logger.event("stranger_authenticated", {
    tick = tick,
    player_id = stranger:player_id(),
    authenticated = auth_stranger.authenticated,
})

-- ============================================================================
-- PHASE 4: SHUTDOWN RELAY
-- ============================================================================

indras.narrative("The relay goes dark — both clients are left holding dead connections")
logger.info("Phase 4: Shutting down relay", { phase = 4 })

relay:shutdown()

advance()
logger.info("Relay shutdown called", { tick = tick })

-- ============================================================================
-- PHASE 5: OWNER PING — should fail
-- ============================================================================

indras.narrative("Owner knocks — silence answers")
logger.info("Phase 5: Owner ping after shutdown — expecting error", { phase = 5 })

local owner_ping_ok, owner_ping_err = pcall(function()
    return owner:ping()
end)

indras.assert.false_(owner_ping_ok, "Owner ping after shutdown should fail")

advance()
logger.event("owner_ping_post_shutdown", {
    tick = tick,
    succeeded = owner_ping_ok,
    error = tostring(owner_ping_err),
    correctly_failed = not owner_ping_ok,
})

-- ============================================================================
-- PHASE 6: STRANGER PING — should also fail
-- ============================================================================

indras.narrative("Stranger knocks — same silence")
logger.info("Phase 6: Stranger ping after shutdown — expecting error", { phase = 6 })

local stranger_ping_ok, stranger_ping_err = pcall(function()
    return stranger:ping()
end)

indras.assert.false_(stranger_ping_ok, "Stranger ping after shutdown should fail")

advance()
logger.event("stranger_ping_post_shutdown", {
    tick = tick,
    succeeded = stranger_ping_ok,
    error = tostring(stranger_ping_err),
    correctly_failed = not stranger_ping_ok,
})

-- ============================================================================
-- PHASE 7: CONFIRM GRACEFUL — no hang, no crash, scenario reached here
-- ============================================================================

indras.narrative("Scenario reached the end — the relay shut down without hanging")
logger.info("Phase 7: Graceful shutdown confirmed — scenario completed without crash or hang", { phase = 7 })

local shutdown_was_graceful = (not owner_ping_ok) and (not stranger_ping_ok)

advance()
logger.event("shutdown_summary", {
    tick = tick,
    owner_ping_failed_correctly = not owner_ping_ok,
    stranger_ping_failed_correctly = not stranger_ping_ok,
    graceful = shutdown_was_graceful,
})

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_relay_graceful_shutdown")

result:add_metrics({
    pre_shutdown_events_stored = 1,
    pre_shutdown_events_retrieved = #del.events,
    owner_ping_post_shutdown_failed = not owner_ping_ok,
    stranger_ping_post_shutdown_failed = not stranger_ping_ok,
    shutdown_graceful = shutdown_was_graceful,
    final_tick = tick,
})

result:record_assertion("relay_created", true, true, true)
result:record_assertion("owner_authenticated", auth_owner.authenticated, true, auth_owner.authenticated)
result:record_assertion("event_stored_pre_shutdown", ack.accepted, true, ack.accepted)
result:record_assertion("event_retrieved_pre_shutdown", #del.events == 1, true, #del.events == 1)
result:record_assertion("stranger_authenticated", auth_stranger.authenticated, true, auth_stranger.authenticated)
result:record_assertion("owner_ping_fails_post_shutdown", not owner_ping_ok, true, not owner_ping_ok)
result:record_assertion("stranger_ping_fails_post_shutdown", not stranger_ping_ok, true, not stranger_ping_ok)
result:record_assertion("shutdown_graceful", shutdown_was_graceful, true, shutdown_was_graceful)

local final_result = result:build()

logger.info("Live Relay Graceful Shutdown completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    graceful = shutdown_was_graceful,
    final_tick = tick,
})

return final_result
