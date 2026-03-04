-- live_farewell_events.lua
--
-- E2E test: Chained farewell event (member departure with to=nil).
--
-- Exercises: switch_attention_conserved with nil destination,
-- chain_events_for verifying farewell event has to=nil,
-- and CRDT sync of farewell events between nodes.
--
-- Uses 3 live IndrasNetwork nodes (A, B, C) with QUIC transport.
--
-- Usage:
--   cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
--     -- simulation/scripts/scenarios/live_farewell_events.lua

package.path = package.path .. ";simulation/scripts/lib/?.lua"
local h = require("live_network_helpers")

print("=== Live Farewell Events Test ===")
print()

-- Step 1: Create 3 networks, start, connect all
h.section(1, "Creating and connecting 3 networks")
local nets = h.create_networks(3)
local a, b, c = nets[1], nets[2], nets[3]
a:set_display_name("A")
b:set_display_name("B")
c:set_display_name("C")
h.connect_all(nets)
print("    3 networks started and connected")

-- Step 2: A creates realm, B and C join
h.section(2, "A creates realm, B and C join")
local realm_a = a:create_realm("FarewellTest")
local invite = realm_a:invite_code()
local realm_b = b:join(invite)
local realm_c = c:join(invite)
print("    All 3 nodes in realm: " .. realm_a:id():sub(1, 16) .. "...")

-- Step 3: Get PQ identities and member IDs
h.section(3, "Getting PQ identities and member IDs")
local pq_a = a:pq_identity()
local id_a = a:member_id()
local id_b = b:member_id()
local id_c = c:member_id()
print("    A: " .. id_a:sub(1, 16) .. "...")
print("    B: " .. id_b:sub(1, 16) .. "...")
print("    C: " .. id_c:sub(1, 16) .. "...")

-- Deterministic artifact IDs
local target_1 = string.rep("f1", 32)
local target_2 = string.rep("f2", 32)

-- Step 4: A creates genesis (seq=0) + switch (seq=1)
h.section(4, "A creates genesis + switch (seq=0, seq=1)")
local genesis, author_state = realm_a:create_genesis_event(target_1, id_a, pq_a)
indras.assert.eq(genesis:seq(), 0, "Genesis should have seq=0")
print("    Genesis: seq=0 hash=" .. genesis:event_hash_hex():sub(1, 16) .. "...")

local switch_ev, author_state = realm_a:switch_attention_conserved(
    target_1, target_2, id_a, pq_a, author_state
)
indras.assert.eq(switch_ev:seq(), 1, "Switch should have seq=1")
print("    Switch: seq=1 (-> target_2)")

-- Verify chain locally: 2 events
local chain_pre = realm_a:chain_events_for(id_a)
indras.assert.eq(#chain_pre, 2, "A should have 2 chain events before farewell")
indras.assert.eq(chain_pre[2].to, target_2, "Second event should point to target_2")
print("    A's chain: 2 events, last to=target_2")

-- Step 5: A creates farewell event (to=nil)
h.section(5, "A creates farewell event (to=nil, seq=2)")
local farewell_ev, author_state = realm_a:switch_attention_conserved(
    target_2, nil, id_a, pq_a, author_state
)
indras.assert.eq(farewell_ev:seq(), 2, "Farewell should have seq=2")
print("    Farewell: seq=2 hash=" .. farewell_ev:event_hash_hex():sub(1, 16) .. "...")

-- Step 6: Verify farewell locally on A via chain events
h.section(6, "Verify farewell in chain on A")
local chain_a = realm_a:chain_events_for(id_a)
indras.assert.eq(#chain_a, 3, "A should have 3 chain events (genesis, switch, farewell)")
print("    A's chain has 3 events (seq 0, 1, 2)")

-- Verify seq ordering
for i, cev in ipairs(chain_a) do
    indras.assert.eq(cev.seq, i - 1, "Event " .. i .. " should have seq=" .. (i - 1))
end
print("    Correct seq ordering 0..2")

-- First event (genesis) should have a `to` target
indras.assert.eq(chain_a[1].to, target_1, "Genesis should have to=target_1")
print("    Event 0 (genesis): to=target_1")

-- Second event (switch) should have to=target_2
indras.assert.eq(chain_a[2].to, target_2, "Switch should have to=target_2")
print("    Event 1 (switch): to=target_2")

-- Last event (farewell) should have to=nil
indras.assert.eq(chain_a[3].to, nil, "Farewell event should have to=nil")
print("    Event 2 (farewell): to=nil (departure confirmed)")

-- Step 7: Wait for farewell chain to sync to B
h.section(7, "Waiting for farewell chain to sync to B")
h.assert_eventually(function()
    local chain = realm_b:chain_events_for(id_a)
    return #chain >= 3
end, { timeout = 15, interval = 0.5, msg = "A's farewell chain should sync to B" })
print("    B sees A's 3 chain events")

-- Step 8: Verify farewell event on B
h.section(8, "Verify farewell event on B")
local chain_a_on_b = realm_b:chain_events_for(id_a)
indras.assert.eq(#chain_a_on_b, 3, "B should see 3 chain events for A")

-- Verify seq ordering on B
for i, cev in ipairs(chain_a_on_b) do
    indras.assert.eq(cev.seq, i - 1, "B: event " .. i .. " should have seq=" .. (i - 1))
end
print("    B confirms seq ordering 0..2")

-- Verify farewell has to=nil on B
indras.assert.eq(chain_a_on_b[3].to, nil, "B should see farewell with to=nil")
print("    B sees farewell event (to=nil)")

-- Verify from field on farewell
indras.assert.eq(chain_a_on_b[3].from, target_2, "Farewell should have from=target_2")
print("    B sees farewell from=target_2 (correct)")

-- Step 9: Wait for farewell chain to sync to C
h.section(9, "Waiting for farewell chain to sync to C")
h.assert_eventually(function()
    local chain = realm_c:chain_events_for(id_a)
    if #chain < 3 then return false end
    return chain[3].to == nil
end, { timeout = 15, interval = 0.5, msg = "A's farewell should sync to C" })
local chain_a_on_c = realm_c:chain_events_for(id_a)
indras.assert.eq(#chain_a_on_c, 3, "C should see 3 chain events for A")
indras.assert.eq(chain_a_on_c[3].to, nil, "C should see farewell with to=nil")
print("    C sees A's farewell (3 events, last to=nil)")

-- Step 10: Stop all
h.section(10, "Stopping networks")
h.stop_all(nets)
print("    All networks stopped")

print()
h.pass("Live Farewell Events Test")
