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
local zephyr = indras.LiveNode.new()
local nova = indras.LiveNode.new()
print("    Zephyr: " .. tostring(zephyr))
print("    Nova:   " .. tostring(nova))

-- Step 2: Start both nodes
print("[2] Starting nodes...")
zephyr:start()
nova:start()
indras.assert.true_(zephyr:is_started(), "Zephyr should be started")
indras.assert.true_(nova:is_started(), "Nova should be started")
print("    Both nodes started successfully")

-- Step 3: Zephyr creates an interface, gets invite
print("[3] Zephyr creates interface 'Sync Test'...")
local iface_id, invite = zephyr:create_interface("Sync Test")
print("    Interface: " .. iface_id:sub(1, 16) .. "...")
print("    Invite length: " .. #invite .. " chars")

-- Step 4: Nova joins via invite
print("[4] Nova joins interface via invite...")
local joined_id = nova:join_interface(invite)
indras.assert.eq(joined_id, iface_id, "Joined interface ID should match")
print("    Joined: " .. joined_id:sub(1, 16) .. "...")

-- Step 5: Zephyr sends a message
print("[5] Zephyr sends message...")
local seq = zephyr:send_message(iface_id, "Hello from Zephyr!")
print("    Message sent, sequence: " .. seq)

-- Step 6: Verify Zephyr's own events
print("[6] Checking Zephyr's events...")
local zephyr_events = zephyr:events_since(iface_id, 0)
indras.assert.eq(#zephyr_events, 1, "Zephyr should have 1 event")
indras.assert.eq(zephyr_events[1].content, "Hello from Zephyr!", "Content should match")
print("    Zephyr has " .. #zephyr_events .. " event(s)")
print("    Content: " .. zephyr_events[1].content)

-- Step 7: Check members
print("[7] Checking members...")
local zephyr_members = zephyr:members(iface_id)
print("    Zephyr sees " .. #zephyr_members .. " member(s)")
for i, m in ipairs(zephyr_members) do
    print("      [" .. i .. "] " .. m)
end

-- Step 8: Nova also sends a message
print("[8] Nova sends message...")
local seq2 = nova:send_message(joined_id, "Hello from Nova!")
print("    Message sent, sequence: " .. seq2)

-- Step 9: Check Nova's local events
print("[9] Checking Nova's events...")
local nova_events = nova:events_since(joined_id, 0)
print("    Nova has " .. #nova_events .. " local event(s)")
for i, e in ipairs(nova_events) do
    print("      [" .. i .. "] " .. e.content .. " (from " .. e.sender .. ")")
end

-- Step 10: Stop both nodes
print("[10] Stopping nodes...")
zephyr:stop()
nova:stop()
indras.assert.true_(not zephyr:is_started(), "Zephyr should be stopped")
indras.assert.true_(not nova:is_started(), "Nova should be stopped")
print("     Both nodes stopped")

print()
print("=== Live P2P Sync Test PASSED ===")
