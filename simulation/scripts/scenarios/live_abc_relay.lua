-- Live ABC Relay — store-and-forward over real P2P
--
-- Tests that messages sent while a peer is offline are delivered
-- when it comes back online, using real IndrasNode instances with
-- QUIC transport and CRDT sync.
--
-- Topology: Light - Valor - Honor (all connected via shared interface)
-- Scenario: Honor goes offline, Light sends a message, Honor comes
-- back online and receives it through CRDT sync.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_abc_relay.lua
--
-- With viewer:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_abc_relay.lua \
--     | cargo run -p indras-realm-viewer --bin omni-viewer-v2

local quest_helpers = require("lib.quest_helpers")

-- ============================================================================
-- SETUP
-- ============================================================================

local ctx = quest_helpers.new_context("live_abc_relay")
local logger = quest_helpers.create_logger(ctx)

local tick = 0
local function advance()
    tick = tick + 1
    return tick
end

logger.info("Starting Live ABC Relay Test", {
    description = "Store-and-forward: message delivery after offline peer reconnects",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE AND START ALL THREE NODES
-- ============================================================================

indras.narrative("Three peers form a network — then one goes dark")
logger.info("Phase 1: Creating three LiveNode instances", { phase = 1 })

local light = indras.LiveNode.new()
local valor = indras.LiveNode.new()

-- Honor uses an explicit directory so files survive GC
-- (TempDir auto-deletes on drop, which kills keys & storage)
local honor_data_dir = os.tmpname() .. "_honor_node"
os.execute("mkdir -p " .. honor_data_dir)
local honor = indras.LiveNode.new(honor_data_dir)

light:start()
valor:start()
honor:start()
indras.assert.true_(light:is_started(), "Light should be started")
indras.assert.true_(valor:is_started(), "Valor should be started")
indras.assert.true_(honor:is_started(), "Honor should be started")

local peer_light = light:identity()
local peer_valor = valor:identity()
local peer_honor = honor:identity()

advance()
logger.info("All nodes started", {
    tick = tick,
    light_id = peer_light,
    valor_id = peer_valor,
    honor_id = peer_honor,
})

-- ============================================================================
-- PHASE 2: CREATE SHARED INTERFACE — Light creates, others join
-- ============================================================================

logger.info("Phase 2: Create shared interface", { phase = 2 })

local realm_id, invite = light:create_interface("Relay Test")
local valor_realm = valor:join_interface(invite)
local honor_realm = honor:join_interface(invite)
indras.assert.eq(valor_realm, realm_id, "Valor's realm ID should match")
indras.assert.eq(honor_realm, realm_id, "Honor's realm ID should match")

advance()
logger.event("realm_created", {
    tick = tick,
    realm_id = realm_id,
    members = table.concat({peer_light, peer_valor, peer_honor}, ","),
    member_count = 3,
})

for _, member in ipairs({peer_light, peer_valor, peer_honor}) do
    advance()
    logger.event("member_joined", {
        tick = tick,
        realm_id = realm_id,
        member = member,
    })
end

advance()
logger.event("realm_alias_set", {
    tick = tick,
    realm_id = realm_id,
    member = peer_light,
    alias = "Relay Test",
})

-- Verify all three see each other
local members_before = light:members(realm_id)
logger.info("Interface created, all members joined", {
    tick = tick,
    realm_id = realm_id:sub(1, 16) .. "...",
    member_count = #members_before,
})

-- Send initial messages to confirm connectivity
light:send_message(realm_id, "Light here — all connected?")
valor:send_message(realm_id, "Valor checking in!")
honor:send_message(realm_id, "Honor online and ready.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_light,
    realm_id = realm_id,
    content = "Light here — all connected?",
    message_type = "text",
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_valor,
    realm_id = realm_id,
    content = "Valor checking in!",
    message_type = "text",
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_honor,
    realm_id = realm_id,
    content = "Honor online and ready.",
    message_type = "text",
})

-- Record baseline: how many events does Honor see right now?
local honor_events_before = honor:events_since(realm_id, 0)
local honor_baseline = #honor_events_before

logger.info("Baseline established", {
    tick = tick,
    honor_events = honor_baseline,
})

-- ============================================================================
-- PHASE 3: HONOR GOES OFFLINE
-- ============================================================================

indras.narrative("Honor drops off the network — but the others keep talking")
logger.info("Phase 3: Honor goes offline", { phase = 3 })

honor:stop()
indras.assert.true_(not honor:is_started(), "Honor should be stopped")

advance()
logger.event("member_left", {
    tick = tick,
    realm_id = realm_id,
    member = peer_honor,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_honor,
    realm_id = realm_id,
    content = "[Honor went offline]",
    message_type = "system",
})

logger.info("Honor is now offline", {
    tick = tick,
    honor_started = honor:is_started(),
})

-- ============================================================================
-- PHASE 4: SEND MESSAGES WHILE HONOR IS OFFLINE
-- ============================================================================

logger.info("Phase 4: Messages sent while Honor is offline", { phase = 4 })

-- Light sends the key message
light:send_message(realm_id, "Hello Honor! Hope you get this when you're back.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_light,
    realm_id = realm_id,
    content = "Hello Honor! Hope you get this when you're back.",
    message_type = "text",
})

-- Valor sends too
valor:send_message(realm_id, "Miss you Honor! Saving these messages for you.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_valor,
    realm_id = realm_id,
    content = "Miss you Honor! Saving these messages for you.",
    message_type = "text",
})

-- Verify Light and Valor can see the messages
local light_events = light:events_since(realm_id, 0)
local valor_events = valor:events_since(realm_id, 0)

indras.assert.true_(#light_events > honor_baseline,
    "Light should have more events than Honor's baseline")
