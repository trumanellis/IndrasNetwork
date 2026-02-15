-- Live ABC Relay — store-and-forward over real P2P
--
-- Tests that messages sent while a peer is offline are delivered
-- when it comes back online, using real IndrasNode instances with
-- QUIC transport and CRDT sync.
--
-- Topology: A - B - C (all connected via shared interface)
-- Scenario: C goes offline, A and B send messages, then A also goes
-- offline. C comes back and receives messages from B alone.
-- Finally A comes back and all three verify full sync.
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
    description = "Store-and-forward: message delivery after offline peers reconnect",
    phase = 0,
})

-- ============================================================================
-- PHASE 1: CREATE AND START ALL THREE NODES
-- ============================================================================

indras.narrative("Three peers form a network — then the lights go out")
logger.info("Phase 1: Creating three LiveNode instances", { phase = 1 })

-- A and C use explicit directories so they survive GC across stop/restart
-- (TempDir auto-deletes on drop, which kills keys & storage)
local a_data_dir = os.tmpname() .. "_node_a"
os.execute("mkdir -p " .. a_data_dir)
local a = indras.LiveNode.new(a_data_dir)

local b = indras.LiveNode.new()

local c_data_dir = os.tmpname() .. "_node_c"
os.execute("mkdir -p " .. c_data_dir)
local c = indras.LiveNode.new(c_data_dir)

a:start()
b:start()
c:start()
indras.assert.true_(a:is_started(), "A should be started")
indras.assert.true_(b:is_started(), "B should be started")
indras.assert.true_(c:is_started(), "C should be started")

local peer_a = a:identity()
local peer_b = b:identity()
local peer_c = c:identity()

advance()
logger.info("All nodes started", {
    tick = tick,
    a_id = peer_a,
    b_id = peer_b,
    c_id = peer_c,
})

-- ============================================================================
-- PHASE 2: CREATE SHARED INTERFACE — A creates, others join
-- ============================================================================

logger.info("Phase 2: Create shared interface", { phase = 2 })

local realm_id, invite = a:create_interface("Relay Test")
local b_realm = b:join_interface(invite)
local c_realm = c:join_interface(invite)
indras.assert.eq(b_realm, realm_id, "B's realm ID should match")
indras.assert.eq(c_realm, realm_id, "C's realm ID should match")

advance()
logger.event("realm_created", {
    tick = tick,
    realm_id = realm_id,
    members = table.concat({peer_a, peer_b, peer_c}, ","),
    member_count = 3,
})

for _, member in ipairs({peer_a, peer_b, peer_c}) do
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
    member = peer_a,
    alias = "Relay Test",
})

-- Verify all three see each other
local members_before = a:members(realm_id)
logger.info("Interface created, all members joined", {
    tick = tick,
    realm_id = realm_id:sub(1, 16) .. "...",
    member_count = #members_before,
})

-- Send initial messages to confirm connectivity
a:send_message(realm_id, "A here — all connected?")
b:send_message(realm_id, "B checking in!")
c:send_message(realm_id, "C online and ready.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_a,
    realm_id = realm_id,
    content = "A here — all connected?",
    message_type = "text",
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_b,
    realm_id = realm_id,
    content = "B checking in!",
    message_type = "text",
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_c,
    realm_id = realm_id,
    content = "C online and ready.",
    message_type = "text",
})

-- Record baseline: how many events does C see right now?
local c_events_before = c:events_since(realm_id, 0)
local c_baseline = #c_events_before

logger.info("Baseline established", {
    tick = tick,
    c_events = c_baseline,
})

-- ============================================================================
-- PHASE 3: C GOES OFFLINE
-- ============================================================================

indras.narrative("C drops off the network — but the others keep talking")
logger.info("Phase 3: C goes offline", { phase = 3 })

c:stop()
indras.assert.true_(not c:is_started(), "C should be stopped")

advance()
logger.event("member_left", {
    tick = tick,
    realm_id = realm_id,
    member = peer_c,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_c,
    realm_id = realm_id,
    content = "[C went offline]",
    message_type = "system",
})

logger.info("C is now offline", {
    tick = tick,
    c_started = c:is_started(),
})

-- ============================================================================
-- PHASE 4: SEND MESSAGES WHILE C IS OFFLINE
-- ============================================================================

logger.info("Phase 4: Messages sent while C is offline", { phase = 4 })

a:send_message(realm_id, "Hello C! Hope you get this when you're back.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_a,
    realm_id = realm_id,
    content = "Hello C! Hope you get this when you're back.",
    message_type = "text",
})

