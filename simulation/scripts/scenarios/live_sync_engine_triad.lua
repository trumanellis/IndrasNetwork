-- live_sync_engine_triad.lua
--
-- Triad test: 3-node Intention lifecycle with intermittent connectivity.
-- Verifies store-and-forward propagation: changes made while a node is
-- disconnected are delivered when it reconnects via CRDT sync.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_sync_engine_triad.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Sync-Engine Triad Test ===")
print()

-- ============================================================
-- 1. Setup: Create 3 networks (A, B, C), start, connect
-- ============================================================
h.section(1, "Creating and connecting 3 networks")
local nets = h.create_networks(3)
local a = nets[1]
local b = nets[2]
local c = nets[3]
a:set_display_name("A")
b:set_display_name("B")
c:set_display_name("C")
h.connect_all(nets)
print("    3 networks started and connected")

-- A creates realm, B and C join
h.section(2, "A creates realm, B and C join")
local realm_a = a:create_realm("Triad Test")
print("    Realm created: " .. realm_a:name())

local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)
indras.assert.eq(realm_b:id(), realm_a:id(), "B's realm ID should match A's")
indras.assert.eq(realm_c:id(), realm_a:id(), "C's realm ID should match A's")
print("    B and C joined realm")

-- Allow membership to sync
indras.sleep(1)

local a_id = a:id()
local b_id = b:id()
local c_id = c:id()

-- ============================================================
-- 3. Baseline: All nodes see intention created while connected
-- ============================================================
h.section(3, "Baseline: Create intention with full connectivity")
local intention_id = realm_a:create_intention("Community garden", "Plant vegetables together")
print("    A created intention: " .. intention_id:sub(1, 16) .. "...")

h.assert_eventually(function()
    local qs = realm_b:read_intentions()
    for _, q in ipairs(qs) do
        if q.title == "Community garden" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "B should see intention" })

h.assert_eventually(function()
    local qs = realm_c:read_intentions()
    for _, q in ipairs(qs) do
        if q.title == "Community garden" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "C should see intention" })
print("    All 3 nodes see the intention")

-- ============================================================
-- 4. Isolate C, then perform blessing flow between A and B
-- ============================================================
h.section(4, "Isolate C, perform A↔B blessing flow")

h.isolate(c, nets)
print("    C disconnected from A and B")

-- A focuses attention on the intention
realm_a:focus_attention(intention_id)
print("    A focused attention on intention")
indras.sleep(1)

-- B submits claim
realm_b:submit_service_claim(intention_id, b_id)
print("    B submitted claim")

