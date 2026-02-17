-- live_direct_connect.lua
--
-- Integration test: DM (direct message) realms and connect_by_code.
-- Three real Network instances (Alice, Bob, Charlie) exercise peer-to-peer
-- DM channels: direct connect, bidirectional messaging, and code-based connect.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_direct_connect.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Direct Connect Test ===")
print()

-- Step 1: Create 3 networks, start, connect all
h.section(1, "Creating and starting 3 networks")
local nets = h.create_networks(3)
local a = nets[1]
local b = nets[2]
local c = nets[3]
h.connect_all(nets)
print("    Networks started and connected")

-- Step 2: Set display names
h.section(2, "Setting display names")
a:set_display_name("Alice")
b:set_display_name("Bob")
c:set_display_name("Charlie")
print("    Alice:   " .. a:id():sub(1, 16) .. "...")
print("    Bob:     " .. b:id():sub(1, 16) .. "...")
print("    Charlie: " .. c:id():sub(1, 16) .. "...")

local a_id = a:id()
local b_id = b:id()

-- Step 3: A connects to B → gets a DM realm
h.section(3, "Alice connects to Bob (DM realm)")
local dm_a = a:connect(b_id)
indras.assert.true_(dm_a ~= nil, "A:connect(b_id) should return a DM realm")
local dm_a_id = dm_a:id()
indras.assert.true_(#dm_a_id > 0, "DM realm id should be non-empty")
print("    DM realm id: " .. dm_a_id:sub(1, 16) .. "...")

-- Step 4: A sends a message in the DM realm
h.section(4, "Alice sends private message to Bob")
dm_a:send("Private message to Bob")
print("    Message sent")

-- Step 5: Sleep for sync
h.section(5, "Sleeping for sync")
indras.sleep(0.5)
print("    Sync wait complete")

-- Step 6: B connects to A → should get the same DM realm
h.section(6, "Bob connects to Alice (same DM realm)")
local dm_b = b:connect(a_id)
indras.assert.true_(dm_b ~= nil, "B:connect(a_id) should return a DM realm")
local dm_b_id = dm_b:id()
indras.assert.eq(dm_b_id, dm_a_id, "Bob's DM realm id should match Alice's")
print("    Bob's DM realm id matches: " .. dm_b_id:sub(1, 16) .. "...")

-- Step 7: B reads messages — verify A's message is there
h.section(7, "Bob reads messages — verifying Alice's message arrived")
h.assert_eventually(function()
    local msgs = dm_b:all_messages()
    for _, msg in ipairs(msgs) do
        if msg.content == "Private message to Bob" then
            return true
        end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "Bob should see Alice's message" })
local b_msgs = dm_b:all_messages()
print("    Bob sees " .. #b_msgs .. " message(s)")
print("    Alice's message verified")

-- Step 8: B sends a reply
h.section(8, "Bob sends reply to Alice")
dm_b:send("Reply from Bob")
print("    Reply sent")

-- Step 9: Sleep, A reads, verify B's reply
h.section(9, "Alice reads Bob's reply")
h.assert_eventually(function()
    local msgs = dm_a:all_messages()
    for _, msg in ipairs(msgs) do
        if msg.content == "Reply from Bob" then
            return true
        end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "Alice should see Bob's reply" })
local a_msgs = dm_a:all_messages()
print("    Alice sees " .. #a_msgs .. " message(s)")
print("    Bob's reply verified")

-- Step 10: Test connect_by_code — A's identity_code, C connects by code
h.section(10, "Testing connect_by_code: Charlie connects to Alice by code")
local a_code = a:identity_code()
print("    Alice identity_code: " .. a_code)
indras.assert.true_(#a_code > 0, "Alice's identity code should be non-empty")
local dm_c = c:connect_by_code(a_code)
indras.assert.true_(dm_c ~= nil, "C:connect_by_code should return a DM realm")
local dm_c_id = dm_c:id()
indras.assert.true_(#dm_c_id > 0, "Charlie's DM realm id should be non-empty")
print("    Charlie's DM realm id: " .. dm_c_id:sub(1, 16) .. "...")

-- Step 11: C sends a message in that realm
h.section(11, "Charlie sends message to Alice")
dm_c:send("Connected by code!")
print("    Message sent")

-- Step 12: A gets DM realm with C and waits for message
h.section(12, "Alice reads Charlie's message")
-- A needs to get the DM realm with C
local c_id = c:id()
local dm_a_c = a:connect(c_id)
h.assert_eventually(function()
    local msgs = dm_a_c:all_messages()
    for _, msg in ipairs(msgs) do
        if msg.content == "Connected by code!" then
            return true
        end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "Alice should see Charlie's code-connected message" })
local a_c_msgs = dm_a_c:all_messages()
print("    Alice sees " .. #a_c_msgs .. " message(s) from Charlie")
print("    Charlie's 'Connected by code!' message verified")

-- Step 13: Stop all
h.section(13, "Stopping all networks")
h.stop_all(nets)
print("    All networks stopped")

h.pass("Live Direct Connect Test")
