-- live_late_joiner_sync.lua
--
-- E2E test: Anti-entropy catch-up for a late-joining node.
--
-- Exercises: chain creation on early nodes, delayed realm join,
-- CRDT tip sync propagating chain events to a late joiner,
-- and gap-fill via detect_gaps -> range sync.
--
-- Uses 3 live IndrasNetwork nodes (A, B, C) with QUIC transport.
-- C joins AFTER A has already created events, verifying catch-up.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_late_joiner_sync.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Late Joiner Sync Test ===")
print()

-- Step 1: Create 3 networks but only connect A and B
h.section(1, "Creating 3 networks, connecting only A and B")
local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
a:set_display_name("A")
b:set_display_name("B")
c:set_display_name("C")
-- Only connect A <-> B (C is isolated for now)
a:connect_to(b)
print("    3 networks started, only A <-> B connected")

-- Step 2: A creates realm, B joins
h.section(2, "A creates realm, B joins")
local realm_a = a:create_realm("LateJoiner")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
print("    A and B in realm: " .. realm_a:id():sub(1, 16) .. "...")

-- Step 3: Get PQ identities and member IDs
h.section(3, "Getting PQ identities and member IDs")
local pq_a = a:pq_identity()
local id_a = a:member_id()
local id_b = b:member_id()
print("    A: " .. id_a:sub(1, 16) .. "...")
print("    B: " .. id_b:sub(1, 16) .. "...")

-- Deterministic artifact IDs for attention targets
local target_1 = string.rep("c1", 32)
local target_2 = string.rep("c2", 32)
local target_3 = string.rep("c3", 32)
local target_4 = string.rep("c4", 32)

-- Step 4: A creates genesis + 3 switches (4 events total)
h.section(4, "A creates genesis + 3 switches (seq=0..3)")
local genesis, author_state = realm_a:create_genesis_event(target_1, id_a, pq_a)
indras.assert.eq(genesis:seq(), 0, "Genesis should have seq=0")
print("    Genesis: seq=0 hash=" .. genesis:event_hash_hex():sub(1, 16) .. "...")

local ev1, author_state = realm_a:switch_attention_conserved(
    target_1, target_2, id_a, pq_a, author_state
)
indras.assert.eq(ev1:seq(), 1, "Switch 1 should have seq=1")
print("    Switch 1: seq=1 (-> target_2)")

local ev2, author_state = realm_a:switch_attention_conserved(
    target_2, target_3, id_a, pq_a, author_state
)
indras.assert.eq(ev2:seq(), 2, "Switch 2 should have seq=2")
print("    Switch 2: seq=2 (-> target_3)")

local ev3, author_state = realm_a:switch_attention_conserved(
    target_3, target_4, id_a, pq_a, author_state
)
indras.assert.eq(ev3:seq(), 3, "Switch 3 should have seq=3")
print("    Switch 3: seq=3 (-> target_4)")

-- Verify locally on A
local chain_a = realm_a:chain_events_for(id_a)
indras.assert.eq(#chain_a, 4, "A should have 4 chain events locally")
print("    A's local chain: 4 events (seq 0..3)")

-- Step 5: Wait for A's chain to sync to B (baseline sync)
h.section(5, "Waiting for A's chain to sync to B (baseline)")
h.assert_eventually(function()
    local chain = realm_b:chain_events_for(id_a)
    return #chain >= 4
end, { timeout = 15, interval = 0.5, msg = "A's 4 events should sync to B" })
local chain_a_on_b = realm_b:chain_events_for(id_a)
indras.assert.eq(#chain_a_on_b, 4, "B should see 4 chain events for A")
print("    B sees A's 4 chain events (baseline sync works)")

-- Step 6: C joins the realm LATE (after events exist)
h.section(6, "C joins realm late, connecting to A and B")
a:connect_to(c)
b:connect_to(c)
local realm_c = c:join(invite)
local id_c = c:member_id()
print("    C connected and joined realm: " .. id_c:sub(1, 16) .. "...")

-- Step 7: Wait for A's full chain to sync to C via anti-entropy
h.section(7, "Waiting for A's chain to sync to C (late joiner catch-up)")
h.assert_eventually(function()
    local chain = realm_c:chain_events_for(id_a)
    return #chain >= 4
end, { timeout = 20, interval = 0.5, msg = "A's 4 events should sync to C via anti-entropy" })
local chain_a_on_c = realm_c:chain_events_for(id_a)
indras.assert.eq(#chain_a_on_c, 4, "C should see 4 chain events for A")
print("    C sees A's 4 chain events (late joiner caught up!)")

-- Step 8: Verify seq ordering and chain integrity on C
h.section(8, "Verify seq ordering and chain integrity on C")
for i, cev in ipairs(chain_a_on_c) do
    indras.assert.eq(cev.seq, i - 1, "C: event " .. i .. " should have seq=" .. (i - 1))
end
print("    C confirms correct seq ordering 0..3")

-- Verify hash chaining: each event's hash differs from the previous
for i = 2, #chain_a_on_c do
    indras.assert.ne(chain_a_on_c[i].hash, chain_a_on_c[i - 1].hash,
        "Event " .. i .. " hash should differ from event " .. (i - 1))
end
print("    C confirms distinct hashes for each event")

-- Step 9: Stop all
h.section(9, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Late Joiner Sync Test")