-- A verifies B's claim
h.assert_eventually(function()
    local qs = realm_a:read_intentions()
    for _, q in ipairs(qs) do
        if q.id == intention_id and q.claim_count > 0 then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see B's claim" })

realm_a:verify_service_claim(intention_id, 0)
print("    A verified B's claim")

-- A blesses B using unblessed attention events
local indices = realm_a:unblessed_event_indices(intention_id)
assert(#indices > 0, "A should have unblessed attention events")
local blessing_id = realm_a:bless_claim(intention_id, b_id, indices)
print("    A blessed B's claim: " .. blessing_id:sub(1, 16) .. "...")

-- B should have a token
h.assert_eventually(function()
    local tokens = realm_b:read_tokens()
    return #tokens > 0
end, { timeout = 10, interval = 0.5, msg = "B should have a token after blessing" })
print("    B has a token of gratitude")

-- B pledges the token
local b_tokens = realm_b:read_tokens()
local token_id = b_tokens[1].id
realm_b:pledge_token(token_id, intention_id)
print("    B pledged token to intention")

-- Confirm C still sees nothing new (it's isolated)
-- C should only see the original intention, no claims, no blessings
local c_qs = realm_c:read_intentions()
local c_saw_claim = false
for _, q in ipairs(c_qs) do
    if q.id == intention_id and q.claim_count > 0 then c_saw_claim = true end
end
assert(not c_saw_claim, "C should NOT see B's claim while disconnected")
print("    Confirmed: C sees no claims while isolated")

-- ============================================================
-- 5. Reconnect C — verify store-and-forward catches it up
-- ============================================================
h.section(5, "Reconnect C — store-and-forward propagation")

h.rejoin(c, nets)
print("    C reconnected to A and B")

-- C should eventually see everything that happened while offline:
-- 1. The claim on the intention
h.assert_eventually(function()
    local qs = realm_c:read_intentions()
    for _, q in ipairs(qs) do
        if q.id == intention_id and q.claim_count > 0 then return true end
    end
    return false
end, { timeout = 15, interval = 0.5, msg = "C should see B's claim after reconnect" })
print("    C sees B's claim (store-and-forward worked for intentions)")

-- 2. The blessing
h.assert_eventually(function()
    local blessings = realm_c:read_blessings(intention_id, b_id)
    return #blessings > 0
end, { timeout = 15, interval = 0.5, msg = "C should see blessing after reconnect" })
print("    C sees the blessing (store-and-forward worked for blessings)")

-- 3. The pledged token on the intention
h.assert_eventually(function()
    local pledged = realm_c:intention_pledged_tokens(intention_id)
    return #pledged > 0
end, { timeout = 15, interval = 0.5, msg = "C should see pledged token after reconnect" })
print("    C sees pledged token (store-and-forward worked for tokens)")

-- ============================================================
-- 6. Isolate B, then C does work — B catches up later
-- ============================================================
h.section(6, "Isolate B, C does work, B catches up")

-- Create a second intention for C's work
local intention2_id = realm_a:create_intention("Fix fence", "Repair the garden fence")
print("    A created second intention: " .. intention2_id:sub(1, 16) .. "...")

h.assert_eventually(function()
    local qs = realm_c:read_intentions()
    for _, q in ipairs(qs) do
        if q.title == "Fix fence" then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "C should see second intention" })

-- Now isolate B
h.isolate(b, nets)
print("    B disconnected from A and C")

-- A focuses attention on the second intention
realm_a:focus_attention(intention2_id)
indras.sleep(1)

-- C submits claim on second intention
realm_c:submit_service_claim(intention2_id, c_id)
print("    C submitted claim on second intention")

-- A verifies C's claim
h.assert_eventually(function()
    local qs = realm_a:read_intentions()
    for _, q in ipairs(qs) do
        if q.id == intention2_id and q.claim_count > 0 then return true end
    end
    return false
end, { timeout = 10, interval = 0.5, msg = "A should see C's claim" })

realm_a:verify_service_claim(intention2_id, 0)
print("    A verified C's claim")

-- A blesses C
local indices2 = realm_a:unblessed_event_indices(intention2_id)
assert(#indices2 > 0, "A should have unblessed events for second intention")
realm_a:bless_claim(intention2_id, c_id, indices2)
print("    A blessed C's claim")

-- C should have a token
h.assert_eventually(function()
    local tokens = realm_c:read_tokens()
    return #tokens > 0
end, { timeout = 10, interval = 0.5, msg = "C should have a token" })
print("    C has a token of gratitude")

-- Reconnect B
h.rejoin(b, nets)
print("    B reconnected to A and C")

-- B should catch up: see second intention's claim, blessing, and C's token
h.assert_eventually(function()
    local qs = realm_b:read_intentions()
    for _, q in ipairs(qs) do
        if q.id == intention2_id and q.claim_count > 0 then return true end
    end
    return false
end, { timeout = 15, interval = 0.5, msg = "B should see C's claim on second intention" })
print("    B sees C's claim (B's store-and-forward works)")

h.assert_eventually(function()
    local blessings = realm_b:read_blessings(intention2_id, c_id)
    return #blessings > 0
end, { timeout = 15, interval = 0.5, msg = "B should see C's blessing after reconnect" })
print("    B sees C's blessing (B's store-and-forward works)")

-- ============================================================
-- 7. Three-way convergence check
-- ============================================================
h.section(7, "Three-way convergence")

-- All three nodes should see the same intention state
h.assert_eventually(function()
    local a_qs = realm_a:read_intentions()
    local b_qs = realm_b:read_intentions()
    local c_qs = realm_c:read_intentions()
    return #a_qs == #b_qs and #b_qs == #c_qs
end, { timeout = 15, interval = 0.5, msg = "All 3 nodes should have same intention count" })

local a_count = #realm_a:read_intentions()
local b_count = #realm_b:read_intentions()
local c_count = #realm_c:read_intentions()
print("    Intention counts — A:" .. a_count .. " B:" .. b_count .. " C:" .. c_count)
indras.assert.eq(a_count, b_count, "A and B should have same intention count")
indras.assert.eq(b_count, c_count, "B and C should have same intention count")
print("    All 3 nodes converged to same state")

-- Cleanup
h.section(8, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Sync-Engine Triad Test")
