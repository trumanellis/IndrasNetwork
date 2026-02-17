-- live_offline_recovery.lua
--
-- Integration test: CRDT-based message delivery and document sync.
-- Verifies that messages and documents are delivered to already-joined peers
-- via Automerge CRDT sync.
--
-- Tests:
--  1. Delayed message delivery: A and B join, A sends messages, B sees them all
--  2. Document recovery via refresh: A writes a doc, B reads it after a delay
--  3. Three-node message propagation: A, B, C all join; messages flow to all
--
-- Note: Pre-join messages (sent before a peer joins) are NOT available to the
-- joining peer. This test only uses post-join messages which are reliably synced.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_offline_recovery.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Offline Recovery Test ===")
print()

-- ============================================================================
-- PART 1: Delayed message delivery to an already-joined peer
-- ============================================================================

h.section(1, "Creating 3 networks and connecting")
local nets = h.create_networks(3)
local a = nets[1]
local b = nets[2]
local c = nets[3]
a:set_display_name("Alice")
b:set_display_name("Bob")
c:set_display_name("Carol")
h.connect_all(nets)
print("    3 networks started and connected")

h.section(2, "Alice creates realm, B joins before messages are sent")
local realm_a = a:create_realm("Recovery Test")
print("    Realm created: " .. realm_a:id():sub(1, 16) .. "...")

local invite = realm_a:invite_code()
local realm_b = b:join(invite)
print("    Bob joined realm")

-- Wait for membership to stabilize before sending
h.assert_eventually(function()
    return realm_a:member_count() >= 2
end, { timeout = 10, interval = 0.5, msg = "Alice should see Bob in member list" })
print("    Membership sync confirmed")

h.section(3, "Alice sends messages, Bob should see them all")
realm_a:send("Message 1 from Alice")
realm_a:send("Message 2 from Alice")
realm_a:send("Message 3 from Alice")
print("    Alice sent 3 messages")

h.assert_eventually(function()
    local msgs = realm_b:all_messages()
    return #msgs >= 3
end, { timeout = 15, interval = 0.5, msg = "Bob should receive all 3 messages from Alice" })

local b_msgs = realm_b:all_messages()
print("    Bob received " .. #b_msgs .. " message(s)")
indras.assert.true_(#b_msgs >= 3, "Bob should see all 3 messages from Alice")
print("    Delayed message delivery verified")

-- ============================================================================
-- PART 2: Document recovery via refresh
-- ============================================================================

h.section(4, "Testing document sync via CRDT refresh")

-- A writes a document while B is already in the realm
local doc_a = realm_a:document("recovery_doc")
doc_a:update({version = 1, status = "initial"})
print("    Alice wrote to 'recovery_doc': version=1, status='initial'")

-- B opens the same document and reads after a delay — refresh() drives sync
local doc_b = realm_b:document("recovery_doc")
h.assert_eventually(function()
    local data = doc_b:read()
    return type(data) == "table" and data.version == 1
end, { timeout = 15, interval = 0.5, msg = "Bob should recover Alice's document data" })

local b_doc_data = doc_b:read()
print("    Bob recovered document: version=" .. tostring(b_doc_data.version) .. ", status=" .. tostring(b_doc_data.status))
indras.assert.eq(b_doc_data.version, 1, "Bob should see version=1")
indras.assert.eq(b_doc_data.status, "initial", "Bob should see status='initial'")
print("    Document recovery via refresh verified")

-- ============================================================================
-- PART 3: Three-node message propagation
-- ============================================================================

h.section(5, "Carol joins, A/B/C exchange messages")

local realm_c = c:join(invite)
print("    Carol joined realm")

-- Wait for Carol's membership to be visible
h.assert_eventually(function()
    return realm_a:member_count() >= 3
end, { timeout = 10, interval = 0.5, msg = "Alice should see Carol in member list" })
print("    Three-node membership confirmed")

-- A sends messages
realm_a:send("From Alice: ping")
realm_a:send("From Alice: status ok")
print("    Alice sent 2 messages")

-- B sends messages
realm_b:send("From Bob: hello all")
realm_b:send("From Bob: ready")
print("    Bob sent 2 messages")

-- Carol should see all messages from A and B
h.assert_eventually(function()
    local msgs = realm_c:all_messages()
    return #msgs >= 4
end, { timeout = 15, interval = 0.5, msg = "Carol should see messages from both Alice and Bob" })

local c_msgs = realm_c:all_messages()
print("    Carol sees " .. #c_msgs .. " message(s)")
indras.assert.true_(#c_msgs >= 4, "Carol should see at least 4 messages (2 from Alice + 2 from Bob)")

-- Carol sends a message, A and B should see it
realm_c:send("From Carol: acknowledged")
print("    Carol sent a message")

h.assert_eventually(function()
    local msgs = realm_a:all_messages()
    for _, msg in ipairs(msgs) do
        if msg.content == "From Carol: acknowledged" then return true end
    end
    return false
end, { timeout = 15, interval = 0.5, msg = "Alice should see Carol's message" })

h.assert_eventually(function()
    local msgs = realm_b:all_messages()
    for _, msg in ipairs(msgs) do
        if msg.content == "From Carol: acknowledged" then return true end
    end
    return false
end, { timeout = 15, interval = 0.5, msg = "Bob should see Carol's message" })

print("    Alice and Bob both received Carol's message")
print("    Three-node message propagation verified")

h.section(6, "Stopping all networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Offline Recovery Test")
