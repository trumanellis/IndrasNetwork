-- Live Relay Tier Access — three-tier access control enforcement
--
-- Tests that tier boundaries are respected:
--   - Owner gets Self_, Connections, Public
--   - A synced contact gets Connections + Public
--   - A stranger gets Public only, and is rejected from Self_
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_relay_tier_access.lua

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_relay_tier_access")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live Relay Tier Access Test", {
    description = "Three-tier access control: owner, contact, and stranger",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE RELAY AND AUTHENTICATE OWNER
-- ============================================================================

indras.narrative("A relay opens its gates — owner key in hand")
logger.info("Phase 1: Create relay and authenticate owner", { phase = 1 })

local relay = indras.RelayNode.new({ owner = true })
local owner_client = indras.RelayClient.new_as_owner(relay)
local owner_auth = owner_client:authenticate()

indras.assert.true_(owner_auth.authenticated, "Owner should authenticate")

local has_self = false
local has_connections = false
local has_public = false
for _, tier in ipairs(owner_auth.granted_tiers) do
    if tier == "Self_" then has_self = true end
    if tier == "Connections" then has_connections = true end
    if tier == "Public" then has_public = true end
end

indras.assert.true_(has_self and has_connections and has_public,
    "Owner should have all three tiers")

advance()
logger.event("owner_authenticated", {
    tick = tick,
    player_id = owner_client:player_id(),
    tier_count = #owner_auth.granted_tiers,
})

-- ============================================================================
-- PHASE 2: CREATE CONTACT CLIENT, GET ITS PLAYER ID
-- ============================================================================

-- Contact is created FIRST so we have its player_id to sync
logger.info("Phase 2: Create contact client and capture player_id", { phase = 2 })

local contact_client = indras.RelayClient.new(relay)
local contact_id = contact_client:player_id()

indras.assert.not_nil(contact_id, "Contact should have a player_id")

advance()
logger.info("Contact client created", {
    tick = tick,
    contact_id = contact_id,
})

-- ============================================================================
-- PHASE 3: OWNER SYNCS CONTACT
-- ============================================================================

indras.narrative("Owner adds a contact — opening the Connections door")
logger.info("Phase 3: Owner syncs contact player_id", { phase = 3 })

local sync_ack = owner_client:sync_contacts({ contact_id })

indras.assert.true_(sync_ack.accepted, "Contact sync should be accepted")
indras.assert.true_(sync_ack.contact_count >= 1, "Contact count should be at least 1")

advance()
logger.event("contacts_synced", {
    tick = tick,
    contact_id = contact_id,
    accepted = sync_ack.accepted,
    contact_count = sync_ack.contact_count,
})

-- ============================================================================
-- PHASE 4: CONTACT AUTHENTICATES — SHOULD GET CONNECTIONS + PUBLIC
-- ============================================================================

logger.info("Phase 4: Contact authenticates, expects Connections + Public", { phase = 4 })

local contact_auth = contact_client:authenticate()

indras.assert.true_(contact_auth.authenticated, "Contact should authenticate")

local contact_has_connections = false
local contact_has_public = false
local contact_has_self = false
for _, tier in ipairs(contact_auth.granted_tiers) do
    if tier == "Connections" then contact_has_connections = true end
    if tier == "Public" then contact_has_public = true end
    if tier == "Self_" then contact_has_self = true end
end

indras.assert.true_(contact_has_connections, "Contact should have Connections tier")
indras.assert.true_(contact_has_public, "Contact should have Public tier")
indras.assert.true_(not contact_has_self, "Contact should NOT have Self_ tier")

advance()
logger.event("contact_authenticated", {
    tick = tick,
    player_id = contact_id,
    tier_count = #contact_auth.granted_tiers,
    has_connections = contact_has_connections,
    has_public = contact_has_public,
    has_self = contact_has_self,
})

-- ============================================================================
-- PHASE 5: STRANGER AUTHENTICATES — SHOULD GET PUBLIC ONLY
-- ============================================================================

indras.narrative("A stranger arrives — only the public gate is open")
logger.info("Phase 5: Stranger authenticates, expects Public only", { phase = 5 })

local stranger = indras.RelayClient.new(relay)
local stranger_auth = stranger:authenticate()

indras.assert.true_(stranger_auth.authenticated, "Stranger should authenticate")

local stranger_has_public = false
local stranger_has_connections = false
local stranger_has_self = false
for _, tier in ipairs(stranger_auth.granted_tiers) do
    if tier == "Public" then stranger_has_public = true end
    if tier == "Connections" then stranger_has_connections = true end
    if tier == "Self_" then stranger_has_self = true end
end

indras.assert.true_(stranger_has_public, "Stranger should have Public tier")
indras.assert.true_(not stranger_has_connections, "Stranger should NOT have Connections tier")
indras.assert.true_(not stranger_has_self, "Stranger should NOT have Self_ tier")

advance()
logger.event("stranger_authenticated", {
    tick = tick,
    player_id = stranger:player_id(),
    tier_count = #stranger_auth.granted_tiers,
    has_public = stranger_has_public,
    has_connections = stranger_has_connections,
    has_self = stranger_has_self,
})

-- ============================================================================
-- PHASE 6: STRANGER ATTEMPTS SELF_ STORE — SHOULD BE REJECTED
-- ============================================================================

logger.info("Phase 6: Stranger tries to store in Self_ tier — expect rejection", { phase = 6 })

local iface_hex = string.rep("99", 32)  -- 64-char hex interface ID
stranger:register({ iface_hex })
local rejected_ack = stranger:store_event("Self_", iface_hex, "unauthorized-attempt")

indras.assert.true_(not rejected_ack.accepted, "Stranger Self_ store should be rejected")

advance()
logger.event("self_store_rejected", {
    tick = tick,
    player_id = stranger:player_id(),
    tier = "Self_",
    accepted = rejected_ack.accepted,
    reason = rejected_ack.reason,
})

-- ============================================================================
-- PHASE 7: STRANGER STORES IN PUBLIC TIER — SHOULD SUCCEED
-- ============================================================================

logger.info("Phase 7: Stranger stores in Public tier — expect acceptance", { phase = 7 })

local public_iface_hex = string.rep("77", 32)
stranger:register({ public_iface_hex })
local public_ack = stranger:store_event("Public", public_iface_hex, "public-hello")

indras.assert.true_(public_ack.accepted, "Stranger Public store should be accepted")

advance()
logger.event("public_store_accepted", {
    tick = tick,
    player_id = stranger:player_id(),
    tier = "Public",
    accepted = public_ack.accepted,
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

local result = quest_helpers.result_builder("live_relay_tier_access")

result:add_metrics({
    owner_tiers = #owner_auth.granted_tiers,
    contact_tiers = #contact_auth.granted_tiers,
    stranger_tiers = #stranger_auth.granted_tiers,
    contact_sync_count = sync_ack.contact_count,
    stranger_self_rejected = not rejected_ack.accepted,
    stranger_public_accepted = public_ack.accepted,
    final_tick = tick,
})

result:record_assertion("owner_has_all_tiers", has_self and has_connections and has_public, true, has_self and has_connections and has_public)
result:record_assertion("contact_synced", sync_ack.accepted, true, sync_ack.accepted)
result:record_assertion("contact_has_connections", contact_has_connections, true, contact_has_connections)
result:record_assertion("contact_no_self", not contact_has_self, true, not contact_has_self)
result:record_assertion("stranger_public_only", stranger_has_public and not stranger_has_connections, true, stranger_has_public and not stranger_has_connections)
result:record_assertion("stranger_self_rejected", not rejected_ack.accepted, true, not rejected_ack.accepted)
result:record_assertion("stranger_public_accepted", public_ack.accepted, true, public_ack.accepted)

local final_result = result:build()

logger.info("Live Relay Tier Access completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    final_tick = tick,
})

return final_result
