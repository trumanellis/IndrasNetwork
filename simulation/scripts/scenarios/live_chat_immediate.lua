-- live_chat_immediate.lua
--
-- Integration test: Immediate delivery of chat messages when all peers are online.
-- Exercises the high-level Network/Realm chat API (send, reply, react,
-- all_messages, unread_count, mark_read) with 3 connected peers.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_chat_immediate.lua --pretty

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Chat Immediate Delivery Test ===")
print()

-- ============================================================================
-- SETUP: 3 networks (Alice, Bob, Carol), all connected, shared realm
-- ============================================================================

h.section(1, "Creating and connecting 3 networks")
local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
a:set_display_name("Alice")
b:set_display_name("Bob")
c:set_display_name("Carol")
h.connect_all(nets)
print("    3 networks started and connected")

-- Capture member IDs for unread tracking
local a_id = a:id()
local b_id = b:id()
local c_id = c:id()

h.section(2, "A creates realm, B and C join")
local realm_a = a:create_realm("Chat Room")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)

-- Wait for membership to propagate
h.assert_eventually(function()
    return realm_a:member_count() >= 3
end, { timeout = 10, interval = 0.5, msg = "A should see 3 members" })
print("    Realm created, all 3 members joined")

-- ============================================================================
-- PHASE 1: Basic text messages
-- ============================================================================

h.section(3, "Basic text messages - A sends 'Hello!'")
local seq1 = realm_a:send("Hello!")
print("    A sent 'Hello!' (seq=" .. seq1 .. ")")

-- B and C should see the message
h.assert_eventually(function()
    return #realm_b:all_messages() >= 1
end, { timeout = 10, interval = 0.5, msg = "B should see A's message" })

h.assert_eventually(function()
    return #realm_c:all_messages() >= 1
end, { timeout = 10, interval = 0.5, msg = "C should see A's message" })
print("    B and C received A's message")

-- ============================================================================
-- PHASE 2: Reply threading
-- ============================================================================

h.section(4, "Reply threading - B replies to A's message")
local reply_seq = realm_b:reply(seq1, "Hey Alice!")
print("    B replied to msg " .. seq1 .. " with 'Hey Alice!' (seq=" .. reply_seq .. ")")

-- A and C should see the reply
h.assert_eventually(function()
    local msgs = realm_a:all_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Hey Alice!" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see B's reply" })

h.assert_eventually(function()
    local msgs = realm_c:all_messages()
    for _, m in ipairs(msgs) do
        if m.content == "Hey Alice!" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "C should see B's reply" })
print("    Reply propagated to A and C")

-- ============================================================================
-- PHASE 3: Reactions
-- ============================================================================

h.section(5, "Reactions - C reacts to A's message")
local react_seq = realm_c:react(seq1, "thumbsup")
print("    C reacted to msg " .. seq1 .. " with thumbsup (seq=" .. react_seq .. ")")

-- A should see the reaction
h.assert_eventually(function()
    local msgs = realm_a:all_messages()
    for _, m in ipairs(msgs) do
        if m.type == "reaction" and m.content == "thumbsup" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see C's reaction" })
print("    Reaction propagated to A")

-- ============================================================================
-- PHASE 4: Rapid-fire burst
-- ============================================================================

h.section(6, "Rapid-fire burst - A sends 5 messages quickly")
local burst_seqs = {}
for i = 1, 5 do
    local s = realm_a:send("Burst " .. i)
    burst_seqs[i] = s
end
print("    A sent 5 burst messages")

-- B should receive all 5
h.assert_eventually(function()
    local msgs = realm_b:all_messages()
    local burst_count = 0
    for _, m in ipairs(msgs) do
        if m.type == "text" and string.find(m.content, "^Burst %d$") then
            burst_count = burst_count + 1
        end
    end
    return burst_count >= 5
end, { timeout = 15, interval = 0.5, msg = "B should see all 5 burst messages" })

-- C should receive all 5
h.assert_eventually(function()
    local msgs = realm_c:all_messages()
    local burst_count = 0
    for _, m in ipairs(msgs) do
        if m.type == "text" and string.find(m.content, "^Burst %d$") then
            burst_count = burst_count + 1
        end
    end
    return burst_count >= 5
end, { timeout = 15, interval = 0.5, msg = "C should see all 5 burst messages" })
print("    All 5 burst messages received by B and C")

-- ============================================================================
-- PHASE 5: Multi-sender conversation
-- ============================================================================

h.section(7, "Multi-sender conversation")
realm_a:send("Message from Alice")
realm_b:send("Message from Bob")
realm_c:send("Message from Carol")
print("    A, B, C each sent a message")

-- All 3 should see all 3 messages
local function has_all_three(msgs)
    local found_a, found_b, found_c = false, false, false
    for _, m in ipairs(msgs) do
        if m.content == "Message from Alice" then found_a = true end
        if m.content == "Message from Bob" then found_b = true end
        if m.content == "Message from Carol" then found_c = true end
    end
    return found_a and found_b and found_c
end

h.assert_eventually(function()
    return has_all_three(realm_a:all_messages())
end, { timeout = 10, interval = 0.5, msg = "A should see all 3 conversation messages" })

h.assert_eventually(function()
    return has_all_three(realm_b:all_messages())
end, { timeout = 10, interval = 0.5, msg = "B should see all 3 conversation messages" })

h.assert_eventually(function()
    return has_all_three(realm_c:all_messages())
end, { timeout = 10, interval = 0.5, msg = "C should see all 3 conversation messages" })
print("    All peers see all 3 conversation messages")

-- ============================================================================
-- PHASE 6: Message content verification
-- ============================================================================

h.section(8, "Message content verification")
local a_msgs = realm_a:all_messages()
print("    A sees " .. #a_msgs .. " total messages")

-- Verify first text message fields
local found_hello = false
for _, m in ipairs(a_msgs) do
    if m.content == "Hello!" then
        indras.assert.eq(m.type, "text", "Hello message should be type 'text'")
        indras.assert.eq(m.sender_id, a_id, "Hello sender_id should be A's ID")
        found_hello = true
        break
    end
end
indras.assert.true_(found_hello, "A should see its own 'Hello!' message")

-- Verify reaction fields
local found_reaction = false
for _, m in ipairs(a_msgs) do
    if m.type == "reaction" and m.content == "thumbsup" then
        indras.assert.eq(m.sender_id, c_id, "Reaction sender_id should be C's ID")
        found_reaction = true
        break
    end
end
indras.assert.true_(found_reaction, "A should see C's reaction")
print("    Message fields verified (type, content, sender_id)")

-- ============================================================================
-- PHASE 7: Unread tracking
-- ============================================================================

h.section(9, "Unread tracking")

-- B's unread count should be > 0 (B hasn't marked read yet)
local b_unread = realm_b:unread_count(b_id)
print("    B unread count: " .. b_unread)
indras.assert.true_(b_unread > 0, "B should have unread messages")

-- B marks read
realm_b:mark_read(b_id)
print("    B marked realm as read")

-- B's unread should now be 0
local b_unread_after = realm_b:unread_count(b_id)
print("    B unread after mark_read: " .. b_unread_after)
indras.assert.eq(b_unread_after, 0, "B should have 0 unread after mark_read")

-- A sends another message — B should get 1 unread
realm_a:send("One more!")
h.assert_eventually(function()
    return realm_b:unread_count(b_id) > 0
end, { timeout = 10, interval = 0.5, msg = "B should have unread after new message from A" })
print("    B has unread after new message from A")

-- ============================================================================
-- SHUTDOWN
-- ============================================================================

h.section(10, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

h.pass("Live Chat Immediate Delivery Test")
