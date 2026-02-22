-- live_chat_push_delivery.lua
--
-- Integration test: chat messages arrive via push notification (broadcast),
-- NOT polling. Verifies that Document instance sharing works correctly so
-- that mutations on one handle notify listeners on the same cached instance.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_chat_push_delivery.lua --pretty

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Chat Push Delivery Test ===")
print()

-- ============================================================================
-- SETUP: 2 networks (A, B), connected, shared realm
-- ============================================================================

h.section(1, "Creating and connecting 2 networks")
local nets = h.create_networks(2)
local a, b = nets[1], nets[2]
a:set_display_name("A")
b:set_display_name("B")
h.connect_all(nets)
print("    2 networks started and connected")

h.section(2, "A creates realm, B joins")
local realm_a = a:create_realm("Push Test Room")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)

h.assert_eventually(function()
    return realm_a:member_count() >= 2
end, { timeout = 10, interval = 0.5, msg = "A should see 2 members" })
print("    Realm created, both members joined")

-- ============================================================================
-- PHASE 1: Subscribe BEFORE sending — A listens for push from B
-- ============================================================================

h.section(3, "A subscribes to chat doc changes BEFORE B sends")
local doc_a = realm_a:chat_doc()
local sub_a = doc_a:subscribe()
print("    A subscribed to chat doc changes")

h.section(4, "B sends a message")
local msg_id_1 = realm_b:chat_send("B", "Hello from B!")
print("    B sent message: " .. msg_id_1)

h.section(5, "Assert A receives push notification (no polling)")
local got_push = sub_a:wait(10.0)
indras.assert.true_(got_push, "A should receive push notification when B sends a message")
print("    A received push notification for B's message")

-- Verify content
h.assert_eventually(function()
    local msgs = doc_a:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Hello from B!" then return true end
    end
    return false
end, { timeout = 5, interval = 0.5, msg = "A should see B's message content" })
print("    A sees correct message content")

-- ============================================================================
-- PHASE 2: Reaction push — B reacts, A gets notification
-- ============================================================================

h.section(6, "Test reaction push delivery")

-- A sends a message for B to react to
local msg_id_2 = realm_a:chat_send("A", "React to this!")
print("    A sent message: " .. msg_id_2)
indras.sleep(1.0)

-- Fresh subscription for reaction test
local sub_a2 = doc_a:subscribe()
print("    A re-subscribed for reaction test")

-- B reacts to A's message
local doc_b = realm_b:chat_doc()
h.assert_eventually(function()
    local msgs = doc_b:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "React to this!" then return true end
    end
    return false
end, { timeout = 5, interval = 0.5, msg = "B should see A's message before reacting" })

realm_b:chat_react("B", msg_id_2, "thumbsup")
print("    B reacted to A's message")

local got_react_push = sub_a2:wait(10.0)
indras.assert.true_(got_react_push, "A should receive push notification when B reacts")
print("    A received push notification for B's reaction")

-- ============================================================================
-- PHASE 3: Bidirectional — A sends, B gets notification
-- ============================================================================

h.section(7, "Bidirectional: A sends, B gets push notification")
local sub_b = doc_b:subscribe()
print("    B subscribed to chat doc changes")

realm_a:chat_send("A", "Message from A to B")
print("    A sent message")

local got_b_push = sub_b:wait(10.0)
indras.assert.true_(got_b_push, "B should receive push notification when A sends a message")
print("    B received push notification for A's message")

h.assert_eventually(function()
    local msgs = doc_b:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Message from A to B" then return true end
    end
    return false
end, { timeout = 5, interval = 0.5, msg = "B should see A's message content" })
print("    B sees correct message content")

-- ============================================================================
-- PHASE 4: Reply push — B replies, A gets notification
-- ============================================================================

h.section(8, "Reply push delivery")
local sub_a3 = doc_a:subscribe()

realm_b:chat_reply("B", msg_id_2, "Here is my reply!")
print("    B replied to A's message")

local got_reply_push = sub_a3:wait(10.0)
indras.assert.true_(got_reply_push, "A should receive push notification when B replies")
print("    A received push notification for B's reply")

h.assert_eventually(function()
    local msgs = doc_a:visible_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Here is my reply!" and m.reply_to == msg_id_2 then
            return true
        end
    end
    return false
end, { timeout = 5, interval = 0.5, msg = "A should see B's reply with correct parent" })
print("    A sees reply with correct parent reference")

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

h.section(9, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

h.pass("Live Chat Push Delivery Test")
