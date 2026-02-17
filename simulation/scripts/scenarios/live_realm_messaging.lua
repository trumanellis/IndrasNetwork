-- live_realm_messaging.lua
--
-- Integration test: Realm creation, joining, and messaging across 3 nodes.
-- Verifies invites, text messages, replies, reactions, search, and member list.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_realm_messaging.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Realm Messaging Test ===")
print()

-- Step 1: Create 3 networks (A, B, C), start, connect_all
h.section(1, "Creating and connecting 3 networks")
local nets = h.create_networks(3)
local a = nets[1]
local b = nets[2]
local c = nets[3]
a:set_display_name("Alice")
b:set_display_name("Bob")
c:set_display_name("Carol")
h.connect_all(nets)
print("    3 networks started and connected")

-- Step 2: A creates realm "Chat Room"
h.section(2, "A creates realm 'Chat Room'")
local realm_a = a:create_realm("Chat Room")
print("    Realm created: " .. realm_a:name())
indras.assert.eq(realm_a:name(), "Chat Room", "Realm name should be 'Chat Room'")

-- Step 3: Get invite code from realm
h.section(3, "Getting invite code")
local invite = realm_a:invite_code()
print("    Invite code length: " .. #invite .. " chars")
indras.assert.true_(#invite > 0, "Invite code should be non-empty")

-- Step 4: B and C join via invite
h.section(4, "B and C join via invite")
local realm_b = b:join(invite)
local realm_c = c:join(invite)
print("    B joined realm: " .. realm_b:id():sub(1, 16) .. "...")
print("    C joined realm: " .. realm_c:id():sub(1, 16) .. "...")
indras.assert.eq(realm_b:id(), realm_a:id(), "B's realm ID should match A's")
indras.assert.eq(realm_c:id(), realm_a:id(), "C's realm ID should match A's")

-- Step 5: Wait for B and C membership to propagate to A
h.section(5, "Waiting for membership sync")
h.assert_eventually(function()
    return realm_a:member_count() >= 3
end, { timeout = 10, interval = 0.5, msg = "A should see at least 3 members after B and C join" })
print("    Membership sync complete")

-- Step 6: A sends "Hello everyone!"
h.section(6, "A sends messages")
realm_a:send("Hello everyone!")
print("    A sent: 'Hello everyone!'")

-- Step 7: A sends "How are you?"
realm_a:send("How are you?")
print("    A sent: 'How are you?'")

-- Step 8: B replies to first message: "Hi Alice!" — B needs to see A's messages first
h.section(7, "B replies and C reacts")
h.assert_eventually(function()
    return #realm_b:all_messages() >= 1
end, { timeout = 10, interval = 0.5, msg = "B should see A's messages before replying" })
realm_b:reply(1, "Hi Alice!")
print("    B replied to msg 1: 'Hi Alice!'")

-- Step 9: C reacts to first message with "👍" — C needs to see A's messages first
h.assert_eventually(function()
    return #realm_c:all_messages() >= 1
end, { timeout = 10, interval = 0.5, msg = "C should see A's messages before reacting" })
realm_c:react(1, "👍")
print("    C reacted to msg 1 with 👍")

-- Step 10: Wait for message sync across nodes
h.section(8, "Waiting for message sync")
-- A should see its own 2 messages locally (instant)
local a_msgs_local = realm_a:all_messages()
indras.assert.true_(#a_msgs_local >= 2, "A should see at least 2 own messages")
print("    A sees " .. #a_msgs_local .. " message(s) locally")

-- B should eventually see A's messages + its own reply
h.assert_eventually(function()
    return #realm_b:all_messages() >= 3
end, { timeout = 10, interval = 0.5, msg = "B should see at least 3 messages" })

-- C should eventually see A's messages
h.assert_eventually(function()
    return #realm_c:all_messages() >= 2
end, { timeout = 10, interval = 0.5, msg = "C should see at least 2 messages" })
print("    Cross-node message sync complete")

-- Step 11: Each node reads all_messages() and verifies count >= expected
h.section(9, "Verifying message counts")
local a_msgs = realm_a:all_messages()
local b_msgs = realm_b:all_messages()
local c_msgs = realm_c:all_messages()
print("    A sees " .. #a_msgs .. " message(s)")
print("    B sees " .. #b_msgs .. " message(s)")
print("    C sees " .. #c_msgs .. " message(s)")
-- A should see its own 2 messages plus reply from B and reaction from C (eventually)
indras.assert.true_(#a_msgs >= 2, "A should see at least 2 messages (own sends)")
indras.assert.true_(#b_msgs >= 3, "B should see at least 3 messages (A's 2 + B's reply)")
indras.assert.true_(#c_msgs >= 2, "C should see at least 2 messages (A's sends)")
print("    Message counts verified")

-- Step 12: A searches for "Hello" - verify results
h.section(10, "A searches for 'Hello'")
local results = realm_a:search_messages("Hello")
print("    Search results for 'Hello': " .. #results .. " match(es)")
indras.assert.true_(#results >= 1, "Search for 'Hello' should return at least 1 result")
local found = false
for _, msg in ipairs(results) do
    if string.find(msg.content, "Hello", 1, true) then
        found = true
        break
    end
end
indras.assert.true_(found, "At least one search result should contain 'Hello'")
print("    Search results verified")

-- Step 13: Check member_list() from A's perspective - should have 3 members
h.section(11, "Verifying member list")
local members = realm_a:member_list()
print("    A sees " .. #members .. " member(s) in realm")
for i, m in ipairs(members) do
    print("      [" .. i .. "] " .. tostring(m))
end
indras.assert.true_(realm_a:member_count() >= 3, "Realm should have at least 3 members")
print("    Member list verified")

-- Step 14: Stop all
h.section(12, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Realm Messaging Test")