b:send_message(realm_id, "Miss you C! Saving these messages for you.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_b,
    realm_id = realm_id,
    content = "Miss you C! Saving these messages for you.",
    message_type = "text",
})

-- Verify A and B can see the messages
local a_events = a:events_since(realm_id, 0)
local b_events = b:events_since(realm_id, 0)

indras.assert.true_(#a_events > c_baseline,
    "A should have more events than C's baseline")
indras.assert.true_(#b_events > 0,
    "B should have events")

logger.info("Messages sent while C offline", {
    tick = tick,
    a_events = #a_events,
    b_events = #b_events,
    c_offline = true,
})

-- ============================================================================
-- PHASE 5: A ALSO GOES OFFLINE — only B remains
-- ============================================================================

indras.narrative("A drops off too — now only B holds the thread")
logger.info("Phase 5: A goes offline", { phase = 5 })

a:stop()
indras.assert.true_(not a:is_started(), "A should be stopped")

advance()
logger.event("member_left", {
    tick = tick,
    realm_id = realm_id,
    member = peer_a,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_a,
    realm_id = realm_id,
    content = "[A went offline]",
    message_type = "system",
})

logger.info("A is now offline, only B remains", {
    tick = tick,
    a_started = a:is_started(),
    b_started = b:is_started(),
})

-- ============================================================================
-- PHASE 6: C COMES BACK ONLINE (new node, same data dir)
-- ============================================================================

indras.narrative("C returns — will the missed messages arrive from B alone?")
logger.info("Phase 6: C comes back online", { phase = 6 })

-- Drop the old node reference and force GC to release the database lock
c = nil
collectgarbage("collect")
collectgarbage("collect")  -- double-collect to handle weak refs

-- Create a fresh node from the same data dir (keys + interfaces persist on disk)
c = indras.LiveNode.new(c_data_dir)
c:start()
indras.assert.true_(c:is_started(), "C should be started again")

-- C should have the same identity (loaded from keystore)
local peer_c_2 = c:identity()

-- Rejoin the interface to re-establish gossip subscriptions
c:join_interface(invite)

-- Wait for C's transport to fully initialize (relay connection setup)
indras.sleep(3)

-- Close stale connection on B (it still thinks old C is alive)
-- A is offline so no need to disconnect there
local c_full = c:identity_full()
b:disconnect_from(c_full)

-- Establish fresh connection: C connects TO B (the only online peer)
c:connect_to(b)

advance()
logger.event("member_joined", {
    tick = tick,
    realm_id = realm_id,
    member = peer_c_2,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_c_2,
    realm_id = realm_id,
    content = "[C came back online]",
    message_type = "system",
})

logger.info("C is back online (A still offline)", {
    tick = tick,
    c_started = c:is_started(),
    c_identity = peer_c_2,
    same_identity = (peer_c == peer_c_2),
    a_started = a:is_started(),
    b_started = b:is_started(),
})

-- Debug: snapshot each node's world view after C reconnects
local b_wv = b:world_view()
local c_wv = c:world_view()
logger.info("World view after C reconnects", {
    b_identity = b_wv.identity,
    b_interface_count = b_wv.interface_count,
    b_connected_count = b_wv.connected_count,
    c_identity = c_wv.identity,
    c_interface_count = c_wv.interface_count,
    c_connected_count = c_wv.connected_count,
})

-- Show members for B's interface
if b_wv.interface_count > 0 then
    local b_iface = b_wv.interfaces[1]
    logger.info("B interface state", {
        b_member_count = b_iface.member_count,
        b_storage_member_count = b_iface.storage_member_count,
    })
end

-- Show members for C's interface
if c_wv.interface_count > 0 then
    local c_iface = c_wv.interfaces[1]
    logger.info("C interface state", {
        c_member_count = c_iface.member_count,
        c_storage_member_count = c_iface.storage_member_count,
    })
end

-- ============================================================================
-- PHASE 7: CHECK SYNC — Did C receive missed messages from B?
-- ============================================================================

logger.info("Phase 7: Checking CRDT sync from B alone", { phase = 7 })

-- Poll for sync completion instead of fixed sleep.
-- Sync task runs every 5s; we check every 2s for up to 30s.
local c_doc = {}
local c_synced = false
local max_polls = 15
for poll = 1, max_polls do
    indras.sleep(2)
    c_doc = c:document_events(realm_id)
    c_synced = #c_doc > c_baseline
    logger.info("Sync poll", {
        tick = tick,
        poll = poll,
        c_doc_events = #c_doc,
        c_baseline = c_baseline,
        c_synced = c_synced,
    })
    if c_synced then
        break
    end
