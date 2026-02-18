-- live_chat_store_forward.lua
--
-- Integration test: Chat messages sent while a peer is offline are delivered
-- when it reconnects, using the high-level Network/Realm API.
--
-- Tests store-and-forward at the chat level (realm:send / realm:all_messages)
-- rather than the low-level LiveNode API.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_chat_store_forward.lua --pretty

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Chat Store-and-Forward Test ===")
print()

-- ============================================================================
-- SETUP: 3 networks with explicit data directories for restart survival
-- ============================================================================

h.section(1, "Creating 3 networks with persistent storage")

local a_data_dir = os.tmpname() .. "_chat_a"
local b_data_dir = os.tmpname() .. "_chat_b"
local c_data_dir = os.tmpname() .. "_chat_c"
os.execute("mkdir -p " .. a_data_dir)
os.execute("mkdir -p " .. b_data_dir)
os.execute("mkdir -p " .. c_data_dir)

local a = indras.Network.new(a_data_dir)
local b = indras.Network.new(b_data_dir)
local c = indras.Network.new(c_data_dir)
a:start()
b:start()
c:start()
a:set_display_name("Alice")
b:set_display_name("Bob")
c:set_display_name("Carol")

-- Connect all pairs
a:connect_to(b)
a:connect_to(c)
b:connect_to(c)
print("    3 networks started and connected (persistent data dirs)")

-- Create realm and have all join
h.section(2, "A creates realm, B and C join")
local realm_a = a:create_realm("Store Forward Chat")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)

h.assert_eventually(function()
    return realm_a:member_count() >= 3
end, { timeout = 10, interval = 0.5, msg = "A should see 3 members" })
print("    Realm created, all 3 members joined")

-- ============================================================================
-- PHASE 1: Baseline - all online, each sends a message
-- ============================================================================

h.section(3, "Baseline - all online, each sends a message")
realm_a:send("Alice checking in")
realm_b:send("Bob checking in")
realm_c:send("Carol checking in")

-- Wait for all peers to see all 3 baseline messages
h.assert_eventually(function()
    return #realm_a:all_messages() >= 3
end, { timeout = 10, interval = 0.5, msg = "A should see 3 baseline messages" })

h.assert_eventually(function()
    return #realm_b:all_messages() >= 3
end, { timeout = 10, interval = 0.5, msg = "B should see 3 baseline messages" })

h.assert_eventually(function()
    return #realm_c:all_messages() >= 3
end, { timeout = 10, interval = 0.5, msg = "C should see 3 baseline messages" })

local baseline_count = #realm_a:all_messages()
print("    Baseline established: " .. baseline_count .. " messages visible to all")

-- ============================================================================
-- PHASE 2: C goes offline
-- ============================================================================

h.section(4, "C goes offline")
c:stop()
indras.assert.true_(not c:is_running(), "C should be stopped")

-- Drop reference and GC to release database lock
c = nil
realm_c = nil
collectgarbage("collect")
collectgarbage("collect")
print("    C stopped and reference released")

-- ============================================================================
-- PHASE 3: Messages while C is offline
-- ============================================================================

h.section(5, "Messages sent while C is offline")
realm_a:send("Message while C offline - from Alice 1")
realm_a:send("Message while C offline - from Alice 2")
realm_b:send("Message while C offline - from Bob")
print("    A sent 2 messages, B sent 1 message while C offline")

-- Verify A and B see the new messages
h.assert_eventually(function()
    return #realm_a:all_messages() >= baseline_count + 3
end, { timeout = 10, interval = 0.5, msg = "A should see 3 new messages" })

h.assert_eventually(function()
    return #realm_b:all_messages() >= baseline_count + 3
end, { timeout = 10, interval = 0.5, msg = "B should see 3 new messages" })

local ab_count = #realm_a:all_messages()
print("    A and B see " .. ab_count .. " messages total")

-- ============================================================================
-- PHASE 4: C restarts and reconnects
-- ============================================================================

h.section(6, "C restarts from persistent storage")
c = indras.Network.new(c_data_dir)
c:start()
c:set_display_name("Carol")
indras.assert.true_(c:is_running(), "C should be running after restart")

-- Rejoin realm to re-establish gossip subscriptions
realm_c = c:join(invite)

-- Wait for transport to fully initialize
indras.sleep(3)

-- Reconnect: C connects to A and B, and they connect back to C
c:connect_to(a)
c:connect_to(b)
a:connect_to(c)
b:connect_to(c)
print("    C restarted, rejoined realm, reconnected to A and B")