indras.assert.true_(#valor_events > 0,
    "Valor should have events")

logger.info("Messages sent while Honor offline", {
    tick = tick,
    light_events = #light_events,
    valor_events = #valor_events,
    honor_offline = true,
})

-- ============================================================================
-- PHASE 5: HONOR COMES BACK ONLINE (new node, same data dir)
-- ============================================================================

indras.narrative("Honor returns — will the missed messages arrive?")
logger.info("Phase 5: Honor comes back online", { phase = 5 })

-- Drop the old node reference and force GC to release the database lock
honor = nil
collectgarbage("collect")
collectgarbage("collect")  -- double-collect to handle weak refs

-- Create a fresh node from the same data dir (keys + interfaces persist on disk)
honor = indras.LiveNode.new(honor_data_dir)
honor:start()
indras.assert.true_(honor:is_started(), "Honor should be started again")

-- Honor should have the same identity (loaded from keystore)
local peer_honor_2 = honor:identity()

-- Rejoin the interface to re-establish gossip subscriptions
honor:join_interface(invite)

-- Wait for Honor's transport to fully initialize (relay connection setup)
indras.sleep(3)

-- Close stale connections on Light and Valor (they still think old Honor is alive)
-- Without this, connect() reuses the dead QUIC connection
local honor_full = honor:identity_full()
light:disconnect_from(honor_full)
valor:disconnect_from(honor_full)

-- Establish fresh connections: Honor connects TO the established nodes
-- (Honor knows their addresses; they've been running the whole time)
honor:connect_to(light)
honor:connect_to(valor)

advance()
logger.event("member_joined", {
    tick = tick,
    realm_id = realm_id,
    member = peer_honor_2,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_honor_2,
    realm_id = realm_id,
    content = "[Honor came back online]",
    message_type = "system",
})

logger.info("Honor is back online", {
    tick = tick,
    honor_started = honor:is_started(),
    honor_identity = peer_honor_2,
    same_identity = (peer_honor == peer_honor_2),
})

-- ============================================================================
-- PHASE 6: CHECK SYNC — Did Honor receive missed messages?
-- ============================================================================

logger.info("Phase 6: Checking CRDT sync (polling for sync...)", { phase = 6 })

-- Poll for sync completion instead of fixed sleep.
-- Sync task runs every 5s; we check every 2s for up to 30s.
local honor_doc = {}
local honor_synced = false
local max_polls = 15
for poll = 1, max_polls do
    indras.sleep(2)
    honor_doc = honor:document_events(realm_id)
    honor_synced = #honor_doc > honor_baseline
    logger.info("Sync poll", {
        tick = tick,
        poll = poll,
        honor_doc_events = #honor_doc,
        honor_baseline = honor_baseline,
        honor_synced = honor_synced,
    })
    if honor_synced then
        break
    end
end

-- Check Honor's local events (what it persisted before going offline)
local honor_local = honor:events_since(realm_id, 0)

-- Also check what Light and Valor see now
local light_final = light:events_since(realm_id, 0)
local valor_final = valor:events_since(realm_id, 0)

advance()
logger.info("Sync results", {
    tick = tick,
    honor_local_events = #honor_local,
    honor_doc_events = #honor_doc,
    honor_baseline = honor_baseline,
    honor_synced = honor_synced,
    light_events = #light_final,
    valor_events = #valor_final,
})

-- Log individual events Honor sees
for i, e in ipairs(honor_doc) do
    logger.info("Honor doc event", {
        tick = tick,
        index = i,
        sender = e.sender,
        content = e.content,
        sequence = e.sequence,
    })
end

-- Honor sends a confirmation message
honor:send_message(realm_id, "I'm back! Checking if I got your messages...")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_honor_2,
    realm_id = realm_id,
    content = "I'm back! Checking if I got your messages...",
    message_type = "text",
})

-- ============================================================================
-- PHASE 7: FINAL VERIFICATION
-- ============================================================================

logger.info("Phase 7: Final verification", { phase = 7 })

-- Re-check after Honor has had a chance to sync
local honor_final_local = honor:events_since(realm_id, 0)
local honor_final_doc = honor:document_events(realm_id)

advance()
logger.info("Final state", {
    tick = tick,
    honor_local_count = #honor_final_local,
    honor_doc_count = #honor_final_doc,
    light_count = #light_final,
    valor_count = #valor_final,
    store_and_forward_worked = #honor_final_doc > honor_baseline,
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

light:stop()
valor:stop()
honor:stop()
indras.assert.true_(not light:is_started(), "Light should be stopped")
indras.assert.true_(not valor:is_started(), "Valor should be stopped")
indras.assert.true_(not honor:is_started(), "Honor should be stopped")

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_abc_relay")

result:add_metrics({
    total_members = 3,
    honor_baseline_events = honor_baseline,
    honor_final_local_events = #honor_final_local,
    honor_final_doc_events = #honor_final_doc,
    light_events = #light_final,
    valor_events = #valor_final,
    store_and_forward_synced = honor_synced,
    messages_sent_while_offline = 2,
})

result:record_assertion("nodes_created", 3, 3, true)
result:record_assertion("interface_joined", 3, 3, true)
result:record_assertion("honor_went_offline", true, true, true)
result:record_assertion("messages_sent_offline", 2, 2, true)
result:record_assertion("honor_came_back", true, true, true)
result:record_assertion("crdt_sync", honor_synced, true, honor_synced)

local final_result = result:build()

logger.info("Live ABC Relay completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    store_and_forward = honor_synced,
    final_tick = tick,
})

-- Clean up Honor's explicit data dir
os.execute("rm -rf " .. honor_data_dir)

return final_result