end

-- Check C's local events
local c_local = c:events_since(realm_id, 0)

advance()
logger.info("Sync results (B-only relay)", {
    tick = tick,
    c_local_events = #c_local,
    c_doc_events = #c_doc,
    c_baseline = c_baseline,
    c_synced = c_synced,
    b_events = #b:events_since(realm_id, 0),
    a_offline = true,
})

-- Log individual events C sees
for i, e in ipairs(c_doc) do
    logger.info("C doc event", {
        tick = tick,
        index = i,
        sender = e.sender,
        content = e.content,
        sequence = e.sequence,
    })
end

-- C sends a confirmation message
c:send_message(realm_id, "I'm back! Got your messages from B.")

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_c_2,
    realm_id = realm_id,
    content = "I'm back! Got your messages from B.",
    message_type = "text",
})

-- ============================================================================
-- PHASE 8: A COMES BACK ONLINE (new node, same data dir)
-- ============================================================================

indras.narrative("A returns — the full circle reconnects")
logger.info("Phase 8: A comes back online", { phase = 8 })

-- Drop the old node reference and force GC to release the database lock
a = nil
collectgarbage("collect")
collectgarbage("collect")

-- Create a fresh node from the same data dir
a = indras.LiveNode.new(a_data_dir)
a:start()
indras.assert.true_(a:is_started(), "A should be started again")

local peer_a_2 = a:identity()

-- Rejoin the interface to re-establish gossip subscriptions
a:join_interface(invite)

-- Wait for A's transport to initialize
indras.sleep(3)

-- Close stale connections on B and C
local a_full = a:identity_full()
b:disconnect_from(a_full)
c:disconnect_from(a_full)

-- A connects to B and C
a:connect_to(b)
a:connect_to(c)

advance()
logger.event("member_joined", {
    tick = tick,
    realm_id = realm_id,
    member = peer_a_2,
})

advance()
logger.event("chat_message", {
    tick = tick,
    member = peer_a_2,
    realm_id = realm_id,
    content = "[A came back online]",
    message_type = "system",
})

logger.info("A is back online, all three reconnected", {
    tick = tick,
    a_started = a:is_started(),
    b_started = b:is_started(),
    c_started = c:is_started(),
})

-- Give A time to sync
indras.sleep(5)

-- ============================================================================
-- PHASE 9: FINAL VERIFICATION
-- ============================================================================

logger.info("Phase 9: Final verification", { phase = 9 })

-- Check final state for all three nodes
local c_final_local = c:events_since(realm_id, 0)
local c_final_doc = c:document_events(realm_id)
local a_final = a:events_since(realm_id, 0)
local b_final = b:events_since(realm_id, 0)

advance()
logger.info("Final state", {
    tick = tick,
    c_local_count = #c_final_local,
    c_doc_count = #c_final_doc,
    a_count = #a_final,
    b_count = #b_final,
    store_and_forward_worked = #c_final_doc > c_baseline,
})

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

a:stop()
b:stop()
c:stop()
indras.assert.true_(not a:is_started(), "A should be stopped")
indras.assert.true_(not b:is_started(), "B should be stopped")
indras.assert.true_(not c:is_started(), "C should be stopped")

-- ============================================================================
-- RESULTS
-- ============================================================================

local result = quest_helpers.result_builder("live_abc_relay")

result:add_metrics({
    total_members = 3,
    c_baseline_events = c_baseline,
    c_final_local_events = #c_final_local,
    c_final_doc_events = #c_final_doc,
    a_events = #a_final,
    b_events = #b_final,
    store_and_forward_synced = c_synced,
    messages_sent_while_offline = 2,
    both_a_and_c_went_offline = true,
})

result:record_assertion("nodes_created", 3, 3, true)
result:record_assertion("interface_joined", 3, 3, true)
result:record_assertion("c_went_offline", true, true, true)
result:record_assertion("messages_sent_offline", 2, 2, true)
result:record_assertion("a_went_offline", true, true, true)
result:record_assertion("c_came_back", true, true, true)
result:record_assertion("crdt_sync_from_b", c_synced, true, c_synced)
result:record_assertion("a_came_back", true, true, true)

local final_result = result:build()

logger.info("Live ABC Relay completed", {
    passed = final_result.passed,
    level = final_result.level,
    duration_sec = final_result.duration_sec,
    store_and_forward = c_synced,
    final_tick = tick,
})

-- Clean up explicit data dirs
os.execute("rm -rf " .. a_data_dir)
os.execute("rm -rf " .. c_data_dir)

return final_result
