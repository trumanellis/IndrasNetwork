-- Live Relay Quota — public tier storage quota enforcement
--
-- Tests that a stranger (Public tier) cannot store events beyond the relay's
-- configured byte limit. Uses a small quota so the third store is rejected.
--
-- Sequence:
-- 1. Create relay with public_max_bytes = 512 (no owner key needed)
-- 2. Stranger authenticates — gets Public tier
-- 3. Register interface
-- 4. Store 200-byte event → accepted
-- 5. Store 200-byte event → accepted (cumulative ~460 bytes serialized)
-- 6. Store 200-byte event → REJECTED (would exceed 512 bytes)
-- 7. Retrieve — verify exactly 2 events stored
-- 8. Shutdown
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_quota.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_quota")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live Relay Quota Test", {
    description = "Public tier quota: third store rejected when limit exceeded",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE RELAY WITH SMALL PUBLIC QUOTA
-- ============================================================================

indras.narrative("A relay opens its doors — but with strict storage limits")
logger.info("Phase 1: Creating relay with public_max_bytes = 512", { phase = 1 })

local relay = indras.RelayNode.new({ public_max_bytes = 512 })

advance()
logger.info("Relay created", {
    tick = tick,
    public_max_bytes = 512,
    data_dir = relay:data_dir(),
})

-- ============================================================================
-- PHASE 2: STRANGER AUTHENTICATES
-- ============================================================================

indras.narrative("A stranger arrives — granted only the Public tier")
logger.info("Phase 2: Stranger authenticates", { phase = 2 })

local stranger = indras.RelayClient.new(relay)
local auth = stranger:authenticate()

indras.assert.true_(auth.authenticated, "Stranger should authenticate")

local has_public = false
for _, tier in ipairs(auth.granted_tiers) do
    if tier == "Public" then has_public = true end
end

indras.assert.true_(has_public, "Stranger should have Public tier")

advance()
logger.event("auth_result", {
    tick = tick,
    player_id = stranger:player_id(),
    authenticated = auth.authenticated,
    tier_count = #auth.granted_tiers,
    has_public = has_public,
})

-- ============================================================================
-- PHASE 3: REGISTER INTERFACE
-- ============================================================================

logger.info("Phase 3: Registering interface", { phase = 3 })

local iface_hex = string.rep("ab", 32)  -- 64-char hex interface ID
local reg = stranger:register({ iface_hex })

indras.assert.true_(#reg.accepted == 1, "Interface should be accepted")
indras.assert.true_(#reg.rejected == 0, "No interfaces should be rejected")

advance()
logger.event("interface_registered", {
    tick = tick,
    iface_hex = iface_hex:sub(1, 16) .. "...",
    accepted_count = #reg.accepted,
    rejected_count = #reg.rejected,
})

-- ============================================================================
-- PHASE 4: FIRST STORE — should be accepted
-- ============================================================================

indras.narrative("First payload lands safely within quota")
logger.info("Phase 4: Storing first 200-byte event", { phase = 4 })

local payload_200 = string.rep("x", 200)
local ack1 = stranger:store_event("Public", iface_hex, payload_200)

indras.assert.true_(ack1.accepted, "First 200-byte store should be accepted")

advance()
logger.event("event_stored", {
    tick = tick,
    store_number = 1,
    payload_bytes = #payload_200,
    accepted = ack1.accepted,
    reason = ack1.reason,
})

-- ============================================================================
-- PHASE 5: SECOND STORE — should be accepted
-- ============================================================================

logger.info("Phase 5: Storing second 200-byte event", { phase = 5 })

local ack2 = stranger:store_event("Public", iface_hex, payload_200)

indras.assert.true_(ack2.accepted, "Second 200-byte store should be accepted")

advance()
logger.event("event_stored", {
    tick = tick,
    store_number = 2,
    payload_bytes = #payload_200,
    accepted = ack2.accepted,
    reason = ack2.reason,
})

-- ============================================================================
-- PHASE 6: THIRD STORE — should be REJECTED (quota exceeded)
-- ============================================================================

indras.narrative("The third payload tips the scale — quota enforced")
logger.info("Phase 6: Storing third 200-byte event — expect rejection", { phase = 6 })

local ack3 = stranger:store_event("Public", iface_hex, payload_200)

indras.assert.true_(not ack3.accepted, "Third 200-byte store should be REJECTED (quota exceeded)")

advance()
logger.event("event_rejected", {
    tick = tick,
    store_number = 3,
    payload_bytes = #payload_200,
    accepted = ack3.accepted,
    reason = ack3.reason,
})

-- ============================================================================
-- PHASE 7: RETRIEVE — verify exactly 2 events stored
-- ============================================================================

logger.info("Phase 7: Retrieving events — expecting exactly 2", { phase = 7 })

local del = stranger:retrieve(iface_hex, "Public")

indras.assert.true_(#del.events == 2, "Should retrieve exactly 2 events (third was rejected)")

advance()
logger.event("events_retrieved", {
    tick = tick,
    event_count = #del.events,
    has_more = del.has_more,
    expected_count = 2,
    count_correct = (#del.events == 2),
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

stranger:close()
relay:shutdown()

advance()
logger.info("Relay shut down cleanly", { tick = tick })

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_relay_quota")

result:add_metrics({
    public_max_bytes = 512,
    payload_bytes_each = 200,
    store1_accepted = ack1.accepted,
    store2_accepted = ack2.accepted,
    store3_accepted = ack3.accepted,
    events_retrieved = #del.events,
    final_tick = tick,
})

result:record_assertion("relay_created", true, true, true)
result:record_assertion("stranger_authenticated", auth.authenticated, true, auth.authenticated)
result:record_assertion("public_tier_granted", has_public, true, has_public)
result:record_assertion("interface_registered", #reg.accepted == 1, true, #reg.accepted == 1)
result:record_assertion("store1_accepted", ack1.accepted, true, ack1.accepted)
result:record_assertion("store2_accepted", ack2.accepted, true, ack2.accepted)
result:record_assertion("store3_rejected", not ack3.accepted, true, not ack3.accepted)
result:record_assertion("retrieved_2_events", #del.events == 2, true, #del.events == 2)

local final_result = result:build()

logger.info("Live Relay Quota completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    quota_enforced = not ack3.accepted,
    final_tick = tick,
})

return final_result
