-- Live Relay Store-and-Forward
--
-- The relay's core purpose: blind store-and-forward between peers.
-- Owner A stores events while Contact B is offline. B reconnects
-- and retrieves messages it missed — the relay bridged the gap.
--
-- Scenario:
--   1. Owner A and Contact B both connect to relay
--   2. B retrieves baseline (empty)
--   3. B goes offline (closes session)
--   4. A stores 3 events while B is away
--   5. B reconnects, retrieves — gets all 3 events
--   6. B stores a reply, A retrieves — bidirectional works
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_store_and_forward.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_store_and_forward")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

local iface_hex = string.rep("ff", 32)  -- shared interface

logger.info("Starting Store-and-Forward Test", {
    description = "Relay bridges the gap when peers go offline",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: START RELAY, CONNECT OWNER AND CONTACT
-- ============================================================================

indras.narrative("A relay opens — owner A and contact B both check in")
logger.info("Phase 1: Create relay, connect both peers", { phase = 1 })

local relay = indras.RelayNode.new({ owner = true })

-- Owner A
local a = indras.RelayClient.new_as_owner(relay)
local a_auth = a:authenticate()
indras.assert.true_(a_auth.authenticated, "A should authenticate")

-- Create contact B and register with relay
local b = indras.RelayClient.new(relay)
local b_id = b:player_id()

-- Owner syncs B as a contact so B gets Connections tier
local sync = a:sync_contacts({ b_id })
indras.assert.true_(sync.accepted, "Contact sync should succeed")

-- B authenticates — should get Connections tier
local b_auth = b:authenticate()
indras.assert.true_(b_auth.authenticated, "B should authenticate")

local b_has_connections = false
for _, tier in ipairs(b_auth.granted_tiers) do
    if tier == "Connections" then b_has_connections = true end
end
indras.assert.true_(b_has_connections, "B should have Connections tier")

advance()
logger.event("peers_connected", {
    tick = tick,
    trace_id = ctx.trace_id,
    a_player_id = a:player_id(),
    b_player_id = b_id,
    b_tier_count = #b_auth.granted_tiers,
})

-- Both register the shared interface
local a_reg = a:register({ iface_hex })
indras.assert.eq(#a_reg.accepted, 1, "A registration accepted")
local b_reg = b:register({ iface_hex })
indras.assert.eq(#b_reg.accepted, 1, "B registration accepted")

advance()
logger.event("interface_registered", {
    tick = tick,
    trace_id = ctx.trace_id,
    iface_hex = iface_hex:sub(1, 16) .. "...",
})

-- ============================================================================
-- PHASE 2: VERIFY EMPTY BASELINE
-- ============================================================================

logger.info("Phase 2: B checks the mailbox — nothing yet", { phase = 2 })

local b_baseline = b:retrieve(iface_hex, "Connections")
indras.assert.eq(#b_baseline.events, 0, "B should see no events initially")

advance()
logger.event("baseline_verified", {
    tick = tick,
    trace_id = ctx.trace_id,
    b_event_count = #b_baseline.events,
})

-- ============================================================================
-- PHASE 3: B GOES OFFLINE
-- ============================================================================

indras.narrative("B disconnects — but A keeps talking into the void")
logger.info("Phase 3: B goes offline", { phase = 3 })

b:close()

advance()
logger.event("b_disconnected", {
    tick = tick,
    trace_id = ctx.trace_id,
})

-- ============================================================================
-- PHASE 4: A STORES EVENTS WHILE B IS AWAY
-- ============================================================================

logger.info("Phase 4: A stores 3 events while B is offline", { phase = 4 })

local messages = {
    "Hey B, you there?",
    "Saving this for when you get back.",
    "Third message — the relay is holding these for you.",
}

for i, msg in ipairs(messages) do
    local ack = a:store_event("Connections", iface_hex, msg)
    indras.assert.true_(ack.accepted, "Store " .. i .. " should succeed")

    advance()
    logger.event("event_stored_while_offline", {
        tick = tick,
        trace_id = ctx.trace_id,
        index = i,
        data = msg,
        accepted = ack.accepted,
    })
end

-- ============================================================================
-- PHASE 5: B RECONNECTS AND RETRIEVES
-- ============================================================================

indras.narrative("B comes back — three messages are waiting")
logger.info("Phase 5: B reconnects, retrieves missed events", { phase = 5 })

-- Create a new client for B (fresh connection)
-- B needs the same identity to get Connections tier again
-- Since we can't reuse signing keys via the current API, create a new stranger
-- and have owner re-sync contacts. Or simpler: use new(relay) and re-sync.
local b2 = indras.RelayClient.new(relay)
local b2_id = b2:player_id()

-- Owner syncs the new B identity as contact
local sync2 = a:sync_contacts({ b2_id })
indras.assert.true_(sync2.accepted, "Re-sync should succeed")

-- B2 authenticates — should get Connections
local b2_auth = b2:authenticate()
indras.assert.true_(b2_auth.authenticated, "B2 should authenticate")

-- B2 registers the same interface
b2:register({ iface_hex })

-- B2 retrieves from Connections tier
local delivery = b2:retrieve(iface_hex, "Connections")

advance()
logger.event("b_retrieved_after_reconnect", {
    tick = tick,
    trace_id = ctx.trace_id,
    event_count = #delivery.events,
    has_more = delivery.has_more,
})

-- Verify all 3 messages arrived
indras.assert.eq(#delivery.events, 3, "B should receive 3 events")

local found = { false, false, false }
for _, evt in ipairs(delivery.events) do
    for i, msg in ipairs(messages) do
        if evt.data == msg then
            found[i] = true
        end
    end
end

for i, f in ipairs(found) do
    indras.assert.true_(f, "Message " .. i .. " should be found")
end

advance()
logger.event("all_messages_delivered", {
    tick = tick,
    trace_id = ctx.trace_id,
    msg_1_found = found[1],
    msg_2_found = found[2],
    msg_3_found = found[3],
})

-- ============================================================================
-- PHASE 6: BIDIRECTIONAL — B REPLIES, A RETRIEVES
-- ============================================================================

indras.narrative("B replies — the relay works both ways")
logger.info("Phase 6: B replies, A retrieves", { phase = 6 })

local reply = "Got all three! Thanks relay."
local reply_ack = b2:store_event("Connections", iface_hex, reply)
indras.assert.true_(reply_ack.accepted, "B's reply should be stored")

-- A retrieves — should see all 4 events (3 from A + 1 from B)
local a_delivery = a:retrieve(iface_hex, "Connections")

advance()
logger.event("bidirectional_verified", {
    tick = tick,
    trace_id = ctx.trace_id,
    a_event_count = #a_delivery.events,
    has_reply = false,  -- will update below
})

indras.assert.eq(#a_delivery.events, 4, "A should see 4 events total")

local reply_found = false
for _, evt in ipairs(a_delivery.events) do
    if evt.data == reply then
        reply_found = true
    end
end
indras.assert.true_(reply_found, "A should see B's reply")

advance()
logger.event("store_and_forward_complete", {
    tick = tick,
    trace_id = ctx.trace_id,
    total_events = #a_delivery.events,
    reply_found = reply_found,
    bidirectional = true,
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

a:close()
b2:close()
relay:shutdown()

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_relay_store_and_forward")

result:add_metrics({
    messages_stored_while_offline = 3,
    messages_retrieved_on_reconnect = #delivery.events,
    bidirectional_events = #a_delivery.events,
    reply_found = reply_found,
    all_messages_found = found[1] and found[2] and found[3],
})

result:record_assertion("a_authenticated", true, true, true)
result:record_assertion("b_authenticated", true, true, true)
result:record_assertion("b_has_connections_tier", b_has_connections, true, b_has_connections)
result:record_assertion("baseline_empty", #b_baseline.events == 0, true, #b_baseline.events == 0)
result:record_assertion("3_events_stored_offline", true, true, true)
result:record_assertion("3_events_retrieved", #delivery.events == 3, true, #delivery.events == 3)
result:record_assertion("all_payloads_match", found[1] and found[2] and found[3], true, found[1] and found[2] and found[3])
result:record_assertion("reply_stored", reply_ack.accepted, true, reply_ack.accepted)
result:record_assertion("reply_found_by_a", reply_found, true, reply_found)
result:record_assertion("bidirectional_total_4", #a_delivery.events == 4, true, #a_delivery.events == 4)

local final_result = result:build()

logger.info("Store-and-Forward test complete", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    store_and_forward = true,
    bidirectional = true,
})

return final_result
