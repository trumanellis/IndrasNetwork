-- Live ABC Relay — store-and-forward over real P2P
--
-- Tests that messages sent while a peer is offline are delivered
-- when it comes back online, using real IndrasNode instances with
-- QUIC transport and CRDT sync.
--
-- Topology: Zephyr - Nova - Sage (all connected via shared interface)
-- Scenario: Sage goes offline, Zephyr sends a message, Sage comes
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

local zephyr = indras.LiveNode.new()
local nova   = indras.LiveNode.new()

-- Sage uses an explicit directory so files survive GC
-- (TempDir auto-deletes on drop, which kills keys & storage)
local sage_data_dir = os.tmpname() .. "_sage_node"
os.execute("mkdir -p " .. sage_data_dir)
local sage = indras.LiveNode.new(sage_data_dir)

zephyr:start()
nova:start()
sage:start()
indras.assert.true_(zephyr:is_started(), "Zephyr should be started")
indras.assert.true_(nova:is_started(), "Nova should be started")
indras.assert.true_(sage:is_started(), "Sage should be started")

local peer_zephyr = zephyr:identity()
local peer_nova   = nova:identity()
local peer_sage   = sage:identity()

advance()
logger.info("All nodes started", {
    tick = tick,
    zephyr_id = peer_zephyr,
    nova_id = peer_nova,
    sage_id = peer_sage,
})

-- ============================================================================
-- PHASE 2: CREATE SHARED INTERFACE — Zephyr creates, others join
-- ============================================================================

logger.info("Phase 2: Create shared interface", { phase = 2 })

local realm_id, invite = zephyr:create_interface("Relay Test")
local nova_realm = nova:join_interface(invite)
local sage_realm = sage:join_interface(invite)
indras.assert.eq(nova_realm, realm_id, "Nova's realm ID should match")
indras.assert.eq(sage_realm, realm_id, "Sage's realm ID should match")

advance()
logger.event("realm_created", {
    tick = tick,
    realm_id = realm_id,
    members = table.concat({peer_zephyr, peer_nova, peer_sage}, ","),
    member_count = 3,
})

for _, member in ipairs({peer_zephyr, peer_nova, peer_sage}) do
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
    member = peer_zephyr,
    alias = "Relay Test",
})

-- Verify all three see each other
local members_before = zephyr:members(realm_id)
logger.info("Interface created, all members joined", {
    tick = tick,
    realm_id = realm_id:sub(1, 16) .. "...",
    member_count = #members_before,
})

-- Send initial messages to confirm connectivity
zephyr:send_message(realm_id, "Zephyr here — all connected?")
nova:send_message(realm_id, "Nova checking in!")
sage:send_message(realm_id, "Sage online and ready.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_zephyr,
    realm_id = realm_id,
    content = "Zephyr here — all connected?",
    message_type = "text",
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_nova,
    realm_id = realm_id,
    content = "Nova checking in!",
    message_type = "text",
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_sage,
    realm_id = realm_id,
    content = "Sage online and ready.",
    message_type = "text",
})

-- Record baseline: how many events does Sage see right now?
local sage_events_before = sage:events_since(realm_id, 0)
local sage_baseline = #sage_events_before

logger.info("Baseline established", {
    tick = tick,
    sage_events = sage_baseline,
})

-- ============================================================================
-- PHASE 3: SAGE GOES OFFLINE
-- ============================================================================

indras.narrative("Sage drops off the network — but the others keep talking")
logger.info("Phase 3: Sage goes offline", { phase = 3 })

sage:stop()
indras.assert.true_(not sage:is_started(), "Sage should be stopped")

advance()
logger.event("member_left", {
    tick = tick,
    realm_id = realm_id,
    member = peer_sage,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_sage,
    realm_id = realm_id,
    content = "[Sage went offline]",
    message_type = "system",
})

logger.info("Sage is now offline", {
    tick = tick,
    sage_started = sage:is_started(),
})

-- ============================================================================
-- PHASE 4: SEND MESSAGES WHILE SAGE IS OFFLINE
-- ============================================================================

logger.info("Phase 4: Messages sent while Sage is offline", { phase = 4 })

-- Zephyr sends the key message
zephyr:send_message(realm_id, "Hello Sage! Hope you get this when you're back.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_zephyr,
    realm_id = realm_id,
    content = "Hello Sage! Hope you get this when you're back.",
    message_type = "text",
})

-- Nova sends too
nova:send_message(realm_id, "Miss you Sage! Saving these messages for you.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_nova,
    realm_id = realm_id,
    content = "Miss you Sage! Saving these messages for you.",
    message_type = "text",
})

-- Verify Zephyr and Nova can see the messages
local zephyr_events = zephyr:events_since(realm_id, 0)
local nova_events = nova:events_since(realm_id, 0)

indras.assert.true_(#zephyr_events > sage_baseline,
    "Zephyr should have more events than Sage's baseline")
indras.assert.true_(#nova_events > 0,
    "Nova should have events")

logger.info("Messages sent while Sage offline", {
    tick = tick,
    zephyr_events = #zephyr_events,
    nova_events = #nova_events,
    sage_offline = true,
})

-- ============================================================================
-- PHASE 5: SAGE COMES BACK ONLINE (new node, same data dir)
-- ============================================================================

indras.narrative("Sage returns — will the missed messages arrive?")
logger.info("Phase 5: Sage comes back online", { phase = 5 })

