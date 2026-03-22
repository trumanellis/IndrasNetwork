-- Live Relay Multi-Client — concurrent clients with isolated storage
--
-- Three clients connect simultaneously: owner (Self_), contact (Connections),
-- and stranger (Public). Each writes to its own interface and verifies that
-- retrieval is isolated — no cross-contamination between interfaces or tiers.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_multi_client.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_multi_client")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live Relay Multi-Client Test", {
    description = "3 concurrent clients, isolated interfaces, no cross-contamination",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE RELAY, CLIENTS, AND ESTABLISH TIERS
-- ============================================================================

indras.narrative("Three clients approach the relay — each with different credentials")
logger.info("Phase 1: Create relay and all clients", { phase = 1 })

local relay = indras.RelayNode.new({ owner = true })

-- Contact client is created before owner syncs, so its player_id is available
local contact_client = indras.RelayClient.new(relay)
local contact_id = contact_client:player_id()

local owner_client = indras.RelayClient.new_as_owner(relay)
local stranger = indras.RelayClient.new(relay)

-- Owner authenticates first, then syncs contacts
local owner_auth = owner_client:authenticate()
indras.assert.true_(owner_auth.authenticated, "Owner should authenticate")

-- Sync contact so it gains Connections tier
local sync_ack = owner_client:sync_contacts({ contact_id })
indras.assert.true_(sync_ack.accepted, "Contact sync should succeed")

-- Contact and stranger now authenticate
local contact_auth = contact_client:authenticate()
local stranger_auth = stranger:authenticate()
indras.assert.true_(contact_auth.authenticated, "Contact should authenticate")
indras.assert.true_(stranger_auth.authenticated, "Stranger should authenticate")

advance()
logger.event("clients_ready", {
    tick = tick,
    owner_id = owner_client:player_id(),
    contact_id = contact_id,
    stranger_id = stranger:player_id(),
    owner_tier_count = #owner_auth.granted_tiers,
    contact_tier_count = #contact_auth.granted_tiers,
    stranger_tier_count = #stranger_auth.granted_tiers,
})

-- ============================================================================
-- PHASE 2: EACH CLIENT REGISTERS A UNIQUE INTERFACE
-- ============================================================================

logger.info("Phase 2: Register unique interfaces per client", { phase = 2 })

local owner_iface   = string.rep("aa", 32)   -- 64-char hex
local contact_iface = string.rep("bb", 32)
local stranger_iface = string.rep("cc", 32)

local owner_reg   = owner_client:register({ owner_iface })
local contact_reg = contact_client:register({ contact_iface })
local stranger_reg = stranger:register({ stranger_iface })

indras.assert.true_(#owner_reg.accepted == 1,   "Owner interface should be accepted")
indras.assert.true_(#contact_reg.accepted == 1,  "Contact interface should be accepted")
indras.assert.true_(#stranger_reg.accepted == 1, "Stranger interface should be accepted")

advance()
logger.event("interfaces_registered", {
    tick = tick,
    owner_accepted = #owner_reg.accepted,
    contact_accepted = #contact_reg.accepted,
    stranger_accepted = #stranger_reg.accepted,
})

-- ============================================================================
-- PHASE 3: EACH CLIENT STORES AN EVENT IN THEIR HIGHEST TIER
-- ============================================================================

indras.narrative("Each client writes to its slice of the relay")
logger.info("Phase 3: Store events in highest accessible tier", { phase = 3 })

local owner_data   = "owner-self-event"
local contact_data = "contact-connections-event"
local stranger_data = "stranger-public-event"

local owner_ack   = owner_client:store_event("Self_",       owner_iface,   owner_data)
local contact_ack = contact_client:store_event("Connections", contact_iface, contact_data)
local stranger_ack = stranger:store_event("Public",        stranger_iface, stranger_data)

indras.assert.true_(owner_ack.accepted,   "Owner Self_ store should be accepted")
indras.assert.true_(contact_ack.accepted,  "Contact Connections store should be accepted")
indras.assert.true_(stranger_ack.accepted, "Stranger Public store should be accepted")

advance()
logger.event("events_stored", {
    tick = tick,
    owner_accepted   = owner_ack.accepted,
    contact_accepted = contact_ack.accepted,
    stranger_accepted = stranger_ack.accepted,
})

-- ============================================================================
-- PHASE 4: EACH CLIENT RETRIEVES FROM ITS OWN INTERFACE
-- ============================================================================

logger.info("Phase 4: Retrieve from own interface, verify isolation", { phase = 4 })

local owner_del   = owner_client:retrieve(owner_iface,   "Self_")
local contact_del = contact_client:retrieve(contact_iface, "Connections")
local stranger_del = stranger:retrieve(stranger_iface,    "Public")

-- Each should see exactly 1 event with matching data
indras.assert.true_(#owner_del.events == 1,
    "Owner should retrieve exactly 1 event")
indras.assert.eq(owner_del.events[1].data, owner_data,
    "Owner data should match")

indras.assert.true_(#contact_del.events == 1,
    "Contact should retrieve exactly 1 event")
indras.assert.eq(contact_del.events[1].data, contact_data,
    "Contact data should match")

indras.assert.true_(#stranger_del.events == 1,
    "Stranger should retrieve exactly 1 event")
indras.assert.eq(stranger_del.events[1].data, stranger_data,
    "Stranger data should match")

local owner_isolated   = #owner_del.events == 1 and owner_del.events[1].data == owner_data
local contact_isolated = #contact_del.events == 1 and contact_del.events[1].data == contact_data
local stranger_isolated = #stranger_del.events == 1 and stranger_del.events[1].data == stranger_data

advance()
logger.event("retrieval_verified", {
    tick = tick,
    owner_events   = #owner_del.events,
    contact_events = #contact_del.events,
    stranger_events = #stranger_del.events,
    owner_isolated   = owner_isolated,
    contact_isolated = contact_isolated,
    stranger_isolated = stranger_isolated,
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

owner_client:close()
contact_client:close()
stranger:close()
relay:shutdown()

advance()
logger.info("Relay shut down cleanly", { tick = tick })

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_relay_multi_client")

result:add_metrics({
    client_count = 3,
    all_authenticated = owner_auth.authenticated and contact_auth.authenticated and stranger_auth.authenticated,
    all_interfaces_registered = (#owner_reg.accepted + #contact_reg.accepted + #stranger_reg.accepted) == 3,
    all_events_stored = owner_ack.accepted and contact_ack.accepted and stranger_ack.accepted,
    owner_isolated   = owner_isolated,
    contact_isolated = contact_isolated,
    stranger_isolated = stranger_isolated,
    final_tick = tick,
})

result:record_assertion("all_clients_authenticated", owner_auth.authenticated and contact_auth.authenticated and stranger_auth.authenticated, true, owner_auth.authenticated and contact_auth.authenticated and stranger_auth.authenticated)
result:record_assertion("all_interfaces_registered", (#owner_reg.accepted + #contact_reg.accepted + #stranger_reg.accepted) == 3, true, (#owner_reg.accepted + #contact_reg.accepted + #stranger_reg.accepted) == 3)
result:record_assertion("all_events_accepted", owner_ack.accepted and contact_ack.accepted and stranger_ack.accepted, true, owner_ack.accepted and contact_ack.accepted and stranger_ack.accepted)
result:record_assertion("owner_isolation", owner_isolated, true, owner_isolated)
result:record_assertion("contact_isolation", contact_isolated, true, contact_isolated)
result:record_assertion("stranger_isolation", stranger_isolated, true, stranger_isolated)

local final_result = result:build()

logger.info("Live Relay Multi-Client completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = tick,
})

return final_result
