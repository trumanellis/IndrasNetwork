-- live_sync_engine_intentions.lua
--
-- Integration test: Intention lifecycle + CRDT sync between 2 nodes.
-- Verifies create, claim, verify, complete, and concurrent-edit merge.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_sync_engine_intentions.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Sync-Engine Intentions Test ===")
print()

-- Setup: Create 2 networks (A, B), start, connect
h.section(0, "Creating and connecting 2 networks")
local nets = h.create_networks(2)
local a = nets[1]
local b = nets[2]
a:set_display_name("A")
b:set_display_name("B")
h.connect_all(nets)
print("    2 networks started and connected")

-- A creates realm "Intentions Test"
h.section(1, "A creates realm")
local realm_a = a:create_realm("Intentions Test")
print("    Realm created: " .. realm_a:name())

-- B joins
h.section(2, "B joins realm")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
indras.assert.eq(realm_b:id(), realm_a:id(), "B's realm ID should match A's")
print("    B joined realm")

-- Allow membership to sync
indras.sleep(1)

-- ============================================================
-- 3a. Create + Sync Intention
-- ============================================================
h.section("3a", "Create + Sync Intention")
local intention_id = realm_a:create_intention("Help garden", "Weed the vegetable beds")
print("    A created intention: " .. intention_id:sub(1, 16) .. "...")

-- B should eventually see the intention
h.assert_eventually(function()
    local intentions = realm_b:read_intentions()
    for _, q in ipairs(intentions) do
        if q.title == "Help garden" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "B should see 'Help garden' intention from A" })
print("    B sees intention 'Help garden'")

-- ============================================================
-- 3b. Submit Proof + Sync
-- ============================================================
h.section("3b", "Submit Proof + Sync")
local b_id = b:id()
realm_a:submit_service_claim(intention_id, b_id)
print("    B submitted claim on A's intention")

-- A should see the claim
h.assert_eventually(function()
    local intentions = realm_a:read_intentions()
    for _, q in ipairs(intentions) do
        if q.id == intention_id and q.claim_count > 0 then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see 1 claim on intention" })
print("    A sees claim on intention")

-- ============================================================
-- 3c. Verify Claim + Complete
-- ============================================================
h.section("3c", "Verify Claim + Complete")
realm_a:verify_service_claim(intention_id, 0)
print("    A verified B's claim")

realm_a:complete_intention(intention_id)
print("    A completed intention")

-- B should see completed_at != nil
h.assert_eventually(function()
    local intentions = realm_b:read_intentions()
    for _, q in ipairs(intentions) do
        if q.id == intention_id and q.is_complete then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "B should see intention completed" })
print("    B sees intention is complete")

-- ============================================================
-- 3e. Concurrent Edit Merge
-- ============================================================
h.section("3e", "Concurrent Edit Merge")
-- A and B both create intentions independently (they're already connected,
-- but we create quickly enough that they'll merge via CRDT)
local id_from_a = realm_a:create_intention("Task from A", "Created by A")
print("    A created 'Task from A': " .. id_from_a:sub(1, 16) .. "...")
local id_from_b = realm_b:create_intention("Task from B", "Created by B")
print("    B created 'Task from B': " .. id_from_b:sub(1, 16) .. "...")

-- Both should eventually see both intentions (union merge)
h.assert_eventually(function()
    local intentions = realm_a:read_intentions()
    local found_a, found_b = false, false
    for _, q in ipairs(intentions) do
        if q.title == "Task from A" then found_a = true end
        if q.title == "Task from B" then found_b = true end
    end
    return found_a and found_b
end, { timeout = 10, interval = 0.5, msg = "A should see both intentions after merge" })
print("    A sees both intentions")

h.assert_eventually(function()
    local intentions = realm_b:read_intentions()
    local found_a, found_b = false, false
    for _, q in ipairs(intentions) do
        if q.title == "Task from A" then found_a = true end
        if q.title == "Task from B" then found_b = true end
    end
    return found_a and found_b
end, { timeout = 10, interval = 0.5, msg = "B should see both intentions after merge" })
print("    B sees both intentions")

-- Cleanup
h.section(4, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Sync-Engine Intentions Test")