-- Drop the old node reference and force GC to release the database lock
sage = nil
collectgarbage("collect")
collectgarbage("collect")  -- double-collect to handle weak refs

-- Create a fresh node from the same data dir (keys + interfaces persist on disk)
sage = indras.LiveNode.new(sage_data_dir)
sage:start()
indras.assert.true_(sage:is_started(), "Sage should be started again")

-- Sage should have the same identity (loaded from keystore)
local peer_sage_2 = sage:identity()

-- Rejoin the interface to re-establish gossip subscriptions
sage:join_interface(invite)

-- Wait for Sage's transport to fully initialize (relay connection setup)
indras.sleep(3)

-- Close stale connections on Zephyr and Nova (they still think old Sage is alive)
-- Without this, connect() reuses the dead QUIC connection
local sage_full = sage:identity_full()
zephyr:disconnect_from(sage_full)
nova:disconnect_from(sage_full)

-- Establish fresh connections: Sage connects TO the established nodes
-- (Sage knows their addresses; they've been running the whole time)
sage:connect_to(zephyr)
sage:connect_to(nova)

advance()
logger.event("member_joined", {
    tick = tick,
    realm_id = realm_id,
    member = peer_sage_2,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_sage_2,
    realm_id = realm_id,
    content = "[Sage came back online]",
    message_type = "system",
})

logger.info("Sage is back online", {
    tick = tick,
    sage_started = sage:is_started(),
    sage_identity = peer_sage_2,
    same_identity = (peer_sage == peer_sage_2),
})

-- ============================================================================
-- PHASE 6: CHECK SYNC — Did Sage receive missed messages?
-- ============================================================================

logger.info("Phase 6: Checking CRDT sync (polling for sync...)", { phase = 6 })

-- Poll for sync completion instead of fixed sleep.
-- Sync task runs every 5s; we check every 2s for up to 30s.
local sage_doc = {}
local sage_synced = false
local max_polls = 15
for poll = 1, max_polls do
    indras.sleep(2)
    sage_doc = sage:document_events(realm_id)
    sage_synced = #sage_doc > sage_baseline
    logger.info("Sync poll", {
        tick = tick,
        poll = poll,
        sage_doc_events = #sage_doc,
        sage_baseline = sage_baseline,
        sage_synced = sage_synced,
    })
    if sage_synced then
        break
    end
end

-- Check Sage's local events (what it persisted before going offline)
local sage_local = sage:events_since(realm_id, 0)

-- Also check what Zephyr and Nova see now
local zephyr_final = zephyr:events_since(realm_id, 0)
local nova_final = nova:events_since(realm_id, 0)

advance()
logger.info("Sync results", {
    tick = tick,
    sage_local_events = #sage_local,
    sage_doc_events = #sage_doc,
    sage_baseline = sage_baseline,
    sage_synced = sage_synced,
    zephyr_events = #zephyr_final,
    nova_events = #nova_final,
})

-- Log individual events Sage sees
for i, e in ipairs(sage_doc) do
    logger.info("Sage doc event", {
        tick = tick,
        index = i,
        sender = e.sender,
        content = e.content,
        sequence = e.sequence,
    })
end

-- Sage sends a confirmation message
sage:send_message(realm_id, "I'm back! Checking if I got your messages...")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_sage_2,
    realm_id = realm_id,
    content = "I'm back! Checking if I got your messages...",
    message_type = "text",
})

-- ============================================================================
-- PHASE 7: FINAL VERIFICATION
-- ============================================================================

logger.info("Phase 7: Final verification", { phase = 7 })

-- Re-check after Sage has had a chance to sync
local sage_final_local = sage:events_since(realm_id, 0)
local sage_final_doc = sage:document_events(realm_id)

advance()
logger.info("Final state", {
    tick = tick,
    sage_local_count = #sage_final_local,
    sage_doc_count = #sage_final_doc,
    zephyr_count = #zephyr_final,
    nova_count = #nova_final,
    store_and_forward_worked = #sage_final_doc > sage_baseline,
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

zephyr:stop()
nova:stop()
sage:stop()
indras.assert.true_(not zephyr:is_started(), "Zephyr should be stopped")
indras.assert.true_(not nova:is_started(), "Nova should be stopped")
indras.assert.true_(not sage:is_started(), "Sage should be stopped")

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_abc_relay")

result:add_metrics({
    total_members = 3,
    sage_baseline_events = sage_baseline,
    sage_final_local_events = #sage_final_local,
    sage_final_doc_events = #sage_final_doc,
    zephyr_events = #zephyr_final,
    nova_events = #nova_final,
    store_and_forward_synced = sage_synced,
    messages_sent_while_offline = 2,
})

result:record_assertion("nodes_created", 3, 3, true)
result:record_assertion("interface_joined", 3, 3, true)
result:record_assertion("sage_went_offline", true, true, true)
result:record_assertion("messages_sent_offline", 2, 2, true)
result:record_assertion("sage_came_back", true, true, true)
result:record_assertion("crdt_sync", sage_synced, true, sage_synced)

local final_result = result:build()

logger.info("Live ABC Relay completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    store_and_forward = sage_synced,
    final_tick = tick,
})

-- Clean up Sage's explicit data dir
os.execute("rm -rf " .. sage_data_dir)

return final_result
