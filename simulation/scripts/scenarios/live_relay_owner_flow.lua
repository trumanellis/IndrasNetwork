-- Live Relay Owner Flow — end-to-end happy path for a relay owner
--
-- Tests the full owner lifecycle: connect, authenticate, register an interface,
-- store an event in the Self_ tier, retrieve it, and verify round-trip latency.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_owner_flow.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_owner_flow")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live Relay Owner Flow", {
    description = "Owner end-to-end: auth, register, store, retrieve, ping",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE RELAY
-- ============================================================================

indras.narrative("A relay node comes to life — owner key generated")
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
-- PHASE 2: OWNER AUTHENTICATES
-- ============================================================================

indras.narrative("Owner knocks on the door — all three tiers open")
logger.info("Phase 2: Owner client authenticates", { phase = 2 })

local owner_client = indras.RelayClient.new_as_owner(relay)
local auth = owner_client:authenticate()

indras.assert.true_(auth.authenticated, "Owner should authenticate successfully")

-- Verify all three tiers are granted
local has_self = false
local has_connections = false
local has_public = false
for _, tier in ipairs(auth.granted_tiers) do
    if tier == "Self_" then has_self = true end
    if tier == "Connections" then has_connections = true end
    if tier == "Public" then has_public = true end
end

indras.assert.true_(has_self, "Owner should have Self_ tier")
indras.assert.true_(has_connections, "Owner should have Connections tier")
indras.assert.true_(has_public, "Owner should have Public tier")

advance()
logger.event("auth_result", {
    tick = tick,
    player_id = owner_client:player_id(),
    authenticated = auth.authenticated,
    tier_count = #auth.granted_tiers,
    has_self = has_self,
    has_connections = has_connections,
    has_public = has_public,
})

-- ============================================================================
-- PHASE 3: REGISTER INTERFACE
-- ============================================================================

logger.info("Phase 3: Registering interface", { phase = 3 })

local iface_hex = string.rep("42", 32)  -- 64-char hex interface ID
local reg = owner_client:register({ iface_hex })

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
-- PHASE 4: STORE EVENT IN SELF_ TIER
-- ============================================================================

logger.info("Phase 4: Storing event in Self_ tier", { phase = 4 })

local event_data = "owner-event-payload-001"
local ack = owner_client:store_event("Self_", iface_hex, event_data)

indras.assert.true_(ack.accepted, "Event should be accepted in Self_ tier")

advance()
logger.event("event_stored", {
    tick = tick,
    tier = "Self_",
    iface_hex = iface_hex:sub(1, 16) .. "...",
    data = event_data,
    accepted = ack.accepted,
})

-- ============================================================================
-- PHASE 5: RETRIEVE AND VERIFY
-- ============================================================================

logger.info("Phase 5: Retrieving events from Self_ tier", { phase = 5 })

local del = owner_client:retrieve(iface_hex, "Self_")

indras.assert.true_(#del.events == 1, "Should retrieve exactly 1 event")
indras.assert.eq(del.events[1].data, event_data, "Retrieved data should match stored data")

advance()
logger.event("event_retrieved", {
    tick = tick,
    event_count = #del.events,
    has_more = del.has_more,
    data_matches = (del.events[1].data == event_data),
    event_id = del.events[1].event_id,
})

-- ============================================================================
-- PHASE 6: PING
-- ============================================================================

logger.info("Phase 6: Measuring relay RTT", { phase = 6 })

local rtt = owner_client:ping()

indras.assert.true_(rtt > 0, "Ping RTT should be greater than zero")

advance()
logger.event("ping_result", {
    tick = tick,
    rtt_ms = rtt,
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

local result = quest_helpers.result_builder("live_relay_owner_flow")

result:add_metrics({
    relay_created = true,
    owner_authenticated = auth.authenticated,
    tiers_granted = #auth.granted_tiers,
    interface_accepted = #reg.accepted,
    event_stored = ack.accepted,
    event_retrieved = #del.events,
    data_round_trip_ok = (del.events[1].data == event_data),
    ping_rtt_ms = rtt,
    final_tick = tick,
})

result:record_assertion("relay_created", true, true, true)
result:record_assertion("owner_authenticated", auth.authenticated, true, auth.authenticated)
result:record_assertion("all_tiers_granted", has_self and has_connections and has_public, true, has_self and has_connections and has_public)
result:record_assertion("interface_registered", #reg.accepted == 1, true, #reg.accepted == 1)
result:record_assertion("event_accepted", ack.accepted, true, ack.accepted)
result:record_assertion("event_retrieved", #del.events == 1, true, #del.events == 1)
result:record_assertion("data_matches", del.events[1].data == event_data, true, del.events[1].data == event_data)
result:record_assertion("ping_positive", rtt > 0, true, rtt > 0)

local final_result = result:build()

logger.info("Live Relay Owner Flow completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    rtt_ms = rtt,
    final_tick = tick,
})

return final_result
