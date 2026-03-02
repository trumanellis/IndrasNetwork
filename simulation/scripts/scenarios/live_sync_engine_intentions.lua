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

-- ============================================================
-- 4a. Attention Focus + Sync
-- ============================================================
h.section("4a", "Attention Focus + Sync")

-- Create a fresh intention for the full lifecycle
local lifecycle_id = realm_a:create_intention("Lifecycle test", "Full lifecycle with blessing and tokens")
print("    A created 'Lifecycle test': " .. lifecycle_id:sub(1, 16) .. "...")

-- A focuses attention on it
realm_a:focus_attention(lifecycle_id)
print("    A focused attention on intention")

h.assert_eventually(function()
    local events = realm_a:read_attention()
    for _, e in ipairs(events) do
        if e.intention_id == lifecycle_id and e.member == a:id() then
            return true
        end
    end
    return false
end, { timeout = 5, interval = 0.5, msg = "A's attention should include intention focus" })
print("    A's attention includes intention focus")

-- Test clear
realm_a:clear_attention()
print("    A cleared attention")

h.assert_eventually(function()
    local events = realm_a:read_attention()
    local last_a = nil
    for _, e in ipairs(events) do
        if e.member == a:id() then last_a = e end
    end
    return last_a ~= nil and last_a.intention_id == nil
end, { timeout = 5, interval = 0.5, msg = "A's attention should be cleared" })
print("    A's attention cleared")

-- ============================================================
-- 4b. Full Blessing Flow
-- ============================================================
h.section("4b", "Full Blessing Flow")

-- A re-focuses on the lifecycle intention (generates attention events)
realm_a:focus_attention(lifecycle_id)
print("    A focused attention on lifecycle intention")

-- Small pause to let the attention event settle
indras.sleep(1)

-- B submits a claim on the lifecycle intention
realm_b:submit_service_claim(lifecycle_id, b_id)
print("    B submitted claim on lifecycle intention")

-- A verifies B's claim
h.assert_eventually(function()
    local intentions = realm_a:read_intentions()
    for _, q in ipairs(intentions) do
        if q.id == lifecycle_id and q.claim_count > 0 then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see claim on lifecycle intention" })

realm_a:verify_service_claim(lifecycle_id, 0)
print("    A verified B's claim")

-- A gets unblessed event indices for the lifecycle intention
local indices = realm_a:unblessed_event_indices(lifecycle_id)
print("    A has " .. #indices .. " unblessed event indices")
assert(#indices > 0, "Should have unblessed attention events")

-- A blesses B's claim
local blessing_id = realm_a:bless_claim(lifecycle_id, b_id, indices)
print("    A blessed B's claim: " .. blessing_id:sub(1, 16) .. "...")

-- B should now have a token
h.assert_eventually(function()
    local tokens = realm_b:read_tokens()
    return #tokens > 0
end, { timeout = 10, interval = 0.5, msg = "B should have a token after blessing" })
print("    B has a token of gratitude")

-- Verify blessing is recorded
local blessings = realm_a:read_blessings(lifecycle_id, b_id)
indras.assert.eq(#blessings, 1, "Should have 1 blessing")
print("    Blessing recorded correctly")

-- ============================================================
-- 4c. Token Actions
-- ============================================================
h.section("4c", "Token Actions")

-- B reads their tokens and picks the first one
local b_tokens = realm_b:read_tokens()
assert(#b_tokens > 0, "B should have tokens")
local token_id = b_tokens[1].id
print("    B's token: " .. token_id:sub(1, 16) .. "...")

-- B pledges the token to the lifecycle intention
realm_b:pledge_token(token_id, lifecycle_id)
print("    B pledged token to intention")

-- Verify intention has pledged tokens
h.assert_eventually(function()
    local pledged = realm_b:intention_pledged_tokens(lifecycle_id)
    return #pledged > 0
end, { timeout = 10, interval = 0.5, msg = "Intention should have pledged token" })
print("    Intention has 1 pledged token")

-- B releases token to A
local a_id = a:id()
realm_b:release_token(token_id, a_id)
print("    B released token to A")

-- A should now own the token
h.assert_eventually(function()
    local a_tokens = realm_a:read_tokens()
    for _, t in ipairs(a_tokens) do
        if t.id == token_id then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should own the released token" })
print("    A now owns the token")

-- Cleanup
h.section(5, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Sync-Engine Intentions Test")
