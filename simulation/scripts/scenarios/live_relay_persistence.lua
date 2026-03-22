-- Live Relay Persistence — multiple store/retrieve cycles across tiers
--
-- Tests that events stored in separate tiers (Self_ and Connections) remain
-- independently retrievable and do not bleed across tier boundaries. Verifies
-- that sequential stores accumulate correctly and retrieval returns them all.
--
-- Note: True restart persistence (surviving relay shutdown/restart with the
-- same owner key) requires a `data_dir` config option in the Lua binding —
-- tracked as a follow-up. This scenario exercises in-process persistence
-- of multiple events across two tiers instead.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_persistence.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_persistence")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live Relay Persistence Test", {
    description = "Multiple store/retrieve cycles, cross-tier isolation",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE RELAY AND AUTHENTICATE OWNER
-- ============================================================================

indras.narrative("Relay initialises — owner prepares to write a sequence of events")
logger.info("Phase 1: Create relay and authenticate owner", { phase = 1 })

local relay = indras.RelayNode.new({ owner = true })
local owner_client = indras.RelayClient.new_as_owner(relay)
local auth = owner_client:authenticate()

indras.assert.true_(auth.authenticated, "Owner should authenticate")

advance()
logger.info("Owner authenticated", {
    tick = tick,
    player_id = owner_client:player_id(),
    tier_count = #auth.granted_tiers,
})

-- ============================================================================
-- PHASE 2: REGISTER INTERFACES FOR BOTH TIERS
-- ============================================================================

logger.info("Phase 2: Register interfaces for Self_ and Connections tiers", { phase = 2 })

local self_iface  = string.rep("11", 32)   -- 64-char hex, used with Self_
local conn_iface  = string.rep("22", 32)   -- 64-char hex, used with Connections

local reg = owner_client:register({ self_iface, conn_iface })

indras.assert.true_(#reg.accepted == 2, "Both interfaces should be accepted")
indras.assert.true_(#reg.rejected == 0, "No interfaces should be rejected")

advance()
logger.event("interfaces_registered", {
    tick = tick,
    accepted_count = #reg.accepted,
    rejected_count = #reg.rejected,
})

-- ============================================================================
-- PHASE 3: STORE 5 SEQUENTIAL EVENTS IN SELF_ TIER
-- ============================================================================

indras.narrative("Owner writes five events into the Self_ tier")
logger.info("Phase 3: Storing 5 events in Self_ tier", { phase = 3 })

local self_payloads = {
    "self-event-001",
    "self-event-002",
    "self-event-003",
    "self-event-004",
    "self-event-005",
}

local self_accepted = 0
for i, payload in ipairs(self_payloads) do
    local ack = owner_client:store_event("Self_", self_iface, payload)
    if ack.accepted then
        self_accepted = self_accepted + 1
    end
    advance()
    logger.event("event_stored", {
        tick = tick,
        tier = "Self_",
        index = i,
        payload = payload,
        accepted = ack.accepted,
    })
end

indras.assert.true_(self_accepted == 5, "All 5 Self_ events should be accepted")

-- ============================================================================
-- PHASE 4: RETRIEVE FROM SELF_ TIER — VERIFY ALL 5 EVENTS
-- ============================================================================

logger.info("Phase 4: Retrieve Self_ events, expect 5", { phase = 4 })

local self_del = owner_client:retrieve(self_iface, "Self_")

indras.assert.true_(#self_del.events == 5, "Should retrieve exactly 5 Self_ events")

-- Verify each payload is present (order not guaranteed, check as a set)
local found = {}
for _, ev in ipairs(self_del.events) do
    found[ev.data] = true
end
local all_found = true
for _, payload in ipairs(self_payloads) do
    if not found[payload] then
        all_found = false
    end
end

indras.assert.true_(all_found, "All 5 Self_ payloads should be present in retrieval")

advance()
logger.event("self_retrieved", {
    tick = tick,
    event_count = #self_del.events,
    has_more = self_del.has_more,
    all_payloads_found = all_found,
})

-- ============================================================================
-- PHASE 5: STORE 3 EVENTS IN CONNECTIONS TIER
-- ============================================================================

indras.narrative("Owner switches to the Connections tier and writes three more")
logger.info("Phase 5: Storing 3 events in Connections tier", { phase = 5 })

local conn_payloads = {
    "conn-event-alpha",
    "conn-event-beta",
    "conn-event-gamma",
}

local conn_accepted = 0
for i, payload in ipairs(conn_payloads) do
    local ack = owner_client:store_event("Connections", conn_iface, payload)
    if ack.accepted then
        conn_accepted = conn_accepted + 1
    end
    advance()
    logger.event("event_stored", {
        tick = tick,
        tier = "Connections",
        index = i,
        payload = payload,
        accepted = ack.accepted,
    })
end

indras.assert.true_(conn_accepted == 3, "All 3 Connections events should be accepted")

-- ============================================================================
-- PHASE 6: RETRIEVE FROM CONNECTIONS TIER — VERIFY 3, NOT 5
-- ============================================================================

logger.info("Phase 6: Retrieve Connections events, expect 3 (isolated from Self_)", { phase = 6 })

local conn_del = owner_client:retrieve(conn_iface, "Connections")

indras.assert.true_(#conn_del.events == 3, "Should retrieve exactly 3 Connections events")

-- Verify all 3 Connections payloads are present
local conn_found = {}
for _, ev in ipairs(conn_del.events) do
    conn_found[ev.data] = true
end
local conn_all_found = true
for _, payload in ipairs(conn_payloads) do
    if not conn_found[payload] then
        conn_all_found = false
    end
end

indras.assert.true_(conn_all_found, "All 3 Connections payloads should be present")

-- Also confirm Self_ data does NOT appear in Connections retrieval
local no_self_bleed = not conn_found["self-event-001"]

advance()
logger.event("connections_retrieved", {
    tick = tick,
    event_count = #conn_del.events,
    has_more = conn_del.has_more,
    all_payloads_found = conn_all_found,
    no_self_bleed = no_self_bleed,
})

-- ============================================================================
-- PHASE 7: RE-READ SELF_ TO CONFIRM IT IS UNCHANGED
-- ============================================================================

logger.info("Phase 7: Re-read Self_ tier to confirm Connections writes did not affect it", { phase = 7 })

local self_del2 = owner_client:retrieve(self_iface, "Self_")

indras.assert.true_(#self_del2.events == 5, "Self_ should still have exactly 5 events after Connections writes")

advance()
logger.event("self_reread", {
    tick = tick,
    event_count = #self_del2.events,
    unchanged = (#self_del2.events == 5),
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

owner_client:close()
relay:shutdown()

advance()
logger.info("Relay shut down cleanly", { tick = tick })

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_relay_persistence")

result:add_metrics({
    owner_authenticated = auth.authenticated,
    self_events_stored  = self_accepted,
    self_events_retrieved = #self_del.events,
    conn_events_stored  = conn_accepted,
    conn_events_retrieved = #conn_del.events,
    self_payloads_verified = all_found,
    conn_payloads_verified = conn_all_found,
    no_self_bleed = no_self_bleed,
    self_unchanged_after_conn = (#self_del2.events == 5),
    final_tick = tick,
})

result:record_assertion("owner_authenticated", auth.authenticated, true, auth.authenticated)
result:record_assertion("both_interfaces_registered", #reg.accepted == 2, true, #reg.accepted == 2)
result:record_assertion("all_self_stored", self_accepted == 5, true, self_accepted == 5)
result:record_assertion("all_self_retrieved", #self_del.events == 5, true, #self_del.events == 5)
result:record_assertion("self_payloads_correct", all_found, true, all_found)
result:record_assertion("all_conn_stored", conn_accepted == 3, true, conn_accepted == 3)
result:record_assertion("all_conn_retrieved", #conn_del.events == 3, true, #conn_del.events == 3)
result:record_assertion("conn_payloads_correct", conn_all_found, true, conn_all_found)
result:record_assertion("no_cross_tier_bleed", no_self_bleed, true, no_self_bleed)
result:record_assertion("self_unchanged_after_conn", #self_del2.events == 5, true, #self_del2.events == 5)

local final_result = result:build()

logger.info("Live Relay Persistence completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = tick,
})

return final_result