-- ============================================================================
-- PHASE 5: Verify store-and-forward delivery to C
-- ============================================================================

h.section(7, "Verifying C receives missed messages")

h.assert_eventually(function()
    local msgs = realm_c:all_messages()
    local found_a1, found_a2, found_b = false, false, false
    for _, m in ipairs(msgs) do
        if m.content == "Message while C offline - from Alice 1" then found_a1 = true end
        if m.content == "Message while C offline - from Alice 2" then found_a2 = true end
        if m.content == "Message while C offline - from Bob" then found_b = true end
    end
    return found_a1 and found_a2 and found_b
end, { timeout = 60, interval = 2, msg = "C should receive all 3 missed messages after reconnect" })

local c_count = #realm_c:all_messages()
print("    C sees " .. c_count .. " messages (including " .. (c_count - baseline_count) .. " missed)")
indras.assert.true_(c_count >= ab_count, "C should see at least as many messages as A and B")

-- ============================================================================
-- PHASE 6: C replies after recovery
-- ============================================================================

h.section(8, "C replies after recovery")

-- Find the sequence of one of A's missed messages
local a1_seq = nil
for _, m in ipairs(realm_c:all_messages()) do
    if m.content == "Message while C offline - from Alice 1" then
        a1_seq = m.id
        break
    end
end
indras.assert.true_(a1_seq ~= nil, "C should find Alice's missed message to reply to")

realm_c:reply(a1_seq, "Got your message Alice! Back online now.")
print("    C replied to A's missed message")

-- Verify A and B receive C's reply
h.assert_eventually(function()
    local msgs = realm_a:all_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Got your message Alice! Back online now." then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see C's reply" })

h.assert_eventually(function()
    local msgs = realm_b:all_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Got your message Alice! Back online now." then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "B should see C's reply" })
print("    A and B received C's post-recovery reply")

-- ============================================================================
-- PHASE 7: Both B and C go offline, A sends alone
-- ============================================================================

h.section(9, "B and C go offline, A sends alone")
b:stop()
c:stop()
indras.assert.true_(not b:is_running(), "B should be stopped")
indras.assert.true_(not c:is_running(), "C should be stopped")

b = nil
c = nil
realm_b = nil
realm_c = nil
collectgarbage("collect")
collectgarbage("collect")
print("    B and C stopped")

realm_a:send("Solo message from A - 1")
realm_a:send("Solo message from A - 2")
print("    A sent 2 messages while alone")

-- ============================================================================
-- PHASE 8: B and C restart, verify they receive A's solo messages
-- ============================================================================

h.section(10, "B and C restart")

-- Stagger restarts to avoid connection churn
b = indras.Network.new(b_data_dir)
b:start()
b:set_display_name("Bob")
realm_b = b:join(invite)
indras.sleep(2)
b:connect_to(a)
a:connect_to(b)
print("    B restarted and connected to A")

c = indras.Network.new(c_data_dir)
c:start()
c:set_display_name("Carol")
realm_c = c:join(invite)
indras.sleep(2)
c:connect_to(a)
c:connect_to(b)
a:connect_to(c)
b:connect_to(c)
print("    C restarted and connected to A and B")

-- ============================================================================
-- PHASE 9: Final verification - all peers see the same messages
-- ============================================================================

h.section(11, "Final verification")

h.assert_eventually(function()
    local msgs = realm_b:all_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Solo message from A - 2" then return true end
    end
    return false
end, { timeout = 60, interval = 2, msg = "B should receive A's solo messages" })

h.assert_eventually(function()
    local msgs = realm_c:all_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Solo message from A - 2" then return true end
    end
    return false
end, { timeout = 60, interval = 2, msg = "C should receive A's solo messages" })

local a_final = #realm_a:all_messages()
local b_final = #realm_b:all_messages()
local c_final = #realm_c:all_messages()
print("    Final message counts: A=" .. a_final .. " B=" .. b_final .. " C=" .. c_final)
indras.assert.eq(a_final, b_final, "A and B should have the same message count")
indras.assert.eq(a_final, c_final, "A and C should have the same message count")
print("    All peers have consistent message counts")

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

h.section(12, "Stopping networks and cleaning up")
a:stop()
b:stop()
c:stop()

-- Clean up data directories
os.execute("rm -rf " .. a_data_dir)
os.execute("rm -rf " .. b_data_dir)
os.execute("rm -rf " .. c_data_dir)
print("    All networks stopped, data dirs cleaned up")

h.pass("Live Chat Store-and-Forward Test")
