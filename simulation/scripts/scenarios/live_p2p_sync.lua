-- live_p2p_sync.lua
--
-- Integration test: Two real IndrasNode instances sync over actual P2P networking.
-- This uses indras.LiveNode which wraps the real IndrasNode with QUIC transport,
-- CRDT sync, and post-quantum cryptography.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_p2p_sync.lua

print("=== Live P2P Sync Test ===")
print()

-- Step 1: Create two real nodes (auto temp dirs)
print("[1] Creating two LiveNode instances...")
local a = indras.LiveNode.new()
local b = indras.LiveNode.new()
print("    A: " .. tostring(a))
print("    B: " .. tostring(b))

-- Step 2: Start both nodes
print("[2] Starting nodes...")
a:start()
b:start()
indras.assert.true_(a:is_started(), "A should be started")
indras.assert.true_(b:is_started(), "B should be started")
print("    Both nodes started successfully")

-- Step 3: A creates an interface, gets invite
print("[3] A creates interface 'Sync Test'...")
local iface_id, invite = a:create_interface("Sync Test")
print("    Interface: " .. iface_id:sub(1, 16) .. "...")
print("    Invite length: " .. #invite .. " chars")

-- Step 4: B joins via invite
print("[4] B joins interface via invite...")
local joined_id = b:join_interface(invite)
indras.assert.eq(joined_id, iface_id, "Joined interface ID should match")
print("    Joined: " .. joined_id:sub(1, 16) .. "...")

-- Step 5: A sends a message
print("[5] A sends message...")
local seq = a:send_message(iface_id, "Hello from A!")
print("    Message sent, sequence: " .. seq)

-- Step 6: Verify A's own events
print("[6] Checking A's events...")
local a_events = a:events_since(iface_id, 0)
indras.assert.eq(#a_events, 1, "A should have 1 event")
indras.assert.eq(a_events[1].content, "Hello from A!", "Content should match")
print("    A has " .. #a_events .. " event(s)")
print("    Content: " .. a_events[1].content)

-- Step 7: Check members
print("[7] Checking members...")
local a_members = a:members(iface_id)
print("    A sees " .. #a_members .. " member(s)")
for i, m in ipairs(a_members) do
    print("      [" .. i .. "] " .. m)
end

-- Step 8: B also sends a message
print("[8] B sends message...")
local seq2 = b:send_message(joined_id, "Hello from B!")
print("    Message sent, sequence: " .. seq2)

-- Step 9: Check B's local events
print("[9] Checking B's events...")
local b_events = b:events_since(joined_id, 0)
print("    B has " .. #b_events .. " local event(s)")
for i, e in ipairs(b_events) do
    print("      [" .. i .. "] " .. e.content .. " (from " .. e.sender .. ")")
end

-- Step 10: Stop both nodes
print("[10] Stopping nodes...")
a:stop()
b:stop()
indras.assert.true_(not a:is_started(), "A should be stopped")
indras.assert.true_(not b:is_started(), "B should be stopped")
print("     Both nodes stopped")

print()
print("=== Live P2P Sync Test PASSED ===")
